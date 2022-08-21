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

pub struct FileImage {
    path: PathBuf,
}

#[async_trait]
impl CachedImage for FileImage {
    type Data = tokio::fs::File;

    async fn format(&self) -> image::ImageFormat {
        let mime = mime_guess::from_path(&self.path).first();
        let format = mime.and_then(image::ImageFormat::from_mime_type);
        format.unwrap_or(image::ImageFormat::Jpeg)
    }

    async fn content_length(&self) -> Result<usize, Error> {
        // get the file size
        let file = tokio::fs::File::open(&self.path)
            .await
            .map_err(Error::from)?;
        let meta = file.metadata().await.map_err(Error::from)?;
        Ok(meta.len() as usize)
    }

    async fn data(&self) -> Result<Self::Data, Error> {
        tokio::fs::File::open(&self.path).await.map_err(Error::from)
    }
}

struct EvictionHandler {}

impl caches::OnEvictCallback for EvictionHandler {
    fn on_evict<K, V>(&self, key: &K, _: &V) {
        // TODO: delete file based on key
    }
}

pub struct FileSystemImageCache<K>
where
    K: Hash + Eq,
{
    inner: caches::RawLRU<K, FileImage, EvictionHandler>,
    cache_dir: PathBuf,
}

impl<K> FileSystemImageCache<K>
where
    K: Hash + Eq,
{
    pub fn new<P: Into<PathBuf>>(cache_dir: P, capacity: usize) -> Self {
        let inner = caches::RawLRU::with_on_evict_cb(capacity, EvictionHandler {}).unwrap();
        Self {
            inner,
            cache_dir: cache_dir.into(),
        }
    }
}

// impl<K> Cache<K, IMData> for FileSystemCache<K>
// where
//     K: Hash + Eq,
// {
//     fn get(&self, k: &K) -> Option<MappedRwLockWriteGuard<'_, IMData>> {
//     }
// }
