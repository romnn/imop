use super::conditionals::{Cond, Conditionals};
use super::headers::{
    AcceptRanges, ContentLength, ContentRange, ContentType, HeaderMapExt, LastModified, Range,
};
use super::FilterClone;
use bytes::{Bytes, BytesMut};
use futures_util::future::Either;
use futures_util::{future, ready, stream, FutureExt, Stream, StreamExt, TryFutureExt};
use std::cmp;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt};
use tokio_util::io::poll_read_buf;
use urlencoding::decode;
use warp::http::StatusCode;
use warp::hyper;
use warp::reply::Response;
use warp::Future;
use warp::Rejection;
use warp::{Filter, Reply};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ArcPath(Arc<PathBuf>);

impl AsRef<Path> for ArcPath {
    fn as_ref(&self) -> &Path {
        (*self.0).as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum FileOrigin {
    Url(reqwest::Url),
    Path(ArcPath),
}

#[derive(Debug)]
pub struct File {
    pub resp: Response,
    pub origin: FileOrigin,
}

impl File {
    pub fn url(&self) -> Option<&reqwest::Url> {
        match &self.origin {
            FileOrigin::Url(url) => Some(&url),
            _ => None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.origin {
            FileOrigin::Path(path) => Some(path.as_ref()),
            _ => None,
        }
    }
}

impl Reply for File {
    fn into_response(self) -> Response {
        self.resp
    }
}

fn reserve_at_least(buf: &mut BytesMut, cap: usize) {
    if buf.capacity() - buf.len() < cap {
        buf.reserve(cap);
    }
}

pub fn file_stream<R: AsyncRead + AsyncSeek + std::marker::Unpin + Send>(
    mut reader: R,
    (start, end): (u64, u64),
    buf_size: Option<usize>,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send {
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
                Err(e) => return Either::Left(stream::once(future::err(e))),
            };

            Either::Right(stream::poll_fn(move |cx| {
                if len == 0 {
                    return Poll::Ready(None);
                }
                reserve_at_least(&mut buf, buf_size);

                let n = match ready!(poll_read_buf(Pin::new(&mut f), cx, &mut buf)) {
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
                    chunk = chunk.split_to(len as usize);
                    len = 0;
                } else {
                    len -= n;
                }

                Poll::Ready(Some(Ok(chunk)))
            }))
        })
        .flatten()
}

const DEFAULT_READ_BUF_SIZE: usize = 8_192;

fn optimal_buf_size(metadata: &std::fs::Metadata) -> usize {
    let block_size = get_block_size(metadata);

    // If file length is smaller than block size, don't waste space
    // reserving a bigger-than-needed buffer.
    cmp::min(block_size as u64, metadata.len()) as usize
}

#[cfg(unix)]
fn get_block_size(metadata: &std::fs::Metadata) -> usize {
    use std::os::unix::fs::MetadataExt;
    //TODO: blksize() returns u64, should handle bad cast...
    //(really, a block size bigger than 4gb?)

    // Use device blocksize unless it's really small.
    cmp::max(metadata.blksize() as usize, DEFAULT_READ_BUF_SIZE)
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

fn sanitize_path(base: impl AsRef<Path>, tail: &str) -> Result<PathBuf, Rejection> {
    let mut buf = PathBuf::from(base.as_ref());
    let p = match decode(tail) {
        Ok(p) => p,
        Err(_) => {
            // FromUrlEncodingError doesn't implement StdError
            return Err(warp::reject::not_found());
        }
    };
    for seg in p.split('/') {
        if seg.starts_with("..") || seg.contains('\\') {
            return Err(warp::reject::not_found());
        } else {
            buf.push(seg);
        }
    }
    Ok(buf)
}

pub fn path_from_tail(
    base: Arc<PathBuf>,
) -> impl FilterClone<Extract = (ArcPath,), Error = Rejection> {
    warp::path::tail().and_then(move |tail: warp::path::Tail| {
        future::ready(sanitize_path(base.as_ref(), tail.as_str())).and_then(|mut buf| async {
            let is_dir = tokio::fs::metadata(buf.clone())
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);

            if is_dir {
                buf.push("index.html");
            }
            // Ok(ArcPath(Arc::new(buf)))
            Ok(ArcPath(Arc::new(buf)))
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
    path: ArcPath,
    conditionals: Conditionals,
) -> impl Future<Output = Result<File, Rejection>> + Send {
    file_metadata(f).map_ok(move |(file, meta)| {
        let mut len = meta.len();
        let modified = meta.modified().ok().map(LastModified::from);

        let resp = match conditionals.check(modified) {
            Cond::NoBody(resp) => resp,
            Cond::WithBody(range) => {
                bytes_range(range, len)
                    .map(|(start, end)| {
                        let sub_len = end - start;
                        let buf_size = optimal_buf_size(&meta);
                        let stream = file_stream(file, (start, end), Some(buf_size));
                        let body = hyper::Body::wrap_stream(stream);

                        let mut resp = Response::new(body);

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
                    })
                    .unwrap_or_else(|BadRange| {
                        // bad byte range
                        let mut resp = Response::new(hyper::Body::empty());
                        *resp.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                        resp.headers_mut()
                            .typed_insert(ContentRange::unsatisfied_bytes(len));
                        resp
                    })
            }
        };

        File {
            resp,
            origin: FileOrigin::Path(path),
        }
    })
}

#[derive(Debug, Clone)]
struct FilePermissionError;
impl warp::reject::Reject for FilePermissionError {}

#[derive(Debug, Clone)]
struct FileOpenError;
impl warp::reject::Reject for FileOpenError {}

pub async fn serve_file(path: ArcPath, conditionals: Conditionals) -> Result<File, Rejection> {
    match tokio::fs::File::open(&path).await {
        Ok(f) => file_conditional(f, path, conditionals).await,
        Err(err) => {
            let rej = match err.kind() {
                io::ErrorKind::NotFound => warp::reject::not_found(),
                io::ErrorKind::PermissionDenied => warp::reject::custom(FilePermissionError {}),
                _ => warp::reject::custom(FileOpenError {}),
            };
            Err(rej)
        }
    }
}
