#![allow(warnings)]

use anyhow::Result;
use clap::Parser;
use imop::compression;
use imop::conditionals::{conditionals, Conditionals};
use imop::file::{path_from_tail, serve_file, ArcPath, File};
use imop::image::Optimizations;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::broadcast;
use warp::{Filter, Rejection};

async fn file_reply(
    path: ArcPath,
    conditionals: Conditionals,
    options: Optimizations,
) -> Result<File, Rejection> {
    println!("{:?}", path);
    // todo: need to get the mime type
    // todo: parse the compression options
    // todo: look up if the file was already compressed, if so, get it from the disk cache
    // todo: if not, compress and save to cache
    // todo: serve the correct file
    serve_file(path, conditionals).await
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
    // let clo = |filter| compression::compress(12, filter);
    let images = warp::path("images")
        .or(warp::head())
        .unify()
        .and(path_from_tail(base))
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
        .with(warp::wrap_fn(compression::brotli(compression::Level::Best)));
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
    let (_addr, server) =
        warp::serve(routes).bind_with_graceful_shutdown(([0, 0, 0, 0], options.port), async move {
            server_shutdown_rx.recv().await.expect("shutdown server");
            println!("server shutting down");
        });

    let tserver = tokio::task::spawn(server);

    if (signal::ctrl_c().await).is_ok() {
        println!("received shutdown");
        println!("waiting for pending tasks to complete...");
        shutdown_tx.send(()).expect("shutdown");
    };

    tserver.await.expect("server terminated");
    println!("exiting");
}
