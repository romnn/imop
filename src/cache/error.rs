#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("cache entry not found")]
    NotFound,

    // this should never happen
    #[error("no capacity for new entry")]
    NoCapacity,

    #[error("io error: `{0}`")]
    Io(#[from] std::io::Error),

    #[error("invalid image: `{0}`")]
    Invalid(#[from] crate::image::Error),
}
