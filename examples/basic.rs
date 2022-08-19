use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use imop::cache::InMemoryCache;
use imop::conditionals::{conditionals, Conditionals};
use imop::file::{file_stream, path_from_tail, serve_file, ArcPath, File};
use imop::headers::HeaderMapExt;
use imop::headers::{AcceptRanges, ContentLength, ContentType};
use imop::image::{Image, ImageFormat, Optimizations};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::signal;
use warp::{Filter, Rejection};

#[derive(Parser)]
pub struct Options {
    #[clap(short = 'i', long = "images", help = "image source path")]
    image_path: PathBuf,

    #[clap(short = 'p', long = "port", default_value = "3000")]
    port: u16,
}

async fn file_reply<K, V>(
    path: ArcPath,
    conditionals: Conditionals,
    optimizations: Optimizations,
    cache: InMemoryCache<K, V>,
) -> Result<File, Rejection> {
    println!("{:?}", path);
    println!("{:?}", optimizations);

    let mut img = Image::open(&path)?;
    // let mime =
    let mut encoded = std::io::Cursor::new(Vec::new());
    img.resize(&optimizations.bounds());
    img.encode(&mut encoded, ImageFormat::Jpeg, optimizations.quality)?;
    let len = encoded.position() as u64;
    encoded.set_position(0);
    let stream = file_stream(encoded, (0, len), None);
    let body = warp::hyper::Body::wrap_stream(stream);
    let mut resp = warp::reply::Response::new(body);

    // let mime = mime_guess::from_path(path.as_ref()).first_or_octet_stream();

    resp.headers_mut()
        .typed_insert(imop::headers::ContentLength(len as u64));
    // resp.headers_mut().typed_insert(imop::headers::ContentType::from(mime));
    resp.headers_mut()
        .typed_insert(imop::headers::AcceptRanges::bytes());

    Ok(File { resp, path })
    // todo: need to get the mime type
    // todo: parse the compression options
    // todo: look up if the file was already compressed, if so, get it from the disk cache
    // todo: if not, compress and save to cache
    // todo: serve the correct file
    // let conditionals = Conditionals::default();
    // serve_file(path, conditionals).await
}

#[tokio::main]
async fn main() -> Result<()> {
    let options: Options = Options::parse();
    let base = Arc::new(options.image_path);

    let cache = InMemoryCache::new(Some(10));

    let static_images = warp::path("static")
        .or(warp::head())
        .unify()
        .and(path_from_tail(base))
        .and(conditionals())
        .and(warp::query::<Optimizations>())
        .and(warp::any().map(move || cache))
        .and_then(file_reply);

    // let fetch_images = warp::get()
    //     .or(warp::head())
    //     .unify()
    //     // .and(path_from_tail(base))
    //     .and(warp::query::<Optimizations>())
    //     .and(warp::any().map(move || cache))
    //     .and_then(file_reply);

    // .then(|path: ArcPath, optimizations: Optimizations| async move {
    //     // check if the request is cached
    //     // return
    //     // if true {
    //     //     println!("found in cache");
    //     //     return Err(warp::reject::not_found());
    //     // }
    //     // // println!("serving the real file");
    //     let cached = false
    //     (path, optimizations, cached)
    // })
    // .and_then(|(path, optimizations)| async move {
    //     // : ArcPath, optimizations: Optimizations)| async move {
    //     println!("serving the real file");
    //     Ok::<warp::http::status::StatusCode, std::convert::Infallible>(
    //         warp::http::status::StatusCode::OK,
    //     )
    //     // Ok(warp::reply::html("hello"))
    //     // Ok::<String, Rejection>(warp::reply())
    //     // return Err(warp::reject::not_found());
    //     // Ok((path, optimizations))
    // });
    // .and_then(cache);
    // .or(file_reply);

    let shutdown = async move {
        signal::ctrl_c().await.expect("shutdown server");
        println!("server shutting down");
    };
    let addr = ([0, 0, 0, 0], options.port);
    let (_, server) = warp::serve(static_images).bind_with_graceful_shutdown(addr, shutdown);

    server.await;
    Ok(())
}
