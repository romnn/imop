use crate::headers::{AcceptEncoding, ContentCoding};
use anyhow::Result;
use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
pub use async_compression::Level;
use bytes::{Bytes, BytesMut};
use clap::Parser;
use futures_util::future::Either;
use futures_util::TryFuture;
use futures_util::{future, ready, stream, FutureExt, Stream, StreamExt, TryFutureExt};
use http_headers::{
    AcceptRanges, ContentEncoding, ContentLength, ContentRange, ContentType, Header, HeaderMap,
    HeaderMapExt, HeaderValue, IfModifiedSince, IfRange, IfUnmodifiedSince, LastModified, Range,
};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;
use tokio::io::AsyncSeekExt;
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::io::poll_read_buf;
use tokio_util::io::{ReaderStream, StreamReader};
use urlencoding::decode;
use warp::http::{StatusCode, Uri};
use warp::hyper;
use warp::reply::Response;
use warp::Future;
use warp::Rejection;
use warp::{Filter, Reply};

pub trait IsReject: fmt::Debug + Send + Sync {
    fn status(&self) -> StatusCode;
    fn into_response(&self) -> Response;
}

#[derive(Debug, Clone, Copy)]
pub enum CompressionAlgo {
    BR,
    DEFLATE,
    GZIP,
    NONE,
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        HeaderValue::from_static(match algo {
            CompressionAlgo::BR => "br",
            CompressionAlgo::DEFLATE => "deflate",
            CompressionAlgo::GZIP => "gzip",
            CompressionAlgo::NONE => "",
        })
    }
}

impl From<ContentCoding> for CompressionAlgo {
    #[inline]
    fn from(coding: ContentCoding) -> Self {
        match coding {
            ContentCoding::BROTLI => CompressionAlgo::BR,
            ContentCoding::COMPRESS => CompressionAlgo::GZIP,
            ContentCoding::DEFLATE => CompressionAlgo::DEFLATE,
            ContentCoding::GZIP => CompressionAlgo::GZIP,
            ContentCoding::IDENTITY => CompressionAlgo::NONE,
        }
        // HeaderValue::from_static(match algo {
        //     CompressionAlgo::BR => "br",
        //     CompressionAlgo::DEFLATE => "deflate",
        //     CompressionAlgo::GZIP => "gzip",
        // })
    }
}

#[pin_project]
#[derive(Debug)]
pub struct CompressableBody<S, E>
where
    E: std::error::Error,
    S: Stream<Item = Result<Bytes, E>>,
{
    #[pin]
    body: S,
}

impl<S, E> Stream for CompressableBody<S, E>
where
    E: std::error::Error,
    S: Stream<Item = Result<Bytes, E>>,
{
    type Item = std::io::Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::io::{Error, ErrorKind};

        let pin = self.project();
        S::poll_next(pin.body, cx).map_err(|_| Error::from(ErrorKind::InvalidData))
    }
}

impl From<hyper::Body> for CompressableBody<hyper::Body, hyper::Error> {
    fn from(body: hyper::Body) -> Self {
        CompressableBody { body }
    }
}

// // #[allow(missing_debug_implementations)]
// // #[derive(Clone, Copy)]
// // pub struct WithCompression<FN, F> {
// //     pub compress: Compression<FN>,
// //     pub filter: F,
// // }

// // impl<FN, F> Filter for WithCompression<FN, F>
// // where
// //     FN: Fn(Compressable) -> Response + Clone + Send,
// //     F: Filter + Clone + Send,
// //     F::Extract: Reply,
// //     F::Error: IsReject,
// // {
// //     type Extract = (Compressed,);
// //     type Error = F::Error;
// //     type Future = WithCompressionFuture<FN, F::Future>;

// //     fn filter(&self, _: Internal) -> Self::Future {
// //         WithCompressionFuture {
// //             compress: self.compress.clone(),
// //             future: self.filter.filter(Internal),
// //         }
// //     }
// // }

// #[allow(missing_debug_implementations)]
// #[pin_project]
// pub struct WithCompressionFuture<FN, F> {
//     // compress: Compression<FN>,
//     compress: FN,
//     #[pin]
//     future: F,
// }

// impl<FN, F> Future for WithCompressionFuture<FN, F>
// where
//     FN: Fn(Compressable) -> Response,
//     F: TryFuture,
//     F::Ok: Reply,
//     F::Error: IsReject,
// {
//     type Output = Result<(Compressed,), F::Error>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         let pin = self.as_mut().project();
//         let result = ready!(pin.future.try_poll(cx));
//         match result {
//             Ok(reply) => {
//                 let resp = (self.compress)(reply.into_response().into());
//                 Poll::Ready(Ok((Compressed(resp),)))
//             }
//             Err(reject) => Poll::Ready(Err(reject)),
//         }
//     }
// }

// #[derive(Clone, Copy, Debug)]
// pub struct Compression<F> {
//     func: F,
// }

// impl<FN, F> warp::filter::wrap::WrapSealed<F> for Compression<FN>
// where
//     FN: Fn(Compressable) -> Response + Clone + Send,
//     F: Filter + Clone + Send,
//     F::Extract: Reply,
//     F::Error: IsReject,
// {
//     type Wrapped = WithCompression<FN, F>;

//     fn wrap(&self, filter: F) -> Self::Wrapped {
//         WithCompression {
//             filter,
//             compress: self.clone(),
//         }
//     }
// }

#[allow(missing_debug_implementations)]
pub struct Compressed(pub(super) Response);
// pub enum Compressed(pub(super) Response);

impl Reply for Compressed {
    #[inline]
    fn into_response(self) -> Response {
        self.0
    }
}

#[derive(Debug)]
struct CompressionOptions {
    accept_encoding: Option<AcceptEncoding>,
}

#[derive(Debug)]
pub struct Compressable {
    pub body: CompressableBody<hyper::Body, hyper::Error>,
    pub head: http::response::Parts,
}

impl From<http::Response<hyper::Body>> for Compressable {
    fn from(resp: http::Response<hyper::Body>) -> Self {
        let (head, body) = resp.into_parts();
        // println!("{:?}", head.headers);
        // let accept_enc = head.headers.typed_get();
        Compressable {
            body: body.into(),
            head,
            // accept_enc,
        }
    }
}

fn compression_options() -> impl Filter<Extract = (CompressionOptions,), Error = Infallible> + Copy
{
    warp::header::headers_cloned().map(|headers: HeaderMap| CompressionOptions {
        accept_encoding: headers.typed_get(),
    })
}

impl Into<http::Response<hyper::Body>> for Compressable {
    fn into(self) -> http::Response<hyper::Body> {
        Response::from_parts(self.head, self.body.body)
    }
}

// fn deflate() -> Compression<impl Fn(Compressable) -> Response + Copy> {
//     // fn deflate() -> impl Fn(Compressable) -> Response + Copy {
//     let func = move |mut props: Compressable| {
//         let body = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
//             StreamReader::new(props.body),
//         )));
//         props.head.headers.append(
//             http::header::CONTENT_ENCODING,
//             CompressionAlgo::DEFLATE.into(),
//         );
//         props.head.headers.remove(http::header::CONTENT_LENGTH);
//         Response::from_parts(props.head, body)
//     };
//     // func
//     Compression { func }
// }
// trait Test: Filter<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static {}

// pub fn compression_filter<F, T>(
// ) -> impl Fn(F) -> (dyn Filter<Extract = dyn Reply, Error = Rejection> + Clone + Send + Sync + 'static)
// where
//     // U: Fn(F) -> T,
//     // U: Filter<Extract = dyn Reply, Error = Rejection> + Clone + Send + Sync + 'static,
//     F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
//     F::Extract: warp::Reply,
//     T: warp::Reply,
// {
//     |filter: F| warp::any().and(filter)
//     // .map(|r| disable_cache(r))
// }

// fn pls(reply: impl Reply) -> impl Reply {
//     let (head, body) = reply.into_response().into_parts();
//     let compressed = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
//         StreamReader::new(body),
//     )));
//     // props.head.headers.append(
//     //     http::header::CONTENT_ENCODING,
//     //     CompressionAlgo::DEFLATE.into(),
//     // );
//     // props.head.headers.remove(http::header::CONTENT_LENGTH);
//     Response::from_parts(head, compressed)
// }

// pub fn compress() -> impl crate::FilterClone<Extract = (Response,), Error = Rejection> {
//     // pub async fn compress(
//     // ) -> Result<impl Response, Rejection> {
//     warp::any().map(|file: crate::File| {
//         let (head, body) = file.into_response().into_parts();
//         let body: CompressableBody<hyper::Body, hyper::Error> = body.into();
//         let compressed = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
//             StreamReader::new(body),
//         )));
//         // props.head.headers.append(
//         //     http::header::CONTENT_ENCODING,
//         //     CompressionAlgo::DEFLATE.into(),
//         // );
//         // props.head.headers.remove(http::header::CONTENT_LENGTH);
//         Response::from_parts(head, compressed)
//     })
//     // println!("{:?}", path);
//     // todo: parse the compression options
//     // todo: look up if the file was already compressed, if so, get it from the disk cache
//     // todo: if not, compress and save to cache
//     // todo: serve the correct file
//     // serve_file(path, conditionals).await
// }

// pub fn compress() -> impl crate::FilterClone<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static
// // where
//     // F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
//     // F::Extract: warp::Reply,
//     // T: warp::Reply + 'static,
// {
//     warp::any().and_then(|reply: &dyn Reply| { reply })
//     // |reply| async move {
//     //     reply
//     // }
//     // warp::any().and(filter)
//     // let func = move |mut props: Compressable| {
//     //     let body = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
//     //         StreamReader::new(props.body),
//     //     )));
//     //     // props.head.headers.append(
//     //     //     http::header::CONTENT_ENCODING,
//     //     //     CompressionAlgo::DEFLATE.into(),
//     //     // );
//     //     // props.head.headers.remove(http::header::CONTENT_LENGTH);
//     //     Response::from_parts(props.head, body)
//     // };
//     // let compressor = Compression { func };

//     // WithCompressionFuture {
//     //     future: filter,
//     //     compress: func, // compressor.clone(),
//     // }
//     // .map(pls)
//     // .map(|r| disable_cache(r))
// }

pub fn auto<F, T>(
    quality: Level,
    // content_type_filter: Option<ContentTypeFilter>,
) -> impl Fn(F) -> warp::filters::BoxedFilter<(Compressed,)>
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
    // CT: Fn(Option<ContentType>) -> bool + Clone + Send + Sync + 'static,
{
    // content_type_filter.clone(),
    move |filter: F| compress::<F, T>(None, quality, None, filter).boxed()
}

// pub fn algo<F, T>(
//     algo: CompressionAlgo,
//     quality: Level,
// ) -> impl Fn(F) -> warp::filters::BoxedFilter<(Compressed,)>
// where
//     F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
//     F::Extract: warp::Reply,
//     // T: warp::Reply + 'static,
//     T: Into<Compressable>,
// {
//     move |filter: F| compress::<F, T>(algo, quality, filter).boxed()
// }

// pub fn brotli<'a, F, T, CT>(
pub fn brotli<F, T>(
    quality: Level,
    // content_type_filter: &'a Option<ContentTypeFilter<CT>>,
    // content_type_filter: Option<ContentTypeFilter>,
) -> impl Fn(F) -> warp::filters::BoxedFilter<(Compressed,)>
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
    // CT: Fn(Option<ContentType>) -> bool + Clone + Send + Sync + 'static,
    // T: warp::Reply + Into<Compressable> + 'static,
{
    move |filter: F| {
        compress::<F, T>(
            Some(CompressionAlgo::BR),
            quality,
            None,
            // content_type_filter.clone(),
            filter,
        )
        .boxed()
    }
}

// pub struct MyTest {}

// impl MyTest {
//     type Output =
//         impl Filter<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static;

//     fn compress(self, filter: ) -> Output
//     where
//         F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
//         F::Extract: warp::Reply,
//         T: warp::Reply,
//     {
//         warp::any().and(filter).map(|reply: T| {
//             let (mut head, body) = reply.into_response().into_parts();
//             let body: CompressableBody<hyper::Body, hyper::Error> = body.into();
//             // let compressed = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
//             //     StreamReader::new(body),
//             // )));
//             // head.headers.append(
//             //     http::header::CONTENT_ENCODING,
//             //     CompressionAlgo::DEFLATE.into(),
//             // );
//             let compressed = hyper::Body::wrap_stream(ReaderStream::new(BrotliEncoder::new(
//                 StreamReader::new(body),
//             )));
//             head.headers
//                 .append(http::header::CONTENT_ENCODING, CompressionAlgo::BR.into());

//             head.headers.remove(http::header::CONTENT_LENGTH);
//             Response::from_parts(head, compressed)
//             // reply
//         })
//         // .boxed()
//     }
// }

// trait NewTrait: Fn<(Option<ContentType>,)> + Clone {}

// #[derive(Clone, Copy)]
// enum ContentTypeFilter<CT: Clone> {
// #[derive(Clone, Copy)]
// trait CopyableFn: Fn(Option<&ContentType>) -> bool + Clone + Sized {}

#[derive(Clone)]
enum ContentTypeFilter {
    // Custom(impl Fn() -> bool),
    // Custom(() -> bool),
    // Custom(Box<dyn NewTrait -> bool + Send + Sync + 'static>),
    // Custom(Box<dyn Fn(Option<&ContentType>) -> bool + Clone + Send + Sync + 'static>),
    // Custom(Box<dyn CopyableFn + Send + Sync + 'static>),
    Custom(Arc<Box<dyn Fn(Option<&ContentType>) -> bool + Send + Sync + 'static>>),
    // Custom(CT),
}

fn compress<F, T>(
    algo: Option<CompressionAlgo>,
    quality: Level,
    // content_type_filter: &'a Option<ContentTypeFilter<CT>>,
    content_type_filter: Option<ContentTypeFilter>,
    filter: F,
    // ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static
) -> impl Filter<Extract = (Compressed,), Error = Rejection> + Clone + Send + Sync + 'static
// ) -> warp::filters::BoxedFilter<(impl Reply,)>
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    // CT: Fn(Option<ContentType>) -> bool + Clone + Send + Sync + 'static,
    // T: warp::Reply,
    // T: Compressable: From<T>,
    T: warp::Reply + 'static,
{
    warp::any()
        .and(filter)
        // .map(move |reply: T| reply.into_response().into())
        .and(compression_options())
        .map(move |reply: T, options: CompressionOptions| (reply.into_response().into(), options))
        .untuple_one()
        .map(
            move |mut compressable: Compressable, options: CompressionOptions| {
                // let (mut head, body) = reply.into_response().into_parts();
                // let body: CompressableBody<hyper::Body, hyper::Error> = body.into();
                // let compressed = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
                //     StreamReader::new(body),
                // )));
                // head.headers.append(
                //     http::header::CONTENT_ENCODING,
                //     CompressionAlgo::DEFLATE.into(),
                // );
                let prefered_encoding: Option<CompressionAlgo> = options
                    .accept_encoding
                    .as_ref()
                    .and_then(|header| header.prefered_encoding())
                    .map(|encoding| encoding.into());

                let test = match &content_type_filter {
                    Some(ContentTypeFilter::Custom(func)) => {
                        (func)(compressable.head.headers.typed_get().as_ref())
                    }
                    None => true,
                };
                // let algo = if content_type_filter
                //     // .as_ref()
                //     .map(|filter| match filter {
                //         ContentTypeFilter::Custom(func) => {
                //             (func)(compressable.head.headers.typed_get())
                //         }
                //     })
                //     .unwrap_or(true)
                // {
                //     algo.or(prefered_encoding)
                // } else {
                //     None
                // };
                let algo = algo.or(prefered_encoding);
                println!("algo: {:?}", algo);
                println!("headers: {:?}", compressable.head.headers);
                let stream = StreamReader::new(compressable.body);
                let encoded_stream: Box<dyn tokio::io::AsyncRead + Send + std::marker::Unpin> =
                    match algo {
                        Some(CompressionAlgo::BR) => {
                            Box::new(BrotliEncoder::with_quality(stream, quality))
                        }
                        Some(CompressionAlgo::DEFLATE) => {
                            Box::new(DeflateEncoder::with_quality(stream, quality))
                        }
                        Some(CompressionAlgo::GZIP) => {
                            Box::new(GzipEncoder::with_quality(stream, quality))
                        }
                        Some(CompressionAlgo::NONE) => Box::new(stream),
                        None => Box::new(stream),
                    };
                let compressed = hyper::Body::wrap_stream(ReaderStream::new(encoded_stream));
                // BrotliEncoder::with_quality(stream, quality)
                if let Some(algo) = algo {
                    compressable
                        .head
                        .headers
                        .append(http::header::CONTENT_ENCODING, algo.into());

                    compressable
                        .head
                        .headers
                        .remove(http::header::CONTENT_LENGTH);
                }
                // Box::<dyn Reply>::new(Response::from_parts(head, compressed))
                // Box::<dyn Reply>::new(Compressed(Response::from_parts(head, compressed)))
                // Box::new(Compressed(Response::from_parts(head, compressed)))
                Compressed(Response::from_parts(compressable.head, compressed))
                // reply
            },
        )
    // .boxed()
    // let func = move |mut props: Compressable| {
    //     let body = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
    //         StreamReader::new(props.body),
    //     )));
    //     // props.head.headers.append(
    //     //     http::header::CONTENT_ENCODING,
    //     //     CompressionAlgo::DEFLATE.into(),
    //     // );
    //     // props.head.headers.remove(http::header::CONTENT_LENGTH);
    //     Response::from_parts(props.head, body)
    // };
    // // let compressor = Compression { func };

    // WithCompressionFuture {
    //     future: filter,
    //     compress: func, // compressor.clone(),
    // }
    // .map(pls)
    // .map(|r| disable_cache(r))
}

// pub fn compression_filter_old<F, T>(
//     filter: F,
// ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static
// where
//     F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
//     F::Extract: warp::Reply,
//     T: warp::Reply + 'static,
// {
//     warp::any().and(filter)
//     // .map(pls)
//     // .map(|r| disable_cache(r))
// }

// pub fn auto() -> Compression<impl Fn(Compressable) -> Response + Copy> {
//     // fn auto() -> impl Fn(Compressable) -> Response + Copy {
//     let func = move |props: Compressable| {
//         // if let Some(ref header) = props.accept_enc {
//         //     if let Some(encoding) = header.prefered_encoding() {
//         //         // return (deflate().func)(props);
//         //         return (deflate())(props);
//         //         // return match encoding {
//         //         //     ContentEncoding::GZIP => (warp::compression::gzip().func)(props),
//         //         //     ContentEncoding::DEFLATE => (warp::compression::deflate().func)(props),
//         //         //     ContentEncoding::BROTLI => (warp::compression::brotli().func)(props),
//         //         //     _ => Response::from_parts(props.head, hyper::Body::wrap_stream(props.body)),
//         //         // };
//         //     }
//         // }
//         Response::from_parts(props.head, hyper::Body::wrap_stream(props.body))
//     };

//     Compression { func }
//     // func
// }
