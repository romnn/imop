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

// pub trait CachedImage<R> {
pub trait CachedImage {
    type Data: tokio::io::AsyncRead + tokio::io::AsyncSeek;

    fn format(&self) -> Option<image::ImageFormat>;
    fn content_length(&self) -> usize;
    fn data(&self) -> Self::Data;
}

#[async_trait]
pub trait ImageCache<K, I>
// , R>
// where
//     I: CachedImage<R>,
// R: tokio::io::AsyncRead + tokio::io::AsyncSeek,
{
    // type Image = CachedImage<R>
    // async fn contains<Q>(&self, id: &Q) -> bool
    // fn contains<Q>(&self, id: &Q) -> bool
    // where
    //     Q: AsRef<K> + Hash + Eq + ?Sized;

    async fn get(&self, k: &K) -> Option<I>;

    async fn put<D: tokio::io::AsyncRead + std::marker::Unpin + Send>(
        &self,
        k: K,
        data: D,
        format: Option<image::ImageFormat>,
    ) -> Result<Option<I>, Error>;
}
