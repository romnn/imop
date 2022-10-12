pub use super::content_type_filter::{CompressContentType, ContentTypeFilter};
use super::headers::{AcceptEncoding, ContentCoding};
pub use async_compression::Level;
use bytes::Bytes;
use futures::Stream;
use http_headers::{ContentType, HeaderMap, HeaderMapExt, HeaderValue};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_util::io::{ReaderStream, StreamReader};
use warp::{hyper, reply, Filter, Rejection};

#[derive(Debug, Clone, Copy)]
pub enum Algorithm {
    BR,
    DEFLATE,
    GZIP,
}

impl From<Algorithm> for HeaderValue {
    #[inline]
    fn from(algo: Algorithm) -> Self {
        HeaderValue::from_static(match algo {
            Algorithm::BR => "br",
            Algorithm::DEFLATE => "deflate",
            Algorithm::GZIP => "gzip",
        })
    }
}

impl From<ContentCoding> for Option<Algorithm> {
    #[inline]
    fn from(coding: ContentCoding) -> Self {
        match coding {
            ContentCoding::BROTLI => Some(Algorithm::BR),
            ContentCoding::DEFLATE => Some(Algorithm::DEFLATE),
            ContentCoding::COMPRESS | ContentCoding::GZIP => Some(Algorithm::GZIP),
            ContentCoding::IDENTITY => None,
        }
    }
}

#[pin_project::pin_project]
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

#[allow(missing_debug_implementations)]
pub struct Compressed(pub(super) reply::Response);

impl warp::Reply for Compressed {
    #[inline]
    fn into_response(self) -> reply::Response {
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
        Compressable {
            body: body.into(),
            head,
        }
    }
}

fn compression_options() -> impl Filter<Extract = (CompressionOptions,), Error = Infallible> + Copy
{
    warp::header::headers_cloned().map(|headers: HeaderMap| CompressionOptions {
        accept_encoding: headers.typed_get(),
    })
}

impl From<Compressable> for http::Response<hyper::Body> {
    fn from(c: Compressable) -> http::Response<hyper::Body> {
        reply::Response::from_parts(c.head, c.body.body)
    }
}

pub fn auto<F, T, CT>(
    quality: Level,
    content_type_filter: CT,
) -> impl Fn(F) -> warp::filters::BoxedFilter<(Compressed,)>
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
    CT: ContentTypeFilter + Sync + Send + 'static,
{
    let ctf = Arc::new(content_type_filter);
    move |filter: F| compress::<F, T, CT>(None, quality, ctf.clone(), filter).boxed()
}

// pub fn algo<F, T>(
//     algo: Algorithm,
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

pub fn brotli<F, T, CT>(
    quality: Level,
    content_type_filter: CT,
) -> impl Fn(F) -> warp::filters::BoxedFilter<(Compressed,)>
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
    CT: ContentTypeFilter + Sync + Send + 'static,
{
    let ctf = Arc::new(content_type_filter);
    move |filter: F| compress::<F, T, CT>(Some(Algorithm::BR), quality, ctf.clone(), filter).boxed()
}

fn compress<F, T, CT>(
    algo: Option<Algorithm>,
    quality: Level,
    content_type_filter: Arc<CT>,
    filter: F,
) -> impl Filter<Extract = (Compressed,), Error = Rejection> + Clone + Send + Sync + 'static
where
    F: Filter<Extract = (T,), Error = Rejection> + Clone + Send + Sync + 'static,
    F::Extract: warp::Reply,
    T: warp::Reply + 'static,
    CT: ContentTypeFilter + Sync + Send + 'static,
{
    use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
    warp::any()
        .and(filter)
        .and(compression_options())
        .map(move |reply: T, options: CompressionOptions| (reply.into_response().into(), options))
        .untuple_one()
        .map(
            move |mut compressable: Compressable, options: CompressionOptions| {
                let prefered_encoding: Option<Algorithm> = options
                    .accept_encoding
                    .as_ref()
                    .and_then(AcceptEncoding::prefered_encoding)
                    .and_then(Into::into);

                let content_type: Option<ContentType> = compressable.head.headers.typed_get();
                let algo = if content_type_filter.should_compress(content_type) {
                    algo.or(prefered_encoding)
                } else {
                    None
                };
                crate::debug!("compression algorithm: {:?}", algo);

                let stream = StreamReader::new(compressable.body);
                let encoded_stream: Box<dyn tokio::io::AsyncRead + Send + std::marker::Unpin> =
                    match algo {
                        Some(Algorithm::BR) => {
                            Box::new(BrotliEncoder::with_quality(stream, quality))
                        }
                        Some(Algorithm::DEFLATE) => {
                            Box::new(DeflateEncoder::with_quality(stream, quality))
                        }
                        Some(Algorithm::GZIP) => {
                            Box::new(GzipEncoder::with_quality(stream, quality))
                        }
                        None => Box::new(stream),
                    };
                let compressed = hyper::Body::wrap_stream(ReaderStream::new(encoded_stream));
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
                Compressed(reply::Response::from_parts(compressable.head, compressed))
            },
        )
}
