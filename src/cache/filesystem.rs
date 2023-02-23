use super::deser;
use super::Cache;
use async_trait::async_trait;
use std::borrow::Borrow;
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::PathBuf;

#[derive(Default, Debug)]
pub struct Filesystem<K, V, DS> {
    // values: HashMap<K, V>,
    key: PhantomData<K>,
    value: PhantomData<V>,
    deser: DS,
    path: PathBuf,
}

impl<K, V, DS> Filesystem<K, V, DS> {
    pub fn new(path: impl Into<PathBuf>, deser: DS) -> Self {
        Self {
            // values: HashMap::new(),
            key: PhantomData,
            value: PhantomData,
            deser,
            path: path.into(),
        }
    }

    pub fn value_path<'a, Q>(&self, key: &'a Q) -> PathBuf
    where
        K: Borrow<Q>,
        Q: Hash + ?Sized,
    {
        self.path.join(Self::key_hash(key)).with_extension("value")
    }

    pub fn key_path<'a, Q>(&self, key: &'a Q) -> PathBuf
    where
        K: Borrow<Q>,
        Q: Hash,
    {
        self.path.join(Self::key_hash(key)).with_extension("key")
    }

    pub fn key_hash<'a, Q>(key: &'a Q) -> String
    where
        K: Borrow<Q>,
        // Q: ToOwned<Owned = K> + Hash + Eq + Sync,
        Q: Hash + ?Sized, //  + Eq + Sync,
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        // hasher.finish()
        // use sha2::Digest;
        // let mut hasher = sha2::Sha256::new();
        // key.hash(&mut hasher);
        // hasher.update(key);
        // std::io::copy(&mut file, &mut hasher)?;
        // format!("{:X}", hasher.finalize())
        format!("{:X}", hasher.finish())
    }
}

#[async_trait]
impl<'de, K, V, DS> super::Backend<K, V> for Filesystem<K, V, DS>
where
    V: serde::Deserialize<'de> + Send + Sync,
    K: serde::Deserialize<'de> + Eq + Hash + Send + Sync,
    DS: deser::Serialize<K>
        + deser::Serialize<V>
        + deser::Deserialize<K>
        + deser::Deserialize<V>
        + Send
        + Sync,
{
    async fn insert<'a>(&'a mut self, k: K, v: V) {
        let value_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.value_path(&k))
            .unwrap();
        self.deser
            .serialize_to(&v, &mut std::io::BufWriter::new(value_file))
            .unwrap();

        let key_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.key_path(&k))
            .unwrap();
        self.deser
            .serialize_to(&k, &mut std::io::BufWriter::new(key_file))
            .unwrap();
    }

    // async fn get<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    async fn get<'a, Q>(&'a self, k: &'a Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        let value_file = match std::fs::OpenOptions::new()
            .read(true)
            .create(false)
            .open(self.value_path(k))
        {
            Ok(file) => file,
            _ => return None,
            // Err(err) => match err.kind() {
            //     std::io::ErrorKind::NotFound => return Ok(None),
            //     _ => return Err(super::Error::Io(err)),
            // },
        };

        let value: V = self
            .deser
            .deserialize_from(&mut std::io::BufReader::new(value_file))
            .unwrap();

        Some(value)
    }

    // async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    // where
    //     K: Borrow<Q>,
    //     Q: Eq + Hash + ?Sized + Sync,
    // {
    //     // self.values.get_mut(k)
    //     None
    // }

    async fn clear(&mut self) {
        // self.values.clear();
    }

    async fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        // self.values.remove(k)
        None
    }

    async fn len(&self) -> usize {
        0
        // self.values.len()
    }

    async fn is_empty(&self) -> bool {
        // self.values.is_empty()
        true
    }
}

// #[derive(thiserror::Error, Debug)]
// pub enum Error<S, D>
// where
//     S: std::error::Error,
//     D: std::error::Error,
// {
//     // #[error("cache entry not found")]
//     // NotFound,

//     // // this should never happen
//     // #[error("no capacity for new entry")]
//     // NoCapacity,

//     // #[error("io error: `{0}`")]
//     #[error(transparent)]
//     Io(#[from] std::io::Error),

//     #[error(transparent)]
//     Serialize(#[from] S),
//     // #[error("invalid image: `{0}`")]
//     // Invalid(#[from] crate::image::Error),
// }

// pub struct LFU<K, V, DS>
// where
//     K: Hash + Eq,
// {
//     inner: super::memory::LFU<K, ()>,
//     deser: DS,
//     path: PathBuf,
//     value: PhantomData<V>,
// }

// impl<K, V, DS> LFU<K, V, DS>
// where
//     K: Hash + Eq,
// {
//     pub fn new(path: impl Into<PathBuf>, deser: DS) -> Self {
//         let inner = super::memory::LFU::with_capacity(None);
//         Self {
//             value: PhantomData,
//             path: path.into(),
//             inner,
//             deser,
//         }
//     }

//     pub fn capacity(self, capacity: impl Into<Option<usize>>) -> Self {
//         let inner = super::memory::LFU::with_capacity(capacity.into());
//         Self { inner, ..self }
//     }

//     // async pub fn value_file_async<'a, Q>(&self, key: &'a Q) -> PathBuf
//     // where
//     //     K: Borrow<Q>,
//     //     Q: Hash,
//     // {
//     //     tokio::fs::OpenOptions::new().read(true)
//     //     // self.path.join(Self::key_hash(key)).with_extension(".value")
//     // }

//     pub fn value_path<'a, Q>(&self, key: &'a Q) -> PathBuf
//     where
//         K: Borrow<Q>,
//         Q: Hash,
//     {
//         self.path.join(Self::key_hash(key)).with_extension(".value")
//     }

//     pub fn key_path<'a, Q>(&self, key: &'a Q) -> PathBuf
//     where
//         K: Borrow<Q>,
//         Q: Hash,
//     {
//         self.path.join(Self::key_hash(key)).with_extension(".key")
//     }

//     pub fn key_hash<'a, Q>(key: &'a Q) -> String
//     where
//         K: Borrow<Q>,
//         // Q: ToOwned<Owned = K> + Hash + Eq + Sync,
//         Q: Hash, //  + Eq + Sync,
//     {
//         use std::collections::hash_map::DefaultHasher;
//         use std::hash::{Hash, Hasher};
//         let mut hasher = DefaultHasher::new();
//         key.hash(&mut hasher);
//         // hasher.finish()
//         // use sha2::Digest;
//         // let mut hasher = sha2::Sha256::new();
//         // key.hash(&mut hasher);
//         // hasher.update(key);
//         // std::io::copy(&mut file, &mut hasher)?;
//         // format!("{:X}", hasher.finalize())
//         format!("{:X}", hasher.finish())
//     }
// }

// #[async_trait]
// pub trait Get<K, V> {
//     async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Result<Option<V>, Error>
//     // <DS::Error>>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Hash + Eq + Sync;
// }

// #[async_trait]
// impl<'de, K, V, DS> LFU<K, V, DS>
// where
//     K: Clone + Hash + Eq + Send + Sync,
//     V: Send + Sync,
//     DS: Deserialize<V> + Send + Sync,
// {
//     async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Result<Option<V>, Error<DS::Error>>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Hash + Eq + Sync,
//     {
//         self.inner.get(k).await;
//         // self.update_freq_bin(k);
//         let value_file = match std::fs::OpenOptions::new()
//             .read(true)
//             .create(false)
//             .open(self.value_path(k))
//         {
//             Ok(file) => file,
//             Err(err) => match err.kind() {
//                 std::io::ErrorKind::NotFound => return Ok(None),
//                 _ => return Err(Error::Io(err)),
//             },
//         };

//         let value: V = self
//             .deser
//             .deserialize_from(&mut std::io::BufReader::new(value_file))
//             .unwrap();

//         Ok(Some(value))
//     }
// }

// #[async_trait]
// impl<'de, K, V, DS> Get<K, V> for LFU<K, V, DS>
// where
//     K: Clone + Hash + Eq + Send + Sync,
//     V: Send + Sync,
//     DS: DeserializeAsync<V> + Send + Sync,
// {
//     async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Result<Option<V>, Error>
//     // <DS::Error>>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Hash + Eq + Sync,
//     {
//         self.inner.get(k).await;
//         // self.update_freq_bin(k);
//         let value_file = match std::fs::OpenOptions::new()
//             .read(true)
//             .create(false)
//             .open(self.value_path(k))
//         {
//             Ok(file) => file,
//             Err(err) => match err.kind() {
//                 std::io::ErrorKind::NotFound => return Ok(None),
//                 _ => return Err(Error::Io(err)),
//             },
//         };

//         Ok(None)
//         // let value: V = self
//         //     .deser
//         //     .deserialize_from(&mut std::io::BufReader::new(value_file))
//         //     .unwrap();

//         // Ok(Some(value))
//     }
// }

// #[async_trait]
// impl<'de, K, V, DS> super::Cache<K, V> for LFU<K, V, DS>
// where
//     K: serde::Serialize + Clone + Hash + Eq + Send + Sync,
//     // for<'de> &'de K: serde::Deserialize<'de>,
//     V: serde::Serialize + serde::Deserialize<'de> + Send + Sync,
//     // for<'de> &'de V: serde::Deserialize<'de>,
//     DS: Serialize<K> + Deserialize<K> + Serialize<V> + Deserialize<V> + Send + Sync,
// {
//     // async fn put(&mut self, k: K, v: V) -> super::PutResult<K, V> {
//     async fn put(&mut self, k: K, v: V) -> super::PutResult {
//         // if let Some(counter) = self.values.get_mut(&k) {
//         //     counter.value = v;
//         //     self.update_freq_bin(&k);
//         //     return super::PutResult::Update;
//         // }
//         // if let Some(capacity) = self.capacity {
//         //     if self.len().await >= capacity {
//         //         self.evict();
//         //     }
//         // }
//         // self.values
//         //     .insert(k.clone(), ValueCounter { value: v, count: 1 });
//         // self.min_frequency = 1;
//         // self.freq_bin
//         //     .entry(self.min_frequency)
//         //     .or_default()
//         //     .insert(k);

//         let value_file = std::fs::OpenOptions::new()
//             .write(true)
//             .create(true)
//             .truncate(true)
//             .open(self.value_path(&k))
//             .unwrap();
//         // let value_writer =
//         self.deser
//             .serialize_to(&v, &mut std::io::BufWriter::new(value_file))
//             .unwrap();

//         let key_file = std::fs::OpenOptions::new()
//             .write(true)
//             .create(true)
//             .truncate(true)
//             .open(self.key_path(&k))
//             .unwrap();
//         // .await;
//         self.deser
//             .serialize_to(&k, &mut std::io::BufWriter::new(key_file))
//             .unwrap();

//         self.inner.put(k, ()).await
//         // match self.inner.put(k, v).await {
//         //     // super::PutResult::Put,
//         //     // super::PutResult::Update,
//         //     super::PutResult::Evicted { key, .. } => {
//         //         super::PutResult::Evicted { key: , .. }
//         //     },
//         //     other => other

//         // }
//         // super::PutResult::Put
//     }

//     async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Hash + Eq + Sync,
//         // Q: Hash + Eq + Sync,
//         // Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
//     {
//         self.inner.get(k).await;
//         // self.update_freq_bin(k);
//         let value_file = std::fs::OpenOptions::new()
//             .read(true)
//             .create(false)
//             .open(self.value_path(k))
//             .unwrap();

//         let value: V = self
//             .deser
//             .deserialize_from(&mut std::io::BufReader::new(value_file))
//             .unwrap();

//         Some(&value)

//         // .await;
//         // self.deser.
//         // self.values.get(k).map(|x| &x.value)
//         // None
//     }

//     async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
//     {
//         // self.update_freq_bin(k);
//         // self.values.get_mut(k).map(|x| &mut x.value)
//         None
//     }

//     async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized + Sync,
//     {
//         // self.values.get(k).map(|x| &x.value)
//         None
//     }

//     async fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized + Sync,
//     {
//         // self.values.get_mut(k).map(|x| &mut x.value)
//         None
//     }

//     async fn contains<Q>(&self, k: &Q) -> bool
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized + Sync,
//     {
//         // self.values.contains_key(k)
//         false
//     }

//     async fn remove<Q>(&mut self, k: &Q) -> Option<V>
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized + Sync,
//     {
//         // match self.values.remove(&k) {
//         //     Some(counter) => {
//         //         self.freq_bin.entry(counter.count).or_default().remove(k);
//         //         Some(counter.value)
//         //     }
//         //     None => None,
//         // }
//         None
//     }

//     async fn purge(&mut self) {
//         // self.values.clear();
//         // self.freq_bin.clear();
//     }

//     async fn len(&self) -> usize {
//         // self.values.len()
//         0
//     }

//     async fn cap(&self) -> Option<usize> {
//         // self.capacity
//         None
//     }

//     async fn is_empty(&self) -> bool {
//         // self.values.is_empty()
//         false
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{deser, Cache, LFU};
    use anyhow::Result;
    use pretty_assertions::assert_eq;

    #[tokio::test(flavor = "multi_thread")]
    async fn get() -> Result<()> {
        // let dir = tempfile::tempdir()?.path();
        let dir = "/Users/roman/dev/imop/tmp";
        let msgpack = deser::MessagePack {};
        let s = Filesystem::new(dir, msgpack);
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        dbg!(&lfu);
        assert_eq!(lfu.get(&10).await, Some(10));
        assert_eq!(lfu.get(&30).await, None);
        Ok(())
    }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn get_mut() {
    //     let mut lfu = LFU::with_capacity(20);
    //     lfu.put(10, 10).await;
    //     lfu.put(20, 30).await;
    //     lfu.get_mut(&10).await.map(|v| *v += 1);
    //     assert_eq!(lfu.get(&10).await, Some(&11));
    //     assert_eq!(lfu.get(&30).await, None);
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn peek() {
    //     let mut lfu = LFU::with_capacity(20);
    //     lfu.put(10, 10).await;
    //     lfu.put(20, 30).await;
    //     assert_eq!(lfu.peek(&10).await, Some(&10));
    //     assert_eq!(lfu.peek(&30).await, None);
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn peek_mut() {
    //     let mut lfu = LFU::with_capacity(20);
    //     lfu.put(10, 10).await;
    //     lfu.put(20, 30).await;
    //     lfu.peek_mut(&10).await.map(|v| *v += 1);
    //     assert_eq!(lfu.peek(&10).await, Some(&11));
    //     assert_eq!(lfu.peek(&30).await, None);
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn eviction() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     lfu.put(3, 30).await;
    //     assert_eq!(lfu.get(&1).await, None);
    //     assert_eq!(lfu.get(&2).await, Some(&20));
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn key_frequency_update_put() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     // cache is at max capacity
    //     // this will evict 2, not 1
    //     lfu.put(1, 30).await;
    //     lfu.put(3, 30).await;
    //     assert_eq!(lfu.get(&2).await, None);
    //     assert_eq!(lfu.get(&1).await, Some(&30));
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn key_frequency_update_get() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     // cache is at max capacity
    //     // increase frequency of 1
    //     lfu.get(&1).await;
    //     // this will evict 2, not 1
    //     lfu.put(3, 30).await;
    //     assert_eq!(lfu.get(&2).await, None);
    //     assert_eq!(lfu.get(&1).await, Some(&10));
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn key_frequency_update_get_mut() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     // cache is at max capacity
    //     // increase frequency of 1
    //     lfu.get_mut(&1).await.map(|v| *v += 1);
    //     // this will evict 2, not 1
    //     lfu.put(3, 30).await;
    //     assert_eq!(lfu.get(&2).await, None);
    //     assert_eq!(lfu.get(&1).await, Some(&11));
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn key_frequency_update_peek() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     // cache is at max capacity
    //     lfu.peek(&1).await;
    //     lfu.peek(&1).await;
    //     assert_eq!(lfu.peek(&1).await, Some(&10));
    //     // this will evict 1, not 2
    //     lfu.put(3, 30).await;
    //     assert_eq!(lfu.get(&1).await, None);
    //     assert_eq!(lfu.get(&2).await, Some(&20));
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn key_frequency_update_peek_mut() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     // cache is at max capacity
    //     lfu.peek_mut(&1).await.map(|v| *v += 1);
    //     lfu.peek_mut(&1).await.map(|v| *v += 1);
    //     assert_eq!(lfu.peek(&1).await, Some(&12));
    //     // this will evict 1, not 2
    //     lfu.put(3, 30).await;
    //     assert_eq!(lfu.get(&1).await, None);
    //     assert_eq!(lfu.get(&2).await, Some(&20));
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn deletion() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(2, 20).await;
    //     assert_eq!(lfu.len().await, 2);
    //     lfu.remove(&1).await;
    //     assert_eq!(lfu.len().await, 1);
    //     assert_eq!(lfu.get(&1).await, None);
    //     lfu.put(3, 30).await;
    //     lfu.put(4, 40).await;
    //     assert_eq!(lfu.get(&2).await, None);
    //     assert_eq!(lfu.get(&3).await, Some(&30));
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn duplicates() {
    //     let mut lfu = LFU::with_capacity(2);
    //     lfu.put(1, 10).await;
    //     lfu.put(1, 20).await;
    //     lfu.put(1, 30).await;
    //     lfu.put(5, 50).await;

    //     assert_eq!(lfu.get(&1).await, Some(&30));
    //     assert_eq!(lfu.len().await, 2);
    // }

    // #[tokio::test(flavor = "multi_thread")]
    // async fn purge() {
    //     let mut lfu = LFU::with_capacity(2);
    //     assert!(lfu.is_empty().await);

    //     lfu.put(1, 10).await;
    //     assert!(!lfu.is_empty().await);
    //     assert_eq!(lfu.len().await, 1);
    //     lfu.put(1, 20).await;
    //     assert!(!lfu.is_empty().await);
    //     assert_eq!(lfu.len().await, 1);
    //     lfu.put(2, 20).await;
    //     assert!(!lfu.is_empty().await);
    //     assert_eq!(lfu.len().await, 2);

    //     // begin to purge
    //     assert_eq!(lfu.get(&1).await, Some(&20));
    //     assert_eq!(lfu.get(&2).await, Some(&20));
    //     lfu.purge().await;
    //     assert!(lfu.is_empty().await);
    //     assert_eq!(lfu.len().await, 0);
    //     assert_eq!(lfu.get(&1).await, None);
    //     assert_eq!(lfu.get(&2).await, None);
    // }
}
