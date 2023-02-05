use async_trait::async_trait;
use caches::RawLRU;
use linked_hash_set::LinkedHashSet;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("cache error: {0}")]
    Cache(caches::lru::CacheError),
}

// #[derive(Debug)]
// struct ValueCounter<V> {
//     value: V,
//     count: usize,
// }

// impl<V> ValueCounter<V> {
//     fn inc(&mut self) {
//         self.count += 1;
//     }
// }

// impl<V> std::ops::Deref for ValueCounter<V> {
//     type Target = V;

//     fn deref(&self) -> &Self::Target {
//         &self.value
//     }
// }

// impl<V> std::ops::DerefMut for ValueCounter<V> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.value
//     }
// }

// todo: cache can be async, but should not lock the entire cache
// file cache, reading while being evicting
// rwlock the file in storage
#[async_trait]
pub trait Storage<K, V>
where
    V: Send + Sync,
{
    async fn insert<'a>(&'a mut self, k: K, value: V);

    async fn get<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn clear(&mut self);

    async fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn len(&self) -> usize;

    async fn is_empty(&self) -> bool;
}

#[derive(Default, Debug)]
pub struct InMemory<K, V> {
    values: HashMap<K, V>,
}

impl<K, V> InMemory<K, V> {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

#[async_trait]
impl<K, V> Storage<K, V> for InMemory<K, V>
where
    V: Send + Sync,
    K: Eq + Hash + Send + Sync,
{
    async fn insert<'a>(&'a mut self, k: K, v: V) {
        self.values.insert(k, v);
    }

    async fn get<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.get(k)
    }

    async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.get_mut(k)
    }

    async fn clear(&mut self) {
        self.values.clear();
    }

    async fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.remove(k)
    }

    async fn len(&self) -> usize {
        self.values.len()
    }

    async fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Debug)]
pub struct LFU<K, V, S>
where
    K: Hash + Eq,
    V: Send + Sync,
    S: Storage<K, V>,
{
    // inner: RwLock<RawLRU<K, V>>,
    // values: HashMap<K, ValueCounter<V>>,
    counts: HashMap<K, usize>,
    storage: S,
    // values: HashMap<K, V>,
    freq_bin: HashMap<usize, LinkedHashSet<K>>,
    capacity: Option<usize>,
    min_frequency: usize,
    phantom: std::marker::PhantomData<V>,
}

impl<K, V, S> LFU<K, V, S>
where
    K: Hash + Eq + Send + Sync,
    V: Send + Sync,
    S: Storage<K, V>,
{
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            counts: HashMap::new(),
            // values: HashMap::new(),
            freq_bin: HashMap::new(),
            capacity: None,
            min_frequency: 0,
            phantom: std::marker::PhantomData,
        }
        // let inner = RawLRU::new(capacity.into()).map_err(Error::Cache)?;
        // Ok(Self {
        //     inner: RwLock::new(inner),
        // })
    }

    // pub fn new() -> Self {
    //     Self::with_capacity(None)
    // }

    pub fn with_capacity(mut self, capacity: impl Into<Option<usize>>) -> Self {
        Self {
            capacity: capacity.into(),
            ..self // values: HashMap::new(),
                   // freq_bin: HashMap::new(),
                   // capacity: capacity.into(),
                   // min_frequency: 0,
        }
        // let inner = RawLRU::new(capacity.into()).map_err(Error::Cache)?;
        // Ok(Self {
        //     inner: RwLock::new(inner),
        // })
    }

    fn update_freq_bin<'a, Q>(&mut self, k: &'a Q)
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync,
        // Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
    {
        // if let Some(value_counter) = self.values.get_mut(k) {
        // if let Some(val) = self.storage.get_mut(k).await {
        if let Some(count) = self.counts.get_mut(k) {
            // let count = self.counts.get_mut(k).unwrap();
            // if let Some(bin) = self.freq_bin.get_mut(&value_counter.count) {
            if let Some(bin) = self.freq_bin.get_mut(&count) {
                bin.remove(k);
                let prev_count = *count;
                // let count = value_counter.count;
                // value_counter.inc();
                *count += 1;
                if *count == self.min_frequency && bin.is_empty() {
                    self.min_frequency += 1;
                }
                self.freq_bin
                    .entry(prev_count + 1)
                    .or_default()
                    .insert(k.to_owned());
            }
        }
    }

    pub async fn evict(&mut self) {
        let least_freq_used_keys = self.freq_bin.get_mut(&self.min_frequency);
        if let Some(least_recently_used) = least_freq_used_keys.and_then(|keys| keys.pop_front()) {
            if let Some(val_counter) = self.storage.remove(&least_recently_used).await {
                let count = self.counts.get(&least_recently_used).unwrap();
                self.freq_bin
                    .get_mut(&count)
                    // .get_mut(&val_counter.count)
                    .map(|bin| bin.remove(&least_recently_used));
            }
        }
    }
}

#[async_trait]
impl<K, V, S> super::Cache<K, V> for LFU<K, V, S>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Send + Sync,
    S: Storage<K, V> + Send + Sync + std::fmt::Debug,
{
    // async fn put(&mut self, k: K, v: V) -> super::PutResult<K, V> {
    #[inline]
    async fn put(&mut self, k: K, v: V) -> super::PutResult {
        if let Some(old) = self.storage.get_mut(&k).await {
            *old = v;
            self.update_freq_bin(&k);
            return super::PutResult::Update;
        }

        // if let Some(counter) = self.values.get_mut(&k) {
        //     counter.value = v;
        //     self.update_freq_bin(&k);
        //     return super::PutResult::Update;
        // }
        if let Some(capacity) = self.capacity {
            if self.len().await >= capacity {
                self.evict().await;
            }
        }
        self.counts.insert(k.clone(), 1);
        self.storage.insert(k.clone(), v).await;
        // .insert(k.clone(), ValueCounter { value: v, count: 1 });
        self.min_frequency = 1;
        self.freq_bin
            .entry(self.min_frequency)
            .or_default()
            .insert(k);
        super::PutResult::Put
    }

    #[inline]
    async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync,
        // Q: Hash + Eq + Sync,
        // Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
    {
        self.update_freq_bin(k);
        self.storage.get(k).await
        // None
        // self.values.get(k).map(|x| &x.value)
    }

    #[inline]
    async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
    {
        self.update_freq_bin(k);
        // self.values.get_mut(k).map(|x| &mut x.value)
        self.storage.get_mut(k).await
        // None
    }

    #[inline]
    async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        // self.values.get(k).map(|x| &x.value)
        self.storage.get(k).await
        // None
    }

    #[inline]
    async fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        // self.values.get_mut(k).map(|x| &mut x.value)
        self.storage.get_mut(k).await
        // None
    }

    #[inline]
    async fn contains<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.counts.contains_key(k)
    }

    #[inline]
    async fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        let count = self.counts.get(&k);
        let value = self.storage.remove(&k).await;
        match (value, count) {
            (Some(value), Some(count)) => {
                self.freq_bin.entry(*count).or_default().remove(k);
                Some(value)
            }
            // Some(counter) => {
            //     self.freq_bin.entry(counter.count).or_default().remove(k);
            //     Some(counter.value)
            // }
            _ => None,
        }
    }

    #[inline]
    async fn purge(&mut self) {
        self.counts.clear();
        self.storage.clear().await;
        self.freq_bin.clear();
    }

    #[inline]
    async fn len(&self) -> usize {
        self.storage.len().await
        // self.counts.len()
        // self.values.len()
    }

    #[inline]
    async fn cap(&self) -> Option<usize> {
        self.capacity
    }

    #[inline]
    async fn is_empty(&self) -> bool {
        self.storage.is_empty().await
        // self.counts.is_empty()
        // self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;

    #[tokio::test(flavor = "multi_thread")]
    async fn get() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        dbg!(&lfu);
        assert_eq!(lfu.get(&10).await, Some(&10));
        assert_eq!(lfu.get(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn get_mut() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        lfu.get_mut(&10).await.map(|v| *v += 1);
        assert_eq!(lfu.get(&10).await, Some(&11));
        assert_eq!(lfu.get(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn peek() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        assert_eq!(lfu.peek(&10).await, Some(&10));
        assert_eq!(lfu.peek(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn peek_mut() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        lfu.peek_mut(&10).await.map(|v| *v += 1);
        assert_eq!(lfu.peek(&10).await, Some(&11));
        assert_eq!(lfu.peek(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn eviction() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, Some(&20));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_put() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        // this will evict 2, not 1
        lfu.put(1, 30).await;
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&2).await, None);
        assert_eq!(lfu.get(&1).await, Some(&30));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_get() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        // increase frequency of 1
        lfu.get(&1).await;
        // this will evict 2, not 1
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&2).await, None);
        assert_eq!(lfu.get(&1).await, Some(&10));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_get_mut() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        // increase frequency of 1
        lfu.get_mut(&1).await.map(|v| *v += 1);
        // this will evict 2, not 1
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&2).await, None);
        assert_eq!(lfu.get(&1).await, Some(&11));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_peek() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        lfu.peek(&1).await;
        lfu.peek(&1).await;
        assert_eq!(lfu.peek(&1).await, Some(&10));
        // this will evict 1, not 2
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, Some(&20));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_peek_mut() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        lfu.peek_mut(&1).await.map(|v| *v += 1);
        lfu.peek_mut(&1).await.map(|v| *v += 1);
        assert_eq!(lfu.peek(&1).await, Some(&12));
        // this will evict 1, not 2
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, Some(&20));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deletion() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        assert_eq!(lfu.len().await, 2);
        lfu.remove(&1).await;
        assert_eq!(lfu.len().await, 1);
        assert_eq!(lfu.get(&1).await, None);
        lfu.put(3, 30).await;
        lfu.put(4, 40).await;
        assert_eq!(lfu.get(&2).await, None);
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn duplicates() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(1, 20).await;
        lfu.put(1, 30).await;
        lfu.put(5, 50).await;

        assert_eq!(lfu.get(&1).await, Some(&30));
        assert_eq!(lfu.len().await, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn purge() {
        let s = InMemory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        assert!(lfu.is_empty().await);

        lfu.put(1, 10).await;
        assert!(!lfu.is_empty().await);
        assert_eq!(lfu.len().await, 1);
        lfu.put(1, 20).await;
        assert!(!lfu.is_empty().await);
        assert_eq!(lfu.len().await, 1);
        lfu.put(2, 20).await;
        assert!(!lfu.is_empty().await);
        assert_eq!(lfu.len().await, 2);

        // begin to purge
        assert_eq!(lfu.get(&1).await, Some(&20));
        assert_eq!(lfu.get(&2).await, Some(&20));
        lfu.purge().await;
        assert!(lfu.is_empty().await);
        assert_eq!(lfu.len().await, 0);
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, None);
    }
}
