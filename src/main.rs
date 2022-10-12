#![allow(warnings)]

use clap::Parser;
use imop::conditionals::{conditionals, Conditionals};
use imop::file::{self, File};
use imop::headers::ContentType;
use imop::image::Optimizations;
use imop::{compression, mime};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{signal, sync::broadcast};
use warp::{Filter, Rejection};

#[derive(Parser, Serialize, Deserialize, Debug, Clone)]
#[clap(version = "1.0", author = "romnn <contact@romnn.com>")]
struct Options {
    #[clap(short = 'i', long = "images", help = "image source path")]
    image_path: PathBuf,

    #[clap(short = 'p', long = "port", default_value = "3000")]
    port: u16,

    #[clap(short = 'n', long = "pages", help = "max number of pages to scape")]
    max_pages: Option<u32>,

    #[clap(short = 'r', long = "retain", help = "days to retain")]
    retain_days: Option<u64>,
}

async fn file_reply(
    path: file::Path,
    conditionals: Conditionals,
    options: Optimizations,
) -> Result<File, Rejection> {
    println!("{:?}", path);
    // todo: need to get the mime type
    // todo: parse the compression options
    // todo: look up if the file was already compressed, if so, get it from the disk cache
    // todo: if not, compress and save to cache
    // todo: serve the correct file
    file::serve(path, conditionals).await
}

#[tokio::main]
async fn main() {
    let options= Options::parse();
    // println!(
    //     "{}",
    //     serde_json::to_string_pretty(&options).expect("options")
    // );

    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut shutdown_rx = shutdown_tx.subscribe();

    let health = warp::path!("healthz").and(warp::get()).map(|| "healthy");

    let base = Arc::new(options.image_path);
    // let clo = |filter| compression::compress(12, filter);
    let images = warp::path("images")
        .or(warp::head())
        .unify()
        .and(file::path_from_tail(base))
        .and(conditionals())
        .and(warp::query::<Optimizations>())
        .and_then(file_reply)
        // .with(warp::compression::brotli());
        // .with(warp::compression::gzip());
        // .with(warp::compression::gzip());
        // .with(warp::wrap_fn(compression::compress(
        //     compression::CompressionAlgo::BR,
        //     compression::Level::Best,
        // )));
        // .with(warp::wrap_fn(compression::brotli(compression::Level::Best)));
        // .with(warp::wrap_fn(compression::auto(compression::Level::Best)));
        .with(warp::wrap_fn(compression::brotli(
            compression::Level::Best,
            // compression::CompressContentType::All,
            compression::CompressContentType::include(vec![mime_guess::mime::IMAGE_STAR]),
        )));
    // .with(warp::wrap_fn(|filter| compression::compress(12, filter)));
    // .with(warp::wrap_fn(clo));
    // .with(warp::wrap_fn(compression::compress_wrap(12)));
    // #[cfg(feature = "compression")]
    // let images = {
    //     // let algo = compression::CompressionAlgo::BR;
    //     // let compressor = compression::auto();
    //     // let compressor: warp::compression::Compression<Box<dyn Fn(_) -> Response + Copy>> =
    //     //     Box::new(match algo {
    //     //         CompressionAlgo::BR => warp::compression::brotli(),
    //     //         CompressionAlgo::DEFLATE => warp::compression::deflate(),
    //     //         CompressionAlgo::GZIP => warp::compression::gzip(),
    //     //     });
    //     images.with(compression::compress)
    //     // images.and_then(compression::compress())
    //     // images.with(warp::wrap_fn(|filter| {
    //     //     compression::compress(filter)
    //     // }))
    // };
    // #[cfg(not(feature = "compression"))]
    // let images = images.and_then(file_reply);

    let routes = images.or(health);
    let addr = ([0, 0, 0, 0], options.port);
    let shutdown = async move {
        shutdown_rx.recv().await.expect("shutdown server");
        println!("server shutting down");
    };
    let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, shutdown);

    let server_task = tokio::task::spawn(server);

    if (signal::ctrl_c().await).is_ok() {
        println!("received shutdown");
        println!("waiting for pending tasks to complete...");
        shutdown_tx.send(()).expect("shutdown");
    };

    server_task.await.expect("server terminated");
    println!("exiting");
}
