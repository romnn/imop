use async_trait::async_trait;
use caches::Cache as LRUCache;
use lru::LruCache;
use parking_lot::{MappedRwLockWriteGuard, RwLock, RwLockWriteGuard};
use std::borrow::Borrow;
use std::hash::Hash;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

pub trait CachedImage<R>
where
    R: tokio::io::AsyncRead,
{
    fn format(&self) -> Option<image::ImageFormat>;
    fn content_length(&self) -> Option<usize>;
    fn data(&self) -> R;
    // get format
    // get length
}

// #[async_trait]
// pub trait Cache<K, V> {
// pub trait Cache<'a, K, V, R>
// pub trait Cache<K, V, R>
pub trait Cache<K, I, R>
where
    // V: Deref<Target = R>,
    // V: Deref<Target = Option<R>>,
    I: CachedImage<R>,
    R: tokio::io::AsyncRead + tokio::io::AsyncSeek,
{
    // async fn contains<Q>(&self, id: &Q) -> bool
    // fn contains<Q>(&self, id: &Q) -> bool
    // where
    //     Q: AsRef<K> + Hash + Eq + ?Sized;

    // fn get<'a, Q>(&'a mut self, k: &Q) -> Option<&'a V>
    // async fn get<'a>(&'a self, k: &K) -> Option<&'a V>;
    // fn get<'a>(&'a self, k: &K) -> Option<&'a V>;
    // fn get<'a, R>(&'a self, k: &K) -> Option<&'a R>
    // fn get<'a>(&'a self, k: &K) -> Option<R>;
    // fn get(&'a self, k: &K) -> V;
    // fn get(&self, k: &K) -> Option<MappedRwLockWriteGuard<'_, R>>;
    // fn get<'a>(&'a self, k: &'a K) -> Option<&'a R>;
    fn get(&self, k: &K) -> Option<I>;
    fn put<D: tokio::io::AsyncRead>(
        &self,
        k: K,
        data: D,
        format: Option<image::ImageFormat>,
    ) -> Option<I>;
    // fn get(&'a self, k: &K) -> Option<V>;
    // where
    //     R: tokio::io::AsyncRead;

    // impl futures::io::AsyncBufRead>;
    // where
    //     Q: AsRef<K>;
    // K: Hash + Eq + ?Sized;
    // Q: AsRef<K> + Hash + Eq + ?Sized;

    // async fn put(&self, k: K, v: V) -> Option<V>;
    // fn put(&self, k: K, v: V) -> Option<V>;
}

struct CacheFile {
    path: PathBuf,
}

struct EvictionHandler {}

impl caches::OnEvictCallback for EvictionHandler {
    fn on_evict<K, V>(&self, key: &K, _: &V) {
        // TODO: delete file based on key
    }
}

pub struct FileSystemCache<K>
where
    K: Hash + Eq,
{
    inner: caches::RawLRU<K, CacheFile, EvictionHandler>,
    cache_dir: PathBuf,
}

impl<K> FileSystemCache<K>
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

// #[derive(Clone)]
// pub enum CachedImage {
//     Memory{
//         data: Arc<Vec<u8>>
//     };
//     File,
// }

// impl AsRef<[u8]> for CacheData {
//     fn as_ref(&self) -> &[u8] {
//         &self.0
//     }
// }

// #[derive(Clone)]
// pub struct CacheData(Arc<Vec<u8>>);

#[derive(Debug, Clone)]
pub struct InMemoryImage {
    data: Arc<Vec<u8>>,
    format: Option<image::ImageFormat>,
    content_length: Option<usize>,
}

impl AsRef<[u8]> for InMemoryImage {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl CachedImage<std::io::Cursor<InMemoryImage>> for InMemoryImage {
    fn format(&self) -> Option<image::ImageFormat> {
        self.format
    }

    fn content_length(&self) -> Option<usize> {
        self.content_length
    }

    fn data(&self) -> std::io::Cursor<InMemoryImage> {
        std::io::Cursor::new(self.clone())
    }
}

// impl Deref for InMemoryImage {
//     // type Target = dyn tokio::io::AsyncRead; // std::io::Cursor<Vec<u8>>;
//     type Target = std::io::Cursor<InMemoryImage>;
//     // type Target = std::io::Cursor<Self>;

//     fn deref(&self) -> &Self::Target {
//         &std::io::Cursor::new(self.clone())
//         // &std::io::Cursor::new(self)
//     }
// }

// pub trait CachedImage: Deref<Target = dyn tokio::io::AsyncRead> {
//     fn format(&self) -> Option<image::ImageFormat>;
//     fn content_length(&self) -> Option<usize>;
//     // get format
//     // get length
// }

// #[derive(Clone)]
// struct CacheData {
//     data: Arc<Vec<u8>>,
// }

// impl tokio::io::AsyncRead for CacheData {
//     async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//     }
// }

// impl<'a> Deref for CacheData<'a> {
//     type Target = std::io::Cursor<&'a Vec<u8>>;

//     fn deref(&self) -> &Self::Target {
//         std::io::Cursor::new(&self.data)
//         // &self.data
//     }
// }

// pub struct MutexGuardRef<'a, T> {
//     guard: MappedRwLockReadGuard<'a, Option<IMData<'a>>>
//     mutex_guard: MutexGuard<'a, Option<Box<T>>>,
// }

// impl<'a, T> Deref for MutexGuardRef<'a, T> {
//     type Target = Option<Box<T>>;

//     fn deref(&self) -> &Self::Target {
//         &*self.mutex_guard
//     }
// }

// type IMData = std::io::Cursor<&'a Vec<u8>>;
// type IMData = std::io::Cursor<CacheData>;

pub struct InMemoryCache<K>
where
    K: Hash + Eq,
{
    // inner: RwLock<LruCache<K, Arc<Vec<u8>>>>,
    // inner: RwLock<caches::RawLRU<K, CacheFile>>, // ,LruCache<K, Arc<Vec<u8>>>>,
    inner: RwLock<caches::RawLRU<K, InMemoryImage>>, // ,LruCache<K, Arc<Vec<u8>>>>,
}

// impl<K, V> InMemoryCache<K, V>
impl<K> InMemoryCache<K>
where
    K: Hash + Eq,
    // V: futures::io::AsyncRead, // futures::io::AsyncBufRead,
    // V: tokio::io::AsyncRead, // futures::io::AsyncBufRead,
{
    pub fn new(capacity: Option<usize>) -> Self {
        let inner = RwLock::new(caches::RawLRU::new(capacity.unwrap()).unwrap());
        // let inner = RwLock::new(match capacity {
        //     Some(c) => LruCache::new(c),
        //     None => LruCache::unbounded(),
        // });
        Self { inner }
    }
}

// type IMData<'a> = std::io::Cursor<&'a Vec<u8>>;
// type IMData<'a> = std::io::Cursor<&'a Vec<u8>>;
// type IMData = std::io::Cursor<Arc<Vec<u8>>>;
// type IMData<'a> = RwLockWriteGuard<'a, std::io::Cursor<&'a Vec<u8>>>;

// #[async_trait]
// impl<K, V> Cache<K, V> for InMemoryCache<K, V>
// impl<'a, K> Cache<'a, K, MappedRwLockWriteGuard<'a, Option<IMData<'a>>>, IMData<'a>> for InMemoryCache<K>
// impl<'a, K> Cache<'a, K, MappedRwLockWriteGuard<'a, Option<IMData<'a>>>, IMData<'a>> for InMemoryCache<K>
// impl<'b, K> Cache<K, MappedRwLockWriteGuard<'b, Option<IMData<'b>>>, IMData<'b>> for InMemoryCache<K>
// impl<'a, K> Cache<'a, K, MappedRwLockWriteGuard<'_, IMData<'_>>, IMData<'_>> for InMemoryCache<K>
// impl<K> Cache<K, IMData<'_>> for InMemoryCache<K>
// impl<K> Cache<K, std::io::Cursor<CacheData>> for InMemoryCache<K>
impl<K> Cache<K, InMemoryImage, std::io::Cursor<InMemoryImage>> for InMemoryCache<K>
where
    K: Hash + Eq,
{
    // #[inline]
    // // async fn contains<Q>(&self, k: &Q) -> bool
    // fn contains<Q>(&self, k: &Q) -> bool
    // where
    //     Q: AsRef<K> + Hash + Eq + ?Sized,
    // {
    //     // self.inner.read().await.peek(k.as_ref()).is_some()
    //     false
    // }

    #[inline]
    // fn get<'a, Q>(&'a mut self, k: &Q) -> Option<&'a V>
    // impl futures::io::AsyncBufRead>
    // async fn get<'a>(&'a self, k: &K) -> Option<&'a V>
    // fn get<'a>(&'a self, k: &K) -> Option<&'a V>
    // where
    //     Q: AsRef<K>, // K: Hash + Eq + ?Sized,
    // fn get<'a, R>(&'a self, k: &K) -> Option<&R>
    // fn get<'a, R>(&'a self, k: &K) -> Option<&R>
    // fn get<'a>(&'a self, k: &K) -> Option<std::io::Cursor<&'a Vec<u8>>>
    // fn get(&'a self, k: &K) -> MappedRwLockWriteGuard<'a, Option<IMData<'a>>>
    // fn get(&'a self, k: &K) -> MappedRwLockWriteGuard<'_, Option<IMData<'_>>>
    // fn get(&self, k: &K) -> Option<MappedRwLockWriteGuard<'_, IMData>>
    // fn get<'a>(&'a self, k: &'a K) -> Option<&'a InMemoryImage>
    fn put<D: tokio::io::AsyncRead>(
        &self,
        k: K,
        data: D,
        format: Option<image::ImageFormat>,
    ) -> Option<InMemoryImage> {
        None
    }

    fn get(&self, k: &K) -> Option<InMemoryImage>
// std::io::Cursor<CacheData>>
// fn get(&self, k: &K) -> MappedRwLockWriteGuard<'_, Option<std::io::Cursor<&Vec<u8>>>>
// IMData<'a>>
// RwLockWriteGuard<'a, std::io::Cursor<&'a Vec<u8>>>>
// where
    //     R: tokio::io::AsyncRead,
    {
        // let data = std::sync::Arc::new(Vec::<u8>::new());
        // let data = self.inner.write().get(k);
        let mut lock = self.inner.write();
        // this is an arc, so we dont have to worry about returning it
        // let data = lock.get(k);
        // RwLockWriteGuard::try_map(self.inner.write(), |lock| {
        //     // Some(std::io::Cursor::new(Vec::new()))
        //     // Some(12i32)
        //     // &mut lock.get(k).map(|data| std::io::Cursor::new(data))
        //     lock.get(k).map(|data| std::io::Cursor::new(data)).as_mut()
        // })
        // .ok()
        // .flatten()
        // RwLockWriteGuard::map(self.inner.write(), |lock| {
        //     // Some(std::io::Cursor::new(Vec::new()))
        //     // Some(12i32)
        //     &mut lock.get(k).map(|data| std::io::Cursor::new(data))
        // })
        // self.inner.write().map()
        let test = lock.get(k);
        // test.to_owned()
        test.map(|v| v.clone()) // to_owned()
                                // match lock.get(k) {
                                //     Some(data) => Some(std::io::Cursor::new(data.clone())),
                                //     None => None,
                                // }
                                // let data = self.inner.read().await.get(k);
                                // Some(std::io::Cursor::new(data))
                                // None
                                // self.inner.get(k.as_ref())
    }

    // #[inline]
    // // async fn put(&self, k: K, v: V) -> Option<V> {
    // fn put(&self, k: K, v: V) -> Option<V> {
    //     // self.inner.write().await.put(k, v)
    //     None
    // }
}
