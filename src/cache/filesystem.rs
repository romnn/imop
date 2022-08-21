use super::error::Error;
use super::image::{CachedImage, ImageCache};
use super::lfu::LFUCache;
use super::Cache;
use super::PutResult;
use crate::image::ImageFormat;
use async_trait::async_trait;
use base64;
use digest::{generic_array::GenericArray, Digest};
use lru::LruCache;
use std::borrow::Borrow;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct FileImage {
    path: PathBuf,
    format: ImageFormat,
}

#[async_trait]
impl CachedImage for FileImage {
    type Data = tokio::fs::File;

    fn format(&self) -> image::ImageFormat {
        self.format
    }

    async fn content_length(&self) -> Result<usize, Error> {
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

// struct EvictionHandler {}

// impl caches::OnEvictCallback for EvictionHandler {
//     fn on_evict<K, V>(&self, key: &K, _: &V) {
//         // TODO: delete file based on key
//     }
// }

pub struct FileSystemImageCache<K>
where
    K: Clone + Hash + Eq,
{
    // inner: caches::RawLRU<K, FileImage, EvictionHandler>,
    inner: RwLock<LFUCache<K, FileImage>>,
    cache_dir: PathBuf,
}

// fn create_hash<V, D>(v: &V, mut hasher: D) -> String
// where
//     V: Hash,
//     D: Digest,
//     digest::Output<D>: std::fmt::LowerHex,
// {
//     hasher.update(v);
//     format!("{:x}", hasher.finalize())
// }

fn guess_format<P: AsRef<Path>>(path: P) -> Option<image::ImageFormat> {
    let mime = mime_guess::from_path(&path).first();
    mime.and_then(image::ImageFormat::from_mime_type)
}

impl<K> FileSystemImageCache<K>
where
    K: Clone + Hash + Eq,
{
    pub fn new<P: Into<PathBuf> + AsRef<Path>>(cache_dir: P, capacity: usize) -> Self {
        // let inner = caches::RawLRU::with_on_evict_cb(capacity, EvictionHandler {}).unwrap();
        let inner = RwLock::new(LFUCache::with_capacity(capacity));
        let _ = std::fs::create_dir_all(&cache_dir);
        Self {
            inner,
            cache_dir: cache_dir.into(),
        }
    }

    pub fn entry(&self, k: &K) -> String {
        let mut hasher = DefaultHasher::new();
        (*k).hash(&mut hasher);
        let hashed = hasher.finish();
        let encoded = base64::encode(&format!("{}", hashed));
        encoded
    }

    pub fn path(&self, k: &K) -> PathBuf {
        let mut path = self.cache_dir.to_owned();
        path.push(self.entry(k));
        path
    }
}

#[async_trait]
impl<K> ImageCache<K, FileImage> for FileSystemImageCache<K>
where
    K: Clone + Hash + Eq + Sync + Send,
{
    #[inline]
    async fn put<D: tokio::io::AsyncRead + std::marker::Unpin + Send>(
        &self,
        k: K,
        mut data: D,
        format: image::ImageFormat,
    ) -> Result<Option<FileImage>, Error> {
        let path = self.path(&k);
        let entry = FileImage {
            path: path.clone(),
            format,
        };
        let mut lock = self.inner.write().await;
        match lock.put(k, entry) {
            PutResult::Put | PutResult::Update => {
                crate::debug!("putting image {:?}", &path);
                let mut file = tokio::fs::OpenOptions::new()
                    .read(false)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path)
                    .await?;
                tokio::io::copy(&mut data, &mut file).await?;
                Ok(None)
            }
            _ => Err(Error::NoCapacity),
        }
    }

    #[inline]
    async fn get(&self, k: &K) -> Option<FileImage> {
        let mut lock = self.inner.write().await;
        match lock.get(k).map(|v| v.clone()) {
            Some(cached) => Some(cached),
            None => {
                // check if file exists but is not in the LFU yet
                let path = self.path(&k);
                match tokio::fs::File::open(&path).await {
                    Ok(_) => {
                        let entry = FileImage {
                            path: path.clone(),
                            format: guess_format(&path).unwrap_or(ImageFormat::Jpeg),
                        };
                        lock.put(k.to_owned(), entry.clone());
                        Some(entry)
                    }
                    _ => None,
                }
            }
        }
    }
}
