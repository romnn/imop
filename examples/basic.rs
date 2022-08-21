#![allow(warnings)]

use anyhow::Result;
use clap::Parser;
use futures::io::AsyncReadExt;
use futures::stream::{StreamExt, TryStreamExt};
use imop::cache::{CachedImage, ImageCache, InMemoryImageCache};
use imop::compression;
use imop::conditionals::{conditionals, Conditionals};
use imop::file::{file_stream, path_from_tail, serve_file, ArcPath, File, FileOrigin};
use imop::headers::HeaderMapExt;
use imop::headers::{AcceptRanges, ContentLength, ContentType};
use imop::image::{mime_of_format, ExternalImage, Image, ImageFormat, Optimizations};
use mime_guess::mime;
use serde::Deserialize;
use std::hash::Hash;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncBufReadExt;
use tokio::signal;
use warp::{Filter, Rejection};

#[derive(Parser)]
struct Options {
    #[clap(short = 'i', long = "images", help = "image source path")]
    image_path: PathBuf,

    #[clap(short = 'p', long = "port", default_value = "3000")]
    port: u16,
}

#[derive(Eq, PartialEq, Hash)]
enum CacheKey {
    // Source(&'a FileOrigin),
    // Optimized {
    //     origin: &'a FileOrigin,
    //     optimizations: &'a Optimizations,
    // },
    Source(FileOrigin),
    Optimized {
        origin: FileOrigin,
        optimizations: Optimizations,
    },
    // origin: FileOrigin,
    // optimizations: Optimizations,
}

async fn serve_static_file(
    path: ArcPath,
    conditionals: Conditionals,
    optimizations: Optimizations,
    cache: Arc<InMemoryImageCache<CacheKey>>,
) -> Result<File, Rejection>
// where
//     K: Hash + Eq,
{
    imop::debug!("{:?}", &path);
    imop::debug!("{:?}", &optimizations);

    let mime = mime_guess::from_path(path.as_ref()).first_or_octet_stream();
    imop::debug!("{}", &mime);

    match mime.type_() {
        mime::IMAGE => {
            let mut img = Image::open(&path)?;
            let mut encoded = std::io::Cursor::new(Vec::new());
            let target_format = optimizations
                .format
                .or(img.format())
                .unwrap_or(ImageFormat::Jpeg);

            img.resize(&optimizations.bounds());
            img.encode(&mut encoded, target_format, optimizations.quality)?;

            let len = encoded.position() as u64;
            encoded.set_position(0);
            let stream = file_stream(encoded, (0, len), None);
            let body = warp::hyper::Body::wrap_stream(stream);
            let mut resp = warp::reply::Response::new(body);

            resp.headers_mut()
                .typed_insert(imop::headers::ContentLength(len as u64));
            resp.headers_mut()
                .typed_insert(imop::headers::ContentType::from(
                    mime_of_format(target_format).unwrap_or(mime::IMAGE_STAR),
                ));
            resp.headers_mut()
                .typed_insert(imop::headers::AcceptRanges::bytes());

            Ok(File {
                resp,
                origin: FileOrigin::Path(path),
            })
        }
        _ => serve_file(path, conditionals).await,
    }
}

async fn fetch_and_serve_file(
    optimizations: Optimizations,
    external_image: ExternalImage,
    cache: Arc<InMemoryImageCache<CacheKey>>,
) -> Result<File, Rejection> {
    imop::debug!("image = {:?}", &external_image);
    imop::debug!("optimizations = {:?}", &optimizations);

    match external_image.image {
        Some(url) => {
            let origin = FileOrigin::Url(url.clone());
            let now = Instant::now();

            let key = CacheKey::Optimized {
                origin: origin.clone(),
                optimizations,
            };
            let source_key = CacheKey::Source(origin.clone());

            // fast path: check if optimized image is cached
            if let Some(cached) = cache.get(&key).await {
                let target_format = optimizations
                    .format
                    .or(cached.format())
                    .unwrap_or(ImageFormat::Jpeg);

                let len = cached.content_length() as u64;
                let stream = file_stream(cached.data(), (0, len), None);
                let body = warp::hyper::Body::wrap_stream(stream);
                let mut resp = warp::reply::Response::new(body);

                resp.headers_mut()
                    .typed_insert(imop::headers::ContentLength(len));
                resp.headers_mut()
                    .typed_insert(imop::headers::ContentType::from(
                        mime_of_format(target_format).unwrap_or(mime::IMAGE_STAR),
                    ));
                resp.headers_mut()
                    .typed_insert(imop::headers::AcceptRanges::bytes());

                return Ok(File {
                    resp,
                    origin: FileOrigin::Url(url),
                });
            };

            let mut img = match cache.get(&source_key).await {
                Some(cached) => Image::new(cached.data())?,
                None => {
                    let res = reqwest::get(url.clone())
                        .await
                        .map_err(imop::image::Error::from)?;
                    let mut data = Vec::new();
                    let mut reader = futures::io::BufReader::new(
                        res.bytes_stream()
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                            .into_async_read(),
                    );
                    reader
                        .read_to_end(&mut data)
                        .await
                        .map_err(imop::image::Error::from)?;
                    imop::debug!("download of {} took {:?}", &url, now.elapsed());
                    let img = Image::new(std::io::Cursor::new(&data))?;

                    // add source image to cache
                    cache
                        .put(source_key, std::io::Cursor::new(&data), img.format())
                        .await;
                    img
                }
            };

            let mut encoded = std::io::Cursor::new(Vec::new());
            let target_format = optimizations
                .format
                .or(img.format())
                .unwrap_or(ImageFormat::Jpeg);

            img.resize(&optimizations.bounds());
            img.encode(&mut encoded, target_format, optimizations.quality)?;

            let len = encoded.position() as u64;
            encoded.set_position(0);
            let stream = file_stream(encoded.clone(), (0, len), None);
            let body = warp::hyper::Body::wrap_stream(stream);
            let mut resp = warp::reply::Response::new(body);

            resp.headers_mut()
                .typed_insert(imop::headers::ContentLength(len as u64));
            resp.headers_mut()
                .typed_insert(imop::headers::ContentType::from(
                    mime_of_format(target_format).unwrap_or(mime::IMAGE_STAR),
                ));
            resp.headers_mut()
                .typed_insert(imop::headers::AcceptRanges::bytes());

            // add source image to cache
            encoded.set_position(0);
            let key = cache.put(key, encoded, img.format()).await;

            Ok(File {
                resp,
                origin: FileOrigin::Url(url),
            })
        }
        None => Err(warp::reject::reject()),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let options: Options = Options::parse();
    let base = Arc::new(options.image_path);
    // let cache: Arc<InMemoryCache<CacheKey, Vec<u8>>> = Arc::new(InMemoryCache::new(Some(10)));
    // let cache: Arc<InMemoryCache<CacheKey, futures::io::Cursor<Vec<u8>>>> =

    // those all implement asyncread which is good
    // but we should not specify file and cursor
    // we should never clone cursors as then we copy
    // also we should not reuse the same cursor because that would be unsafe
    let cache = Arc::new(InMemoryImageCache::<CacheKey>::new(Some(10)));
    let cache2 = Arc::new(InMemoryImageCache::<CacheKey>::new(Some(10)));

    let cache_clone = cache.clone();
    let static_images = warp::path("static")
        .or(warp::head())
        .unify()
        .and(path_from_tail(base))
        .and(conditionals())
        .and(warp::query::<Optimizations>())
        .and(warp::any().map(move || cache_clone.clone()))
        .and_then(serve_static_file)
        .with(warp::wrap_fn(compression::auto(
            compression::Level::Best,
            compression::CompressContentType::default(),
        )));

    let cache_clone = cache.clone();
    let fetch_images = warp::path::end()
        .or(warp::head())
        .unify()
        .and(warp::query::<Optimizations>())
        .and(warp::query::<ExternalImage>())
        .and(warp::any().map(move || cache_clone.clone()))
        .and_then(fetch_and_serve_file)
        .with(warp::wrap_fn(compression::auto(
            compression::Level::Best,
            compression::CompressContentType::default(),
        )));

    let shutdown = async move {
        signal::ctrl_c().await.expect("shutdown server");
        println!("server shutting down");
    };
    let addr = ([0, 0, 0, 0], options.port);
    let routes = static_images.or(fetch_images);
    warp::serve(routes).run(addr).await;
    Ok(())
}
