pub use image::ImageFormat;
use image::{
    codecs, imageops, io::Reader as ImageReader, DynamicImage, ImageEncoder, ImageOutputFormat,
    RgbaImage,
};
use serde::Deserialize;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Deserialize, Debug, Clone, Copy)]
pub enum ScalingMode {
    /// Fit into wxh if both are given.
    /// Only keeps aspect ratio if at most a single dimension is given
    Exact,
    /// Fit into wxh if both are given while keeping aspect ratio
    /// If at most one dimension is given, falls back to ``ScalingMode::exact``
    Fit,
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct Bounds {
    /// width of the image
    pub width: Option<u32>,
    /// height of the image
    pub height: Option<u32>,
    /// mode of scaling
    pub mode: Option<ScalingMode>,
}

// use serde::de::{self, Visitor};

// struct ImageFormatVisitor;

// impl<'de> serde::de::Visitor<'de> for ImageFormatVisitor {
//     type Value = ImageFormat;

//     fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//         formatter.write_str("a valid image format")
//     }

//     fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
//     where
//         E: de::Error,
//     {
//         Ok(ImageFormat::Jpeg)
//     }
// }

// impl<'de> serde::Deserialize<'de> for ImageFormat {
//     fn deserialize<D>(deserializer: D) -> Result<ImageFormat, D::Error>
//     where
//         D: serde::de::Deserializer<'de>,
//     {
//         deserializer.deserialize_string(I32Visitor)
//     }
// }

// #[derive(Deserialize)]
// #[serde(remote = "ImageFormat")]
// enum ImageFormatDef {
//     Png,
//     Jpeg,
//     Gif,
//     WebP,
//     Pnm,
//     Tiff,
//     Tga,
//     Dds,
//     Bmp,
//     Ico,
//     Hdr,
//     OpenExr,
//     Farbfeld,
//     Avif,
// }

fn from_image_format_ext<'de, D>(deserializer: D) -> Result<Option<ImageFormat>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<&str> = Option::deserialize(deserializer)?;
    // let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) => {
            let fmt = ImageFormat::from_extension(s)
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
    // match s {
    //     "test" => Ok(Some(ImageFormat::Jpeg)),
    //     _ => Err(D::Error::custom(),
    // }
    // do better hex decoding than this
    // u64::from_str_radix(&s[2..], 16).map_err(D::Error::custom)
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct Optimizations {
    /// quality value for JPEG (0 to 100)
    pub quality: Option<u8>,
    // #[serde(flatten)]
    // pub bounds: Bounds,
    /// width of the image
    pub width: Option<u32>,
    /// height of the image
    pub height: Option<u32>,
    /// mode of scaling
    pub mode: Option<ScalingMode>,
    /// encoding format
    #[serde(default)]
    #[serde(deserialize_with = "from_image_format_ext")]
    pub format: Option<ImageFormat>,
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

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("image error: `{0}`")]
    Image(#[from] image::error::ImageError),

    #[error("io error: `{0}`")]
    Io(#[from] std::io::Error),
}

impl warp::reject::Reject for Error {}

// impl From<image::ImageFormat> for super::headers::ContentType {
//     fn from(f: image::ImageFormat) -> Self {
//         super::headers::ContentType::from("test")
//     }
// }

#[inline]
pub fn clamp<T: PartialOrd>(input: T, min: T, max: T) -> T {
    debug_assert!(min <= max, "min must be less than or equal to max");
    if input < min {
        min
    } else if input > max {
        max
    } else {
        input
    }
}

#[inline]
pub fn fit_to_bounds(width: u32, height: u32, bounds: &Bounds) -> Option<(u32, u32)> {
    // clamp bounds, as we dont allow enlargement
    let bwidth = bounds.width.map(|w| clamp(w, 1, width));
    let bheight = bounds.height.map(|h| clamp(h, 1, height));
    match (bwidth, bheight) {
        (None, None) => None,
        (Some(w), None) => Some((w, height)),
        (None, Some(h)) => Some((width, h)),
        (Some(w), Some(h)) => Some((w, h)),
    }
}

#[derive(Debug)]
pub struct Image {
    // inner: RgbaImage,
    inner: DynamicImage,
    format: Option<ImageFormat>,
    size: (u32, u32),
}

const DEFAULT_JPEG_QUALITY: u8 = 70; // 1-100

impl Image {
    pub fn new<R: std::io::BufRead + std::io::Seek>(reader: R) -> Result<Self, Error> {
        let reader = ImageReader::new(reader).with_guessed_format()?;
        let format = reader.format();
        let inner = reader.decode()?;
        // .to_rgba8();
        let size = (inner.width(), inner.height());
        Ok(Self {
            inner,
            format,
            size,
        })
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        Self::new(BufReader::new(File::open(path)?))
    }

    pub fn resize(&mut self, bounds: &Bounds) {
        let now = Instant::now();
        let (w, h) = self.size;
        if let Some((w, h)) = fit_to_bounds(w, h, bounds) {
            self.inner = self
                .inner
                .resize_exact(w, h, imageops::FilterType::Lanczos3);
            println!("fitting to {} x {} took {:?}", w, h, now.elapsed());
        };
    }

    pub fn format(&self) -> Option<ImageFormat> {
        self.format
    }

    pub fn encode<W: std::io::Write + Seek>(
        &self,
        w: &mut W,
        format: ImageFormat,
        quality: Option<u8>,
    ) -> Result<(), Error> {
        let now = Instant::now();
        let buf = self.inner.as_bytes();
        // let buf = match self.inner {
        //     DynamicImage::ImageLuma8(img) => img.inner_pixels().as_bytes(),
        // };
        let color = self.inner.color();
        let width = self.inner.width();
        let height = self.inner.height();
        match format.into() {
            ImageOutputFormat::Png => codecs::png::PngEncoder::new(w)
                .write_image(buf, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Jpeg(_) => {
                let quality = quality.unwrap_or(DEFAULT_JPEG_QUALITY);
                codecs::jpeg::JpegEncoder::new_with_quality(w, quality)
                    .write_image(buf, width, height, color)
                    .map_err(Error::from)
            }
            ImageOutputFormat::Gif => codecs::gif::GifEncoder::new(w)
                .encode(buf, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Ico => codecs::ico::IcoEncoder::new(w)
                .write_image(buf, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Bmp => codecs::bmp::BmpEncoder::new(w)
                .write_image(buf, width, height, color)
                .map_err(Error::from),
            ImageOutputFormat::Tiff => codecs::tiff::TiffEncoder::new(w)
                .write_image(buf, width, height, color)
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
        println!("encoding took {:?}", now.elapsed());
        Ok(())
    }

    pub fn encode_jpeg(&mut self, quality: u8) {
        // let mut encoder = JpegEncoder::new_with_quality(&mut file, quality.unwrap_or(80));
        // encoder.encode_image(&DynamicImage::ImageRgba8(buffer))?;
    }
}

// let output_path = self.get_output_path(output_path)?;
//         println!("saving to {}...", output_path.display());
//         let mut file = File::create(&output_path)?;
//         encoder.encode_image(&DynamicImage::ImageRgba8(buffer))?;
//
// resize the image to fit the screen
// let (mut fit_width, mut fit_height) = utils::resize_dimensions(
//     photo.width(),
//     photo.height(),
//     size.width,
//     size.height,
//     false,
// );

// if let Some(scale_factor) = options.scale_factor {
//     // scale the image by factor
//     fit_width = (fit_width as f32 * utils::clamp(scale_factor, 0f32, 1f32)) as u32;
//     fit_height = (fit_height as f32 * utils::clamp(scale_factor, 0f32, 1f32)) as u32;
//     // println!("scaling to {} x {}", fit_width, fit_height);
// };
// }
