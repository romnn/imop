#![allow(warnings)]

use anyhow::Result;
use clap::Parser;
use futures::stream::{StreamExt, TryStreamExt};
use imop::compression;
use imop::file::{File, Origin};
use imop::image::{Format as ImageFormat, Image, Optimizations};
use reqwest::Url;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::signal;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use warp::{Filter, Rejection, Reply};

#[inline]
pub fn url_from_string<'de, D>(deser: D) -> Result<Option<Url>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use std::borrow::Cow;
    let s: Option<Cow<'de, str>> = Option::deserialize(deser)?;
    match s {
        None => Ok(None),
        Some(ref s) => Ok(Some(Url::parse(s).map_err(serde::de::Error::custom)?)),
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ImageSource {
    #[serde(default)]
    #[serde(deserialize_with = "url_from_string")]
    /// URL to the external image
    pub image: Option<reqwest::Url>,
}

#[derive(Parser)]
struct Options {
    #[clap(short = 'p', long = "port", default_value = "3000")]
    port: u16,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("image error error: `{0}`")]
    Image(#[from] imop::image::Error),

    #[error("fetch error: `{0}`")]
    Fetch(#[from] reqwest::Error),
    // #[error("cache error: `{0}`")]
    // Cache(#[from] CacheError),
}

impl warp::reject::Reject for Error {}

async fn fetch_and_serve_file(
    optimizations: Optimizations,
    src: ImageSource,
    // cache: Arc<FileSystemImageCache<CacheKey>>,
    // cache: Arc<C>,
) -> Result<impl warp::Reply, Rejection> {
    // ) -> Result<File, Rejection> {
    imop::debug!("source = {:?}", &src);
    imop::debug!("optimizations = {:?}", &optimizations);

    match src.image {
        Some(url) => {
            let now = Instant::now();
            let res = reqwest::get(url.clone()).await.map_err(Error::from)?;
            // let data = res.bytes().await.map_err(Error::from)?;
            let mut buffer = Vec::new();
            let mut reader = tokio::io::BufReader::new(
                res.bytes_stream()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                    .into_async_read()
                    .compat(),
            );
            tokio::io::copy(&mut reader, &mut buffer).await;

            imop::debug!("download of {} took {:?}", &url, now.elapsed());

            let img = Image::new(std::io::Cursor::new(&buffer)).map_err(Error::from);
            let mut img = img?;

            let target_format = optimizations
                .format
                .or(img.format())
                .unwrap_or(ImageFormat::Jpeg);

            img.resize(optimizations.bounds());

            let mut buffer = Vec::new();
            let mut cursor = std::io::Cursor::new(buffer);
            // let mut writer = std::io::BufWriter::new(cursor);

            let encoded = img
                .encode_to(&mut cursor, target_format, optimizations.quality)
                .map_err(Error::from)?;

            // let file_stream = stream(file, (start, end), Some(buf_size));
            // let body = hyper::Body::wrap_stream(file_stream);

            // let mut resp = reply::Response::new(body);

            // let body = warp::hyper::Body::wrap_stream(writer);
            let body = warp::hyper::body::Bytes::from(cursor.into_inner());
            let mut resp = warp::reply::Response::new(body.into());
            // warp::hyper::body::Body::from(writer).into());
            // warp::reply::Response::new(warp::hyper::body::Bytes::from(writer).into());

            // Ok(File {
            //     resp: encoded.into_response(),
            //     origin: Origin::Url(url),
            // })
            Ok(resp)
            // Err(warp::reject::reject())
        }
        None => Err(warp::reject::reject()),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let options: Options = Options::parse();
    let image_endpoint = warp::path::end()
        .or(warp::head())
        .unify()
        .and(warp::query::<Optimizations>())
        .and(warp::query::<ImageSource>())
        // .and(warp::any().map(move || cache_clone.clone()))
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
    warp::serve(image_endpoint).run(addr).await;
    Ok(())
}
