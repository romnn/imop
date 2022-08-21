#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io error: `{0}`")]
    Io(#[from] std::io::Error),
}

impl warp::reject::Reject for Error {}
