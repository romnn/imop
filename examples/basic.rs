#![allow(warnings)]

use anyhow::Result;
// use clap::Parser;
// use futures::stream::{StreamExt, TryStreamExt};
// // use imop::cache::{
// //     CachedImage, Error as CacheError, FileSystemImageCache, ImageCache, InMemoryImageCache,
// // };
// use imop::compression;
// use imop::conditionals::{conditionals, Conditionals};
// use imop::file::{file_stream, path_from_tail, serve_file, ArcPath, File, FileOrigin};
// use imop::headers::HeaderMapExt;
// use imop::headers::{AcceptRanges, ContentLength, ContentType};
// use imop::image::{
//     mime_of_format, EncodedImage, Error as ImageError, ExternalImage, Image, ImageFormat,
//     Optimizations,
// };
// use mime_guess::mime;
// use serde::Deserialize;
// use std::hash::Hash;
// use std::ops::Deref;
// use std::path::PathBuf;
// use std::sync::Arc;
// use std::time::{Duration, Instant};
// use tokio::io::{AsyncBufReadExt, AsyncReadExt};
// use tokio::signal;
// use tokio_util::compat::FuturesAsyncReadCompatExt;
// use warp::{Filter, Rejection, Reply};

// #[derive(Parser)]
// struct Options {
//     #[clap(short = 'i', long = "images", help = "image source path")]
//     image_path: PathBuf,

//     #[clap(short = 'c', long = "cache", help = "image cache dir path")]
//     cache_dir_path: PathBuf,

//     #[clap(short = 'p', long = "port", default_value = "3000")]
//     port: u16,
// }

// #[derive(Clone, Eq, PartialEq, Hash)]
// enum CacheKey {
//     Source(FileOrigin),
//     Optimized {
//         origin: FileOrigin,
//         optimizations: Optimizations,
//     },
// }

// // async fn serve_static_file<C, V>(
// async fn serve_static_file(
//     path: ArcPath,
//     conditionals: Conditionals,
//     optimizations: Optimizations,
//     cache: Arc<FileSystemImageCache<CacheKey>>,
//     // cache: Arc<C>,
// ) -> Result<File, Rejection>
// // where
// //     C: ImageCache<CacheKey, V>,
// //     V: CachedImage,
// {
//     imop::debug!("{:?}", &path);
//     imop::debug!("{:?}", &optimizations);

//     let mime = mime_guess::from_path(path.as_ref()).first_or_octet_stream();
//     imop::debug!("{}", &mime);

//     match mime.type_() {
//         mime::IMAGE => {
//             let mut img = Image::open(&path).map_err(Error::from)?;
//             let target_format = optimizations
//                 .format
//                 .or(img.format())
//                 .unwrap_or(ImageFormat::Jpeg);

//             img.resize(&optimizations.bounds());
//             let encoded = img
//                 .encode(target_format, optimizations.quality)
//                 .map_err(Error::from)?;

//             Ok(File {
//                 resp: encoded.into_response(),
//                 origin: FileOrigin::Path(path),
//             })
//         }
//         _ => serve_file(path, conditionals).await,
//     }
// }

// #[derive(thiserror::Error, Debug)]
// pub enum Error {
//     #[error("image error error: `{0}`")]
//     Image(#[from] ImageError),

//     #[error("fetch error: `{0}`")]
//     Fetch(#[from] reqwest::Error),

//     #[error("cache error: `{0}`")]
//     Cache(#[from] CacheError),
// }

// impl warp::reject::Reject for Error {}

// // async fn fetch_and_serve_file<C, V>(
// async fn fetch_and_serve_file(
//     optimizations: Optimizations,
//     external_image: ExternalImage,
//     cache: Arc<FileSystemImageCache<CacheKey>>,
//     // cache: Arc<C>,
// ) -> Result<File, Rejection>
// // where
// //     C: ImageCache<CacheKey, V>,
// //     V: CachedImage,
// {
//     imop::debug!("image = {:?}", &external_image);
//     imop::debug!("optimizations = {:?}", &optimizations);

//     match external_image.image {
//         Some(url) => {
//             let origin = FileOrigin::Url(url.clone());
//             let now = Instant::now();

//             let key = CacheKey::Optimized {
//                 origin: origin.clone(),
//                 optimizations,
//             };
//             let source_key = CacheKey::Source(origin.clone());

//             // fast path: check if optimized image is cached
//             if let Some(cached) = cache.get(&key).await {
//                 match async {
//                     let len = cached.content_length().await? as u64;
//                     let mut buffer = Vec::new();
//                     let mut reader = tokio::io::BufReader::new(cached.data().await?);
//                     tokio::io::copy(&mut reader, &mut buffer).await;

//                     let encoded = EncodedImage {
//                         buffer,
//                         format: cached.format(),
//                     };

//                     Ok::<File, CacheError>(File {
//                         resp: encoded.into_response(),
//                         origin: FileOrigin::Url(url.clone()),
//                     })
//                 }
//                 .await
//                 {
//                     Ok(file) => {
//                         imop::debug!("cache hit");
//                         return Ok(file);
//                     }
//                     Err(err) => {
//                         eprintln!("fail fast cache error: {:?}", err);
//                     }
//                 }
//             };

//             let mut img = async {
//                 let cached = cache.get(&source_key).await.ok_or(CacheError::NotFound)?;
//                 let mut buffer = Vec::new();
//                 let mut data = cached.data().await?;
//                 tokio::io::copy(&mut data, &mut buffer)
//                     .await
//                     .map_err(CacheError::from)?;
//                 let reader = std::io::BufReader::new(std::io::Cursor::new(buffer));
//                 let img = Image::new(reader).map_err(CacheError::from)?;
//                 Ok::<Image, CacheError>(img)
//             }
//             .await
//             .map_err(Error::from);

//             if img.is_err() {
//                 imop::debug!("source cache miss");
//                 let res = reqwest::get(url.clone()).await.map_err(Error::from)?;
//                 // let data = res.bytes().await.map_err(Error::from)?;
//                 let mut buffer = Vec::new();
//                 let mut reader = tokio::io::BufReader::new(
//                     res.bytes_stream()
//                         .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
//                         .into_async_read()
//                         .compat(),
//                 );
//                 tokio::io::copy(&mut reader, &mut buffer).await;

//                 imop::debug!("download of {} took {:?}", &url, now.elapsed());

//                 img = Image::new(std::io::Cursor::new(&buffer)).map_err(Error::from);
//                 if let Ok(ref img) = img {
//                     cache
//                         .put(
//                             source_key,
//                             std::io::Cursor::new(&buffer),
//                             img.format().unwrap_or(ImageFormat::Jpeg),
//                         )
//                         .await
//                         .map_err(Error::from)?;
//                 }
//             }

//             let mut img = img?;
//             let target_format = optimizations
//                 .format
//                 .or(img.format())
//                 .unwrap_or(ImageFormat::Jpeg);

//             img.resize(&optimizations.bounds());
//             let encoded = img
//                 .encode(target_format, optimizations.quality)
//                 .map_err(Error::from)?;

//             // add source image to cache
//             cache
//                 .put(key, std::io::Cursor::new(&encoded.buffer), target_format)
//                 .await
//                 .map_err(Error::from)?;

//             Ok(File {
//                 resp: encoded.into_response(),
//                 origin: FileOrigin::Url(url),
//             })
//         }
//         None => Err(warp::reject::reject()),
//     }
// }

#[tokio::main]
async fn main() -> Result<()> {
    // let options: Options = Options::parse();
    // let base = Arc::new(options.image_path);

    // let cache1 = Arc::new(InMemoryImageCache::<CacheKey>::new(Some(10)));
    // let cache2 = Arc::new(InMemoryImageCache::<CacheKey>::new(Some(10)));
    // let cache = Arc::new(FileSystemImageCache::<CacheKey>::new(
    //     options.cache_dir_path,
    //     10,
    // ));

    // let cache_clone = cache.clone();
    // let static_images = warp::path("static")
    //     .or(warp::head())
    //     .unify()
    //     .and(path_from_tail(base))
    //     .and(conditionals())
    //     .and(warp::query::<Optimizations>())
    //     .and(warp::any().map(move || cache_clone.clone()))
    //     .and_then(serve_static_file)
    //     .with(warp::wrap_fn(compression::auto(
    //         compression::Level::Best,
    //         compression::CompressContentType::default(),
    //     )));

    // let cache_clone = cache.clone();
    // let fetch_images = warp::path::end()
    //     .or(warp::head())
    //     .unify()
    //     .and(warp::query::<Optimizations>())
    //     .and(warp::query::<ExternalImage>())
    //     .and(warp::any().map(move || cache_clone.clone()))
    //     .and_then(fetch_and_serve_file)
    //     .with(warp::wrap_fn(compression::auto(
    //         compression::Level::Best,
    //         compression::CompressContentType::default(),
    //     )));

    // let shutdown = async move {
    //     signal::ctrl_c().await.expect("shutdown server");
    //     println!("server shutting down");
    // };
    // let addr = ([0, 0, 0, 0], options.port);
    // let routes = static_images.or(fetch_images);
    // warp::serve(routes).run(addr).await;
    Ok(())
}
