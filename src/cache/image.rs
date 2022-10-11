use super::error::Error;
use async_trait::async_trait;
use caches::Cache as LRUCache;
use lru::LruCache;
use parking_lot::{MappedRwLockWriteGuard, RwLock, RwLockWriteGuard};
use std::borrow::Borrow;
use std::hash::Hash;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

#[async_trait]
pub trait CachedImage {
    type Data: tokio::io::AsyncRead + tokio::io::AsyncSeek;

    fn format(&self) -> image::ImageFormat;
    async fn content_length(&self) -> Result<usize, Error>;
    async fn data(&self) -> Result<Self::Data, Error>;
}

#[async_trait]
pub trait ImageCache<K, V>
where
    V: CachedImage,
{
    async fn get(&self, k: &K) -> Option<V>;

    async fn put<D: tokio::io::AsyncRead + std::marker::Unpin + Send>(
        &self,
        k: K,
        data: D,
        format: image::ImageFormat,
    ) -> Result<Option<V>, Error>;
}
