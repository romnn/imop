use serde::Deserialize;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    // #[error("image error: `{0}`")]
    // Image(#[from] image::error::ImageError),

    // #[error("io error: `{0}`")]
    // Io(#[from] std::io::Error),
}

#[derive(Deserialize, Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum ScalingMode {
    /// Fit into wxh if both are given.
    ///
    /// Only keeps aspect ratio if at most a single dimension is given
    Exact,

    /// Fit to wxh while keeping aspect ratio.
    ///
    /// If at most one dimension is given, the larger image dimension is scaled to
    /// fit into ``min(w, h)``.
    Fit,

    /// Fit to cover wxh while keeping aspect ratio.
    ///
    /// If at most one dimension is given, the smallest dimension is scaled up to
    /// cover ``min(w, h)``.
    Cover,
}

impl Default for ScalingMode {
    fn default() -> Self {
        Self::Fit
    }
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

#[derive(Debug, Clone, Copy)]
pub struct Size {
    /// width
    pub width: u32,
    /// height
    pub height: u32,
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl Size {
    #[inline]
    pub fn fit_to_bounds(self, bounds: Bounds) -> Result<Self, Error> {
        let size = match bounds {
            // unbounded
            Bounds {
                width: None,
                height: None,
                mode,
            } => Ok(self),
            // single dimension is bounded
            Bounds {
                width: None,
                height: Some(height),
                mode,
            } => self.fit(
                Size {
                    width: self.width,
                    height,
                },
                mode,
            ),
            Bounds {
                width: Some(width),
                height: None,
                mode,
            } => self.fit(
                Size {
                    width,
                    height: self.height,
                },
                mode,
            ),
            // all dimensions bounded
            Bounds {
                width: Some(width),
                height: Some(height),
                mode,
            } => self.fit(Size { width, height }, mode),
        };
        size
        // match size {
        //     Ok(scaled) => Ok(scaled),
        //     // Err(err) => Err(Error {
        //     //     // size: self,
        //     //     // bounds,
        //     //     // mode,
        //     //     // source: err.into(),
        //     // }),
        // }
    }

    #[inline]
    pub fn fit(self, size: Size, mode: Option<ScalingMode>) -> Result<Self, Error> {
        // let target = size.into();
        let mode = mode.unwrap_or_default();
        if mode == ScalingMode::Exact {
            return Ok(size);
        }
        todo!();
        return Ok(size);
        // match (|| {
        //     let scale = self.scale_factor(target, mode)?;
        //     let scaled = self.scale_by::<_, Ceil>(scale.0)?;
        //     Ok::<_, arithmetic::Error>(scaled)
        // })() {
        //     Ok(scaled_size) => Ok(scaled_size),
        //     Err(err) => Err(ScaleToError {
        //         size: self,
        //         target,
        //         mode,
        //         source: err,
        //     }),
        // }
    }
}
