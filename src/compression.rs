use anyhow::Result;
use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
use bytes::{Bytes, BytesMut};
use clap::Parser;
use futures_util::future::Either;
use futures_util::TryFuture;
use futures_util::{future, ready, stream, FutureExt, Stream, StreamExt, TryFutureExt};
use headers::{
    AcceptRanges, ContentEncoding, ContentLength, ContentRange, ContentType, Header, HeaderMapExt,
    HeaderValue, IfModifiedSince, IfRange, IfUnmodifiedSince, LastModified, Range,
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

trait IsReject: fmt::Debug + Send + Sync {
    fn status(&self) -> StatusCode;
    fn into_response(&self) -> Response;
}

// #[cfg(feature = "compression")]
// pub enum CompressionAlgo {
//     BR,
//     DEFLATE,
//     GZIP,
// }

// impl From<CompressionAlgo> for HeaderValue {
//     #[inline]
//     fn from(algo: CompressionAlgo) -> Self {
//         HeaderValue::from_static(match algo {
//             #[cfg(feature = "compression-brotli")]
//             CompressionAlgo::BR => "br",
//             #[cfg(feature = "compression-gzip")]
//             CompressionAlgo::DEFLATE => "deflate",
//             #[cfg(feature = "compression-gzip")]
//             CompressionAlgo::GZIP => "gzip",
//         })
//     }
// }

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
// //     FN: Fn(CompressionProps) -> Response + Clone + Send,
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

#[allow(missing_debug_implementations)]
#[pin_project]
pub struct WithCompressionFuture<FN, F> {
    compress: Compression<FN>,
    #[pin]
    future: F,
}

impl<FN, F> Future for WithCompressionFuture<FN, F>
where
    FN: Fn(CompressionProps) -> Response,
    F: TryFuture,
    F::Ok: Reply,
    F::Error: IsReject,
{
    type Output = Result<(Compressed,), F::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = self.as_mut().project();
        let result = ready!(pin.future.try_poll(cx));
        match result {
            Ok(reply) => {
                let resp = (self.compress.func)(reply.into_response().into());
                Poll::Ready(Ok((Compressed(resp),)))
            }
            Err(reject) => Poll::Ready(Err(reject)),
        }
    }
}

// #[derive(Clone, Copy, Debug)]
// pub struct Compression<F> {
//     func: F,
// }

// impl<FN, F> warp::filter::wrap::WrapSealed<F> for Compression<FN>
// where
//     FN: Fn(CompressionProps) -> Response + Clone + Send,
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

#[derive(Debug)]
struct CompressionProps {
    pub body: CompressableBody<hyper::Body, hyper::Error>,
    pub head: http::response::Parts,
}

impl From<http::Response<hyper::Body>> for CompressionProps {
    fn from(resp: http::Response<hyper::Body>) -> Self {
        let (head, body) = resp.into_parts();
        CompressionProps {
            body: body.into(),
            head,
        }
    }
}

// fn deflate() -> Compression<impl Fn(CompressionProps) -> Response + Copy> {
//     // fn deflate() -> impl Fn(CompressionProps) -> Response + Copy {
//     let func = move |mut props: CompressionProps| {
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

pub fn compress<F, T>(
    filter: F,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
{
    // warp::any().and(filter)
    let func = move |mut props: CompressionProps| {
        let body = hyper::Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(
            StreamReader::new(props.body),
        )));
        // props.head.headers.append(
        //     http::header::CONTENT_ENCODING,
        //     CompressionAlgo::DEFLATE.into(),
        // );
        // props.head.headers.remove(http::header::CONTENT_LENGTH);
        Response::from_parts(props.head, body)
    };
    let compressor = Compression { func };

    WithCompressionFuture {
        filter,
        compress: compressor.clone(),
    }
    // .map(pls)
    // .map(|r| disable_cache(r))
}

pub fn compression_filter_old<F, T>(
    filter: F,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone + Send + Sync + 'static
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
{
    warp::any().and(filter)
    // .map(pls)
    // .map(|r| disable_cache(r))
}

// pub fn auto() -> Compression<impl Fn(CompressionProps) -> Response + Copy> {
//     // fn auto() -> impl Fn(CompressionProps) -> Response + Copy {
//     let func = move |props: CompressionProps| {
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
