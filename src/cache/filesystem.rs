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

struct CacheFile {
    path: PathBuf,
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
    inner: caches::RawLRU<K, CacheFile, EvictionHandler>,
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


