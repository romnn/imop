#[macro_use]
extern crate lazy_static;

// use crate::util::ScalarDuration;
use anyhow::Result;
use futures_util::future::Either;
use futures_util::TryFuture;
use futures_util::{future, ready, stream, FutureExt, Stream, StreamExt, TryFutureExt};
use std::pin::Pin;
// use tokio::fs::File;
use tokio::io::AsyncSeekExt;
use warp::hyper::Body;
use warp::reply::Response;
use warp::Future;
// use futures::{future, stream, Future, Stream};
use warp::Rejection;
// use tokio::fs::File as TkFile;
// use chrono::{Duration, Local, NaiveDate};
use bytes::{Bytes, BytesMut};
use clap::Parser;
use headers::{
    AcceptRanges, ContentLength, ContentRange, ContentType, Header, HeaderMapExt, IfModifiedSince,
    IfRange, IfUnmodifiedSince, LastModified, Range,
};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::task::Poll;
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::io::poll_read_buf;
use urlencoding::decode;
use warp::http::{StatusCode, Uri};
use warp::{Filter, Reply};

trait FilterClone: Filter + Clone {}
type One<T> = (T,);

impl<T: Filter + Clone> FilterClone for T {}

#[derive(Clone, Debug)]
struct ArcPath(Arc<PathBuf>);

impl AsRef<Path> for ArcPath {
    fn as_ref(&self) -> &Path {
        (*self.0).as_ref()
    }
}

// #[derive(Copy, Clone)]
// struct FilterFn<F> {
//     func: F,
// }

// impl<F, U> FilterBase for FilterFn<F>
// where
//     F: Fn(&mut warp::route::Route) -> U,
//     U: TryFuture + Send + 'static,
//     U::Ok: Tuple + Send,
//     U::Error: warp::reject::IsReject,
// {
//     type Extract = U::Ok;
//     type Error = U::Error;
//     type Future =
//         Pin<Box<dyn Future<Output = Result<Self::Extract, Self::Error>> + Send + 'static>>;

//     #[inline]
//     fn filter(&self, _: Internal) -> Self::Future {
//         Box::pin(warp::route::with(|route| (self.func)(route)).into_future())
//     }
// }

fn reserve_at_least(buf: &mut BytesMut, cap: usize) {
    if buf.capacity() - buf.len() < cap {
        buf.reserve(cap);
    }
}

fn file_stream(
    mut file: tokio::fs::File,
    buf_size: usize,
    (start, end): (u64, u64),
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send {
    use std::io::SeekFrom;

    let seek = async move {
        if start != 0 {
            file.seek(SeekFrom::Start(start)).await?;
        }
        Ok(file)
    };

    seek.into_stream()
        .map(move |result| {
            let mut buf = BytesMut::new();
            let mut len = end - start;
            let mut f = match result {
                Ok(f) => f,
                Err(f) => return Either::Left(stream::once(future::err(f))),
            };

            Either::Right(stream::poll_fn(move |cx| {
                if len == 0 {
                    return Poll::Ready(None);
                }
                reserve_at_least(&mut buf, buf_size);

                let n = match ready!(poll_read_buf(Pin::new(&mut f), cx, &mut buf)) {
                    Ok(n) => n as u64,
                    Err(err) => {
                        // tracing::debug!("file read error: {}", err);
                        return Poll::Ready(Some(Err(err)));
                    }
                };

                if n == 0 {
                    // tracing::debug!("file read found EOF before expected length");
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
                // tracing::trace!("unsatisfiable byte range: {}-{}/{}", start, end, max_len);
                Err(BadRange)
            }
        })
        .next()
        .unwrap_or(Ok((0, max_len)));
    ret
}

// fn filter_fn<F, U>(func: F) -> FilterFn<F>
// where
//     F: Fn(&mut warp::route::Route) -> U,
//     U: TryFuture,
//     U::Ok: Tuple,
//     U::Error: warp::reject::IsReject,
// {
//     FilterFn { func }
// }

// fn filter_fn_one<F, U>(func: F) -> impl Filter<Extract = (U::Ok,), Error = U::Error> + Copy
// where
//     F: Fn(&mut warp::route::Route) -> U + Copy,
//     U: TryFuture + Send + 'static,
//     U::Ok: Send,
//     U::Error: warp::reject::IsReject,
// {
//     filter_fn(move |route| func(route).map_ok(|item| (item,)))
// }

// fn optional2<T>() -> impl Filter<Extract = One<Option<T>>, Error = Infallible> + Copy
// where
//     T: Header + Send + 'static,
// {
//     // filter_fn_one(move |route| future::ready(Ok(route.headers().typed_get())))
// }

fn sanitize_path(base: impl AsRef<Path>, tail: &str) -> Result<PathBuf, Rejection> {
    let mut buf = PathBuf::from(base.as_ref());
    let p = match decode(tail) {
        Ok(p) => p,
        Err(err) => {
            // tracing::debug!("dir: failed to decode route={:?}: {:?}", tail, err);
            // FromUrlEncodingError doesn't implement StdError
            return Err(warp::reject::not_found());
        }
    };
    // tracing::trace!("dir? base={:?}, route={:?}", base.as_ref(), p);
    for seg in p.split('/') {
        if seg.starts_with("..") {
            // tracing::warn!("dir: rejecting segment starting with '..'");
            return Err(warp::reject::not_found());
        } else if seg.contains('\\') {
            // tracing::warn!("dir: rejecting segment containing with backslash (\\)");
            return Err(warp::reject::not_found());
        } else {
            buf.push(seg);
        }
    }
    Ok(buf)
}

#[derive(Debug)]
struct Conditionals {
    if_modified_since: Option<IfModifiedSince>,
    if_unmodified_since: Option<IfUnmodifiedSince>,
    if_range: Option<IfRange>,
    range: Option<Range>,
}

enum Cond {
    NoBody(Response),
    WithBody(Option<Range>),
}

impl Conditionals {
    fn check(self, last_modified: Option<LastModified>) -> Cond {
        if let Some(since) = self.if_unmodified_since {
            let precondition = last_modified
                .map(|time| since.precondition_passes(time.into()))
                .unwrap_or(false);

            if !precondition {
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::PRECONDITION_FAILED;
                return Cond::NoBody(res);
            }
        }

        if let Some(since) = self.if_modified_since {
            // tracing::trace!(
            //     "if-modified-since? header = {:?}, file = {:?}",
            //     since,
            //     last_modified
            // );
            let unmodified = last_modified
                .map(|time| !since.is_modified(time.into()))
                // no last_modified means its always modified
                .unwrap_or(false);
            if unmodified {
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::NOT_MODIFIED;
                return Cond::NoBody(res);
            }
        }

        if let Some(if_range) = self.if_range {
            // tracing::trace!("if-range? {:?} vs {:?}", if_range, last_modified);
            let can_range = !if_range.is_modified(None, last_modified.as_ref());

            if !can_range {
                return Cond::WithBody(None);
            }
        }

        Cond::WithBody(self.range)
    }
}

fn path_from_tail(
    base: Arc<PathBuf>,
) -> impl FilterClone<Extract = One<ArcPath>, Error = Rejection> {
    warp::path::tail().and_then(move |tail: warp::path::Tail| {
        future::ready(sanitize_path(base.as_ref(), tail.as_str())).and_then(|mut buf| async {
            let is_dir = tokio::fs::metadata(buf.clone())
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);

            if is_dir {
                // tracing::debug!("dir: appending index.html to directory path");
                buf.push("index.html");
            }
            // tracing::trace!("dir: {:?}", buf);
            Ok(ArcPath(Arc::new(buf)))
        })
    })
}

fn conditionals() -> impl Filter<Extract = (Conditionals,), Error = Infallible> + Copy {
    warp::header::headers_cloned().map(|headers: headers::HeaderMap| Conditionals {
        if_modified_since: headers.typed_get(),
        if_unmodified_since: headers.typed_get(),
        if_range: headers.typed_get(),
        range: headers.typed_get(),
    })
    // |route: &mut warp::route::Route| async {
    //     (Conditionals {
    //         if_modified_since: None,
    //         if_unmodified_since: None,
    //         if_range: None,
    //         range: None,
    //     },)
    // }
    // warp::any().map(|| {
    //     (Conditionals {
    //         if_modified_since: None,
    //         if_unmodified_since: None,
    //         if_range: None,
    //         range: None,
    //     },)
    // })

    // optional2()
    //     .and(optional2())
    //     .and(optional2())
    //     .and(optional2())
    //     .map(
    //         |if_modified_since, if_unmodified_since, if_range, range| Conditionals {
    //             if_modified_since,
    //             if_unmodified_since,
    //             if_range,
    //             range,
    //         },
    //     )
}

async fn file_metadata(
    f: tokio::fs::File,
) -> Result<(tokio::fs::File, std::fs::Metadata), Rejection> {
    match f.metadata().await {
        Ok(meta) => Ok((f, meta)),
        Err(err) => {
            // tracing::debug!("file metadata error: {}", err);
            Err(warp::reject::not_found())
        }
    }
}

#[derive(Debug)]
pub struct File {
    resp: Response,
    path: ArcPath,
}

impl File {
    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }
}

impl Reply for File {
    fn into_response(self) -> Response {
        self.resp
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
                        let stream = file_stream(file, buf_size, (start, end));
                        let body = Body::wrap_stream(stream);

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
                        let mut resp = Response::new(Body::empty());
                        *resp.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                        resp.headers_mut()
                            .typed_insert(ContentRange::unsatisfied_bytes(len));
                        resp
                    })
            }
        };

        File { resp, path }
    })
}

#[derive(Debug, Clone)]
struct FilePermissionError;
// impl std::error::Error for FilePermissionError {}
impl warp::reject::Reject for FilePermissionError {}

#[derive(Debug, Clone)]
struct FileOpenError;
// impl std::error::Error for FileOpenError {}
impl warp::reject::Reject for FileOpenError {}

async fn test(path: ArcPath, conditionals: Conditionals) -> Result<File, Rejection> {
    match tokio::fs::File::open(&path).await {
        // Ok(f) => Either::Left(file_conditional(f, path, conditionals)),
        Ok(f) => file_conditional(f, path, conditionals).await,
        Err(err) => {
            let rej = match err.kind() {
                io::ErrorKind::NotFound => warp::reject::not_found(),
                io::ErrorKind::PermissionDenied => warp::reject::custom(FilePermissionError {}),
                _ => warp::reject::custom(FileOpenError {}),
            };
            // Either::Right(future::err(rej))
            // Either::Right(rej)
            Err(rej)
        }
    }
    // tokio::fs::File::open(path.clone()).then(move |res| match res {
    //     Ok(f) => Either::Left(file_conditional(f, path, conditionals)),
    //     Err(err) => {
    //         let rej = match err.kind() {
    //             io::ErrorKind::NotFound => warp::reject::not_found(),
    //             io::ErrorKind::PermissionDenied => warp::reject::custom(FilePermissionError {}),
    //             _ => warp::reject::custom(FileOpenError {}),
    //         };
    //         Either::Right(future::err(rej))
    //     }
    // })
}

// fn file_reply(
//     path: ArcPath,
//     conditionals: Conditionals,
// ) -> impl Future<Output = Result<File, Rejection>> + Send {

async fn file_reply(path: ArcPath, conditionals: Conditionals) -> Result<File, Rejection> {
    println!("{:?}", path);
    test(path, conditionals).await
}

#[derive(Parser, Serialize, Deserialize, Debug, Clone)]
#[clap(version = "1.0", author = "romnn <contact@romnn.com>")]
pub struct ImopOptions {
    #[clap(short = 'i', long = "images", help = "image source path")]
    image_path: PathBuf,

    #[clap(short = 'p', long = "port", default_value = "3000")]
    port: u16,

    #[clap(short = 'n', long = "pages", help = "max number of pages to scape")]
    max_pages: Option<u32>,

    #[clap(short = 'r', long = "retain", help = "days to retain")]
    retain_days: Option<u64>,
}

// pub async fn serve_optimized(
// pub async fn serve_optimized(
//     file: warp::filters::fs::File,
//     // ) -> Result<impl warp::Reply, warp::Rejection> {
//     // ) -> impl Filter<Extract = (warp::filters::fs::File,), Error = Rejection> {
//     // ) -> impl Future<Item = warp::filters::fs::File, Error = Rejection> + Send {
// ) -> impl Future<Output = Result<warp::filters::fs::File, Rejection>> + Send {
//     file
//     // warp::fs::file("./Cargo.toml") // .into_response()
//     // Ok("Hello world !")
// }

#[tokio::main]
async fn main() {
    let options: ImopOptions = ImopOptions::parse();
    println!(
        "{}",
        serde_json::to_string_pretty(&options).expect("options")
    );

    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut server_shutdown_rx = shutdown_tx.subscribe();

    let health = warp::path!("healthz").and(warp::get()).map(|| "healthy");

    let base = Arc::new(options.image_path);
    let images = warp::path("images")
        // .and(warp::fs::dir(options.image_path))
        // .and(warp::fs::dir(options.image_path))
        // .map(|reply: warp::filters::fs::File| Ok(reply))
        // .map(|reply: Result<warp::filters::fs::File>| reply.unwrap())
        // .map(serve_optimized)
        // .and_then(|reply: warp::filters::fs::File| {
        //     // todo: parse the compression options
        //     // todo: look up if the file was already compressed, if so, get it from the disk cache
        //     // todo: if not, compress and save to cache
        //     // todo: serve the correct file
        //     println!("{}", reply.path().display());
        //     warp::fs::file("./Cargo.toml") // .into_response()
        //     // if reply.path().ends_with("video.mp4") {
        //     //     warp::reply::with_header(reply, "Content-Type", "video/mp4").into_response()
        //     // } else {
        //     //     reply.into_response()
        //     // }
        // })
        // .with(warp::compression::deflate());
        // crate::get()
        // .or(warp::head())
        // .unify()
        .and(path_from_tail(base))
        .and(conditionals())
        .and_then(file_reply);
    // .with(warp::compression::deflate());

    // let tracker_clone = tracker.clone();
    // let options_clone = options.clone();
    // let current = warp::path!("likes" / "current")
    //     .and(warp::get())
    //     .and(warp::query::<RenderOptions>())
    //     .and(warp::any().map(move || tmpl_clone.clone()))
    //     .and(warp::any().map(move || tracker_clone.clone()))
    //     .and(warp::any().map(move || options_clone.clone()))
    //     .and_then(current_handler);

    // let tmpl_clone = tmpl.clone();
    // let tracker_clone = tracker.clone();
    // let likes = warp::path!(u16 / u16 / "likes")
    //     .and(warp::get())
    //     .and(warp::any().map(move || tmpl_clone.clone()))
    //     .and(warp::any().map(move || tracker_clone.clone()))
    //     .and_then(like_handler);

    // let index = warp::path::end()
    //     .and(warp::get())
    //     .map(|| warp::redirect(Uri::from_static("/1/0/likes")));

    let routes = images.or(health);
    let (_addr, server) =
        warp::serve(routes).bind_with_graceful_shutdown(([0, 0, 0, 0], options.port), async move {
            server_shutdown_rx.recv().await.expect("shutdown server");
            println!("server shutting down");
        });

    let tserver = tokio::task::spawn(server);

    // let tracker_clone = tracker.clone();
    // let periodic = tokio::task::spawn(async move {
    //     tracker_clone.run().await.expect("start periodic");
    // });

    // let tracker_clone = tracker.clone();
    // let mut shutdown_rx = shutdown_tx.subscribe();
    // tokio::task::spawn(async move { tracker_clone.periodic(&mut shutdown_rx).await });

    if (signal::ctrl_c().await).is_ok() {
        println!("received shutdown");
        println!("waiting for pending tasks to complete...");
        shutdown_tx.send(()).expect("shutdown");
    };

    tserver.await.expect("server terminated");
    // periodic.await.expect("periodic terminated");
    println!("exiting");
}
