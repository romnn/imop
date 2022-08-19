use anyhow::Result;
use clap::Parser;
use imop::conditionals::{conditionals, Conditionals};
use imop::file::{path_from_tail, serve_file, ArcPath, File};
use imop::image::Optimizations;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use warp::{Filter, Rejection};

#[derive(Parser)]
pub struct Options {
    #[clap(short = 'i', long = "images", help = "image source path")]
    image_path: PathBuf,

    #[clap(short = 'p', long = "port", default_value = "3000")]
    port: u16,
}

async fn file_reply(
    path: ArcPath,
    conditionals: Conditionals,
    optimizations: Optimizations,
) -> Result<File, Rejection> {
    println!("{:?}", path);
    println!("{:?}", optimizations);
    // todo: need to get the mime type
    // todo: parse the compression options
    // todo: look up if the file was already compressed, if so, get it from the disk cache
    // todo: if not, compress and save to cache
    // todo: serve the correct file
    // let conditionals = Conditionals::default();
    serve_file(path, conditionals).await
}

#[derive(Deserialize, Debug)]
pub struct ImageSource {
    // /// quality value for JPEG (0 to 100)
// pub quality: Option<u8>,
// #[serde(flatten)]
// bounds: Bounds,
}

#[tokio::main]
async fn main() -> Result<()> {
    let options: Options = Options::parse();
    let base = Arc::new(options.image_path);
    let images = warp::get()
        .or(warp::head())
        .unify()
        .and(path_from_tail(base))
        .and(conditionals())
        // .and(warp::query::<ImageSource>())
        .and(warp::query::<Optimizations>())
        .and_then(file_reply);

    let shutdown = async move {
        signal::ctrl_c().await.expect("shutdown server");
        println!("server shutting down");
    };
    let addr = ([0, 0, 0, 0], options.port);
    let (_, server) = warp::serve(images).bind_with_graceful_shutdown(addr, shutdown);

    server.await;
    Ok(())
}
