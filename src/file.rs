use super::conditionals::{Cond, Conditionals};
use super::headers::{
    AcceptRanges, ContentLength, ContentRange, ContentType, HeaderMapExt, LastModified, Range,
};
use super::FilterClone;
use bytes::{Bytes, BytesMut};
use futures::{future, Future, FutureExt, Stream, StreamExt, TryFutureExt};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt};
use tokio_util::io::poll_read_buf;
use warp::{http::StatusCode, hyper, reply, Filter, Rejection};

const DEFAULT_READ_BUF_SIZE: usize = 8_192;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Path(Arc<std::path::PathBuf>);

impl AsRef<std::path::Path> for Path {
    #[inline]
    fn as_ref(&self) -> &std::path::Path {
        (*self.0).as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Origin {
    Url(reqwest::Url),
    Path(Path),
}

#[derive(Debug)]
pub struct File {
    pub resp: reply::Response,
    pub origin: Origin,
}

impl File {
    #[inline]
    pub fn url(&self) -> Option<&reqwest::Url> {
        match self.origin {
            Origin::Url(ref url) => Some(url),
            Origin::Path(_) => None,
        }
    }

    #[inline]
    pub fn path(&self) -> Option<&std::path::Path> {
        match self.origin {
            Origin::Path(ref path) => Some(path.as_ref()),
            Origin::Url(_) => None,
        }
    }
}

impl warp::Reply for File {
    #[inline]
    fn into_response(self) -> reply::Response {
        self.resp
    }
}

#[inline]
fn reserve_at_least(buf: &mut BytesMut, cap: usize) {
    if buf.capacity() - buf.len() < cap {
        buf.reserve(cap);
    }
}

#[inline]
pub fn stream<R: AsyncRead + AsyncSeek + std::marker::Unpin + Send>(
    mut reader: R,
    (start, end): (u64, u64),
    buf_size: Option<usize>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    use futures::future::Either;
    use num_traits::NumCast;
    use std::io::SeekFrom;

    let buf_size = buf_size.unwrap_or(DEFAULT_READ_BUF_SIZE);

    let seek = async move {
        if start != 0 {
            reader.seek(SeekFrom::Start(start)).await?;
        }
        Ok(reader)
    };

    seek.into_stream()
        .map(move |result| {
            let mut buf = BytesMut::new();
            let mut len = end - start;
            let mut f = match result {
                Ok(f) => f,
                Err(e) => return Either::Left(futures::stream::once(future::err(e))),
            };

            Either::Right(futures::stream::poll_fn(move |cx| {
                if len == 0 {
                    return Poll::Ready(None);
                }
                reserve_at_least(&mut buf, buf_size);

                let n = match futures::ready!(poll_read_buf(Pin::new(&mut f), cx, &mut buf)) {
                    Ok(n) => n as u64,
                    Err(err) => {
                        return Poll::Ready(Some(Err(err)));
                    }
                };

                if n == 0 {
                    return Poll::Ready(None);
                }

                let mut chunk = buf.split().freeze();
                if n > len {
                    chunk = chunk.split_to(NumCast::from(len).unwrap());
                    len = 0;
                } else {
                    len -= n;
                }

                Poll::Ready(Some(Ok(chunk)))
            }))
        })
        .flatten()
}

#[inline]
fn optimal_buf_size(metadata: &std::fs::Metadata) -> usize {
    use num_traits::NumCast;
    let block_size = get_block_size(metadata);
    // If file length is smaller than block size, don't waste space
    // reserving a bigger-than-needed buffer.
    let block_size: u64 = NumCast::from(block_size).unwrap();
    NumCast::from(metadata.len().min(block_size)).unwrap()
}

#[cfg(unix)]
fn get_block_size(metadata: &std::fs::Metadata) -> usize {
    use num_traits::NumCast;
    use std::os::unix::fs::MetadataExt;
    // Use device blocksize unless it's really small.
    DEFAULT_READ_BUF_SIZE.max(NumCast::from(metadata.blksize()).unwrap())
}

#[cfg(not(unix))]
fn get_block_size(_metadata: &std::fs::Metadata) -> usize {
    DEFAULT_READ_BUF_SIZE
}

struct BadRange;

fn bytes_range(range: Option<Range>, max_len: u64) -> Result<(u64, u64), BadRange> {
    use std::ops::Bound;

    let range = if let Some(range) = range {
        range
    } else {
        return Ok((0, max_len));
    };

    let ret = range
        .iter()
        .map(|(start, end)| {
            let start = match start {
                Bound::Unbounded => 0,
                Bound::Included(s) => s,
                Bound::Excluded(s) => s + 1,
            };

            let end = match end {
                Bound::Unbounded => max_len,
                Bound::Included(s) => {
                    // For the special case where s == the file size
                    if s == max_len {
                        s
                    } else {
                        s + 1
                    }
                }
                Bound::Excluded(s) => s,
            };

            if start < end && end <= max_len {
                Ok((start, end))
            } else {
                Err(BadRange)
            }
        })
        .next()
        .unwrap_or(Ok((0, max_len)));
    ret
}

#[inline]
fn sanitize_path(
    base: impl AsRef<std::path::Path>,
    tail: &str,
) -> Result<std::path::PathBuf, Rejection> {
    let mut buf = std::path::PathBuf::from(base.as_ref());
    let p = match urlencoding::decode(tail) {
        Ok(p) => p,
        Err(_) => {
            // FromUrlEncodingError doesn't implement StdError
            return Err(warp::reject::not_found());
        }
    };
    for seg in p.split('/') {
        if seg.starts_with("..") || seg.contains('\\') {
            return Err(warp::reject::not_found());
        }
        buf.push(seg);
    }
    Ok(buf)
}

#[inline]
#[must_use]
pub fn path_from_tail(
    base: Arc<std::path::PathBuf>,
) -> impl FilterClone<Extract = (Path,), Error = Rejection> {
    warp::path::tail().and_then(move |tail: warp::path::Tail| {
        future::ready(sanitize_path(base.as_ref(), tail.as_str())).and_then(|mut buf| async {
            let is_dir = tokio::fs::metadata(buf.clone())
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);

            if is_dir {
                buf.push("index.html");
            }
            Ok(Path(Arc::new(buf)))
        })
    })
}

async fn file_metadata(
    f: tokio::fs::File,
) -> Result<(tokio::fs::File, std::fs::Metadata), Rejection> {
    match f.metadata().await {
        Ok(meta) => Ok((f, meta)),
        Err(_) => Err(warp::reject::not_found()),
    }
}

fn file_conditional(
    f: tokio::fs::File,
    path: Path,
    conditionals: Conditionals,
) -> impl Future<Output = Result<File, Rejection>> + Send {
    file_metadata(f).map_ok(move |(file, meta)| {
        let mut len = meta.len();
        let modified = meta.modified().ok().map(LastModified::from);

        let resp = match conditionals.check(modified) {
            Cond::NoBody(resp) => resp,
            Cond::WithBody(range) => {
                let range = bytes_range(range, len).map(|(start, end)| {
                    let sub_len = end - start;
                    let buf_size = optimal_buf_size(&meta);
                    let file_stream = stream(file, (start, end), Some(buf_size));
                    let body = hyper::Body::wrap_stream(file_stream);

                    let mut resp = reply::Response::new(body);

                    if sub_len != len {
                        *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
                        resp.headers_mut().typed_insert(
                            ContentRange::bytes(start..end, len).expect("valid ContentRange"),
                        );

                        len = sub_len;
                    }

                    let mime = mime_guess::from_path(path.as_ref()).first_or_octet_stream();

                    resp.headers_mut().typed_insert(ContentLength(len));
                    resp.headers_mut().typed_insert(ContentType::from(mime));
                    resp.headers_mut().typed_insert(AcceptRanges::bytes());

                    if let Some(last_modified) = modified {
                        resp.headers_mut().typed_insert(last_modified);
                    }

                    resp
                });
                range.unwrap_or_else(|BadRange| {
                    // bad byte range
                    let mut resp = reply::Response::new(hyper::Body::empty());
                    *resp.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                    resp.headers_mut()
                        .typed_insert(ContentRange::unsatisfied_bytes(len));
                    resp
                })
            }
        };

        File {
            resp,
            origin: Origin::Path(path),
        }
    })
}

#[derive(Debug, Clone)]
struct FilePermissionError;
impl warp::reject::Reject for FilePermissionError {}

#[derive(Debug, Clone)]
struct FileOpenError;
impl warp::reject::Reject for FileOpenError {}

pub async fn serve(path: Path, conditionals: Conditionals) -> Result<File, Rejection> {
    use std::io::ErrorKind;
    match tokio::fs::File::open(&path).await {
        Ok(f) => file_conditional(f, path, conditionals).await,
        Err(err) => {
            let rej = match err.kind() {
                ErrorKind::NotFound => warp::reject::not_found(),
                ErrorKind::PermissionDenied => warp::reject::custom(FilePermissionError {}),
                _ => warp::reject::custom(FileOpenError {}),
            };
            Err(rej)
        }
    }
}
