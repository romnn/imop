use super::bounds::{Bounds, ScalingMode, Size};
use super::headers::HeaderMapExt;
pub use image::ImageFormat;
use image::{
    codecs, imageops, io::Reader as ImageReader, DynamicImage, ImageEncoder, ImageOutputFormat,
    RgbaImage,
};
use mime_guess::mime;
use serde::Deserialize;
use std::borrow::Cow;
use std::io::{self, BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

const DEFAULT_JPEG_QUALITY: u8 = 70; // 1-100

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("image error: `{0}`")]
    Image(#[from] image::error::ImageError),

    #[error("io error: `{0}`")]
    Io(#[from] std::io::Error),
}

#[derive(Deserialize, Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub struct Optimizations {
    /// quality value for JPEG (0 to 100)
    pub quality: Option<u8>,
    /// width of the image
    pub width: Option<u32>,
    /// height of the image
    pub height: Option<u32>,
    /// mode of scaling
    pub mode: Option<ScalingMode>,
    /// encoding format
    #[serde(default)]
    #[serde(deserialize_with = "image_format_from_ext")]
    pub format: Option<ImageFormat>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ExternalImage {
    #[serde(default)]
    #[serde(deserialize_with = "url_from_string")]
    /// URL to the external image
    pub image: Option<reqwest::Url>,
}

impl Optimizations {
    pub fn bounds(&self) -> Bounds {
        Bounds {
            width: self.width,
            height: self.height,
            mode: self.mode,
        }
    }
}

pub fn mime_of_format(format: ImageFormat) -> Option<mime::Mime> {
    match format {
        ImageFormat::Png => Some(mime::IMAGE_PNG),
        ImageFormat::Jpeg => Some(mime::IMAGE_JPEG),
        ImageFormat::Gif => Some(mime::IMAGE_GIF),
        ImageFormat::WebP => "image/webp".parse().ok(),
        ImageFormat::Pnm => "image/x-portable-bitmap".parse().ok(),
        ImageFormat::Tiff => "image/tiff".parse().ok(),
        ImageFormat::Tga => "image/x-tga".parse().ok(),
        ImageFormat::Dds => "image/vnd-ms.dds".parse().ok(),
        ImageFormat::Bmp => Some(mime::IMAGE_BMP),
        ImageFormat::Ico => "image/x-icon".parse().ok(),
        ImageFormat::Hdr => "image/vnd.radiance".parse().ok(),
        ImageFormat::OpenExr => None,
        ImageFormat::Farbfeld => None,
        ImageFormat::Avif => "image/avif".parse().ok(),
        _ => None,
    }
}

fn image_format_from_ext<'de, D>(deserializer: D) -> Result<Option<ImageFormat>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<Cow<'de, str>> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) => {
            let fmt = ImageFormat::from_extension(s.as_ref())
                .ok_or(image::error::UnsupportedError::from_format_and_kind(
                    image::error::ImageFormatHint::Unknown,
                    image::error::UnsupportedErrorKind::Format(
                        image::error::ImageFormatHint::Name(s.to_string()),
                    ),
                ))
                .map_err(image::error::ImageError::Unsupported)
                .map_err(serde::de::Error::custom)?;
            Ok(Some(fmt))
        }
    }
}

fn url_from_string<'de, D>(deserializer: D) -> Result<Option<reqwest::Url>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<Cow<'de, str>> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(ref s) => Ok(Some(
            reqwest::Url::parse(s).map_err(serde::de::Error::custom)?,
        )),
    }
}

// #[inline]
// pub fn clamp<T: PartialOrd>(input: T, min: T, max: T) -> T {
//     debug_assert!(min <= max, "min must be less than or equal to max");
//     if input < min {
//         min
//     } else if input > max {
//         max
//     } else {
//         input
//     }
// }

// #[inline]
// pub fn fit_to_bounds(width: u32, height: u32, bounds: &Bounds) -> Option<(u32, u32)> {
//     // clamp bounds, as we dont allow enlargement
//     let bwidth = bounds.width.map(|w| clamp(w, 1, width));
//     let bheight = bounds.height.map(|h| clamp(h, 1, height));
//     match (bwidth, bheight) {
//         (None, None) => None,
//         (Some(w), None) => Some((w, height)),
//         (None, Some(h)) => Some((width, h)),
//         (Some(w), Some(h)) => Some((w, h)),
//     }
// }

#[derive(Debug)]
pub struct Image {
    inner: DynamicImage,
    format: Option<ImageFormat>,
    size: Size,
}

impl std::ops::Deref for Image {
    type Target = DynamicImage;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for Image {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Debug)]
pub struct EncodedImage {
    pub buffer: Vec<u8>,
    pub format: image::ImageFormat,
}

impl warp::Reply for EncodedImage {
    fn into_response(self) -> warp::reply::Response {
        let len = self.buffer.len() as u64;
        let buffer = std::io::Cursor::new(self.buffer);
        let stream = super::file::file_stream(buffer, (0, len), None);
        let body = warp::hyper::Body::wrap_stream(stream);
        let mut resp = warp::reply::Response::new(body);

        resp.headers_mut()
            .typed_insert(super::headers::ContentLength(len));
        resp.headers_mut()
            .typed_insert(super::headers::ContentType::from(
                mime_of_format(self.format).unwrap_or(mime::IMAGE_STAR),
            ));
        resp.headers_mut()
            .typed_insert(super::headers::AcceptRanges::bytes());
        resp
    }
}

// impl From<EncodedImage> for super::file::File {
//     fn from(image: EncodedImage) -> Self {
//         // CliError::IoError(error)
//         superFile {
//             image.into_response(),
//             origin:
//     }
// }

// impl warp::Reply for Image {
//     fn into_response(self) -> warp::Response {
//         self.encoded
//         let stream = file_stream(self.buffer, (0, self.buffer.len()), None);
//         let body = warp::hyper::Body::wrap_stream(stream);
//         let mut resp = warp::reply::Response::new(body);

//         resp.headers_mut()
//             .typed_insert(imop::headers::ContentLength(self.buffer.len()));
//         resp.headers_mut()
//             .typed_insert(imop::headers::ContentType::from(
//                 mime_of_format(self.format).unwrap_or(mime::IMAGE_STAR),
//             ));
//         resp.headers_mut()
//             .typed_insert(imop::headers::AcceptRanges::bytes());
//         resp
//     }
// }

impl Image {
    pub fn new<R: std::io::BufRead + std::io::Seek>(reader: R) -> Result<Self, Error> {
        let now = Instant::now();
        let reader = ImageReader::new(reader).with_guessed_format()?;
        let format = reader.format();
        let inner = reader.decode()?;
        let size = Size {
            width: inner.width(),
            height: inner.height(),
        };
        crate::debug!("image decode took {:?}", now.elapsed());
        Ok(Self {
            inner,
            format,
            size,
        })
    }

    // pub fn content_length(&self) -> usize {
    // }
    // pub fn into_response(self) -> warp::Response {
    //     // self.resp
    //     // resp.headers_mut()
    //     //     .typed_insert(imop::headers::ContentLength(len as u64));
    //     // resp.headers_mut()
    //     //     .typed_insert(imop::headers::ContentType::from(
    //     //         mime_of_format(target_format).unwrap_or(mime::IMAGE_STAR),
    //     //     ));
    //     // resp.headers_mut()
    //     //     .typed_insert(imop::headers::AcceptRanges::bytes());
    // }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        use std::fs::File;
        Self::new(BufReader::new(File::open(path)?))
    }

    pub fn resize(&mut self, bounds: Bounds) {
        let now = Instant::now();
        let new_size = self.size.fit_to_bounds(bounds).unwrap();
        self.inner = self.inner.resize_exact(
            new_size.width,
            new_size.height,
            imageops::FilterType::Lanczos3,
        );
        crate::debug!("fitting to {} took {:?}", new_size, now.elapsed());

        // let (w, h) = self.size;
        // if let Some((w, h)) = fit_to_bounds(w, h, bounds) {
        //     self.inner = self
        //         .inner
        //         .resize_exact(w, h, imageops::FilterType::Lanczos3);
        //     crate::debug!("fitting to {} x {} took {:?}", w, h, now.elapsed());
        // };
    }

    pub fn format(&self) -> Option<ImageFormat> {
        self.format
    }

    pub fn encode_to<W: std::io::Write + Seek>(
        &self,
        w: &mut W,
        format: ImageFormat,
        quality: Option<u8>,
    ) -> Result<(), Error> {
        let now = Instant::now();
        let data = self.inner.as_bytes();
        let color = self.inner.color();
        let width = self.inner.width();
        let height = self.inner.height();
        match format.into() {
            ImageOutputFormat::Png => codecs::png::PngEncoder::new(w)
                .write_image(data, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Jpeg(_) => {
                let quality = quality.unwrap_or(DEFAULT_JPEG_QUALITY);
                codecs::jpeg::JpegEncoder::new_with_quality(w, quality)
                    .write_image(data, width, height, color)
                    .map_err(Error::from)
            }
            ImageOutputFormat::Gif => codecs::gif::GifEncoder::new(w)
                .encode(data, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Ico => codecs::ico::IcoEncoder::new(w)
                .write_image(data, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Bmp => codecs::bmp::BmpEncoder::new(w)
                .write_image(data, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Tiff => codecs::tiff::TiffEncoder::new(w)
                .write_image(data, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Unsupported(msg) => {
                Err(Error::from(image::error::ImageError::Unsupported(
                    image::error::UnsupportedError::from_format_and_kind(
                        image::error::ImageFormatHint::Unknown,
                        image::error::UnsupportedErrorKind::Format(
                            image::error::ImageFormatHint::Name(msg),
                        ),
                    ),
                )))
            }
            _ => Err(Error::from(image::error::ImageError::Unsupported(
                image::error::UnsupportedError::from_format_and_kind(
                    image::error::ImageFormatHint::Unknown,
                    image::error::UnsupportedErrorKind::Format(
                        image::error::ImageFormatHint::Name("missing format".to_string()),
                    ),
                ),
            ))),
        }?;
        crate::debug!("encoding took {:?}", now.elapsed());
        Ok(())
    }

    pub fn encode(&self, format: ImageFormat, quality: Option<u8>) -> Result<EncodedImage, Error> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        self.encode_to(&mut buffer, format, quality)?;
        Ok(EncodedImage {
            buffer: buffer.into_inner(),
            format,
        })
    }
}
