use super::error::Error;
use super::image::{CachedImage, ImageCache};
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

#[derive(Debug, Clone)]
pub struct InMemoryImage {
    data: Arc<Vec<u8>>,
    format: Option<image::ImageFormat>,
    content_length: usize,
}

impl AsRef<[u8]> for InMemoryImage {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl CachedImage for InMemoryImage {
    type Data = std::io::Cursor<InMemoryImage>;

    fn format(&self) -> Option<image::ImageFormat> {
        self.format
    }

    fn content_length(&self) -> usize {
        self.content_length
    }

    fn data(&self) -> Self::Data {
        // std::io::Cursor<InMemoryImage> {
        std::io::Cursor::new(self.clone())
    }
}

pub struct InMemoryImageCache<K>
where
    K: Hash + Eq,
{
    inner: RwLock<caches::RawLRU<K, InMemoryImage>>,
}

impl<K> InMemoryImageCache<K>
where
    K: Hash + Eq,
{
    pub fn new(capacity: Option<usize>) -> Self {
        let inner = RwLock::new(caches::RawLRU::new(capacity.unwrap()).unwrap());
        Self { inner }
    }
}

#[async_trait]
// , std::io::Cursor<InMemoryImage>
impl<K> ImageCache<K, InMemoryImage> for InMemoryImageCache<K>
where
    K: Hash + Eq + Sync + Send,
{
    #[inline]
    async fn put<D: tokio::io::AsyncRead + std::marker::Unpin + Send>(
        &self,
        k: K,
        mut data: D,
        format: Option<image::ImageFormat>,
    ) -> Result<Option<InMemoryImage>, Error> {
        let mut buffer = Vec::new();
        data.read_to_end(&mut buffer).await.map_err(Error::from)?;
        let content_length = buffer.len();
        let entry = InMemoryImage {
            data: Arc::new(buffer),
            format,
            content_length,
        };
        let mut lock = self.inner.write();
        lock.put(k, entry);
        Ok(None)
    }

    #[inline]
    async fn get(&self, k: &K) -> Option<InMemoryImage> {
        let mut lock = self.inner.write();
        lock.get(k).map(|v| v.clone())
    }
}
