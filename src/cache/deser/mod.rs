use async_trait::async_trait;
use std::hash::Hash;

#[derive(thiserror::Error, Debug)]
pub enum Error<S>
where
    S: std::error::Error,
{
    #[error("not implemented")]
    NotImplemented,

    #[error(transparent)]
    Io(std::io::Error),

    #[error(transparent)]
    Test(S),
}

#[async_trait]
pub trait Deserialize<V> {
    type Error: std::error::Error;

    async fn deserialize_from_async<R>(&self, reader: &mut R) -> Result<V, Error<Self::Error>>
    where
        R: tokio::io::AsyncRead + Send + Sync,
    {
        Err(Error::NotImplemented)
    }

    fn deserialize_from<R>(&self, reader: &mut R) -> Result<V, Error<Self::Error>>
    where
        R: std::io::Read,
    {
        Err(Error::NotImplemented)
    }
}

#[async_trait]
pub trait Serialize<V>
where
    V: Sync,
{
    type Error: std::error::Error;

    async fn serialize_to_async<W>(
        &self,
        value: &V,
        writer: &mut W,
    ) -> Result<(), Error<Self::Error>>
    where
        W: tokio::io::AsyncWrite + Send,
    {
        Err(Error::NotImplemented)
    }

    fn serialize_to<W>(&self, value: &V, writer: &mut W) -> Result<(), Error<Self::Error>>
    where
        W: std::io::Write,
    {
        Err(Error::NotImplemented)
    }
}
