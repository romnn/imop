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

// pub trait Accessor<K, V> {
//     async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
// }

pub struct LFU<K, V>
where
    K: Hash + Eq,
{
    // inner: RwLock<RawLRU<K, V>>,
    // values: HashMap<K, ValueCounter<V>>,
    counts: HashMap<K, usize>,
    values: HashMap<K, V>,
    freq_bin: HashMap<usize, LinkedHashSet<K>>,
    capacity: Option<usize>,
    min_frequency: usize,
}

impl<K, V> LFU<K, V>
where
    K: Hash + Eq,
{
    pub fn new() -> Self {
        Self::with_capacity(None)
    }

    pub fn with_capacity(capacity: impl Into<Option<usize>>) -> Self {
        Self {
            values: HashMap::new(),
            freq_bin: HashMap::new(),
            capacity: capacity.into(),
            min_frequency: 0,
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
        if let Some(value_counter) = self.values.get_mut(k) {
            if let Some(bin) = self.freq_bin.get_mut(&value_counter.count) {
                bin.remove(k);
                let count = value_counter.count;
                value_counter.inc();
                if count == self.min_frequency && bin.is_empty() {
                    self.min_frequency += 1;
                }
                self.freq_bin
                    .entry(count + 1)
                    .or_default()
                    .insert(k.to_owned());
            }
        }
    }

    pub fn evict(&mut self) {
        let least_freq_used_keys = self.freq_bin.get_mut(&self.min_frequency);
        if let Some(least_recently_used) = least_freq_used_keys.and_then(|keys| keys.pop_front()) {
            if let Some(val_counter) = self.values.remove(&least_recently_used) {
                self.freq_bin
                    .get_mut(&val_counter.count)
                    .map(|bin| bin.remove(&least_recently_used));
            }
        }
    }
}

#[async_trait]
impl<K, V> super::Cache<K, V> for LFU<K, V>
where
    K: Clone + Hash + Eq + Send + Sync,
    V: Send + Sync,
{
    // async fn put(&mut self, k: K, v: V) -> super::PutResult<K, V> {
    async fn put(&mut self, k: K, v: V) -> super::PutResult {
        if let Some(counter) = self.values.get_mut(&k) {
            counter.value = v;
            self.update_freq_bin(&k);
            return super::PutResult::Update;
        }
        if let Some(capacity) = self.capacity {
            if self.len().await >= capacity {
                self.evict();
            }
        }
        self.values
            .insert(k.clone(), ValueCounter { value: v, count: 1 });
        self.min_frequency = 1;
        self.freq_bin
            .entry(self.min_frequency)
            .or_default()
            .insert(k);
        super::PutResult::Put
    }

    async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync,
        // Q: Hash + Eq + Sync,
        // Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
    {
        self.update_freq_bin(k);
        self.values.get(k).map(|x| &x.value)
    }

    async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
    {
        self.update_freq_bin(k);
        self.values.get_mut(k).map(|x| &mut x.value)
    }

    async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.get(k).map(|x| &x.value)
    }

    async fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.get_mut(k).map(|x| &mut x.value)
    }

    async fn contains<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.contains_key(k)
    }

    async fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        match self.values.remove(&k) {
            Some(counter) => {
                self.freq_bin.entry(counter.count).or_default().remove(k);
                Some(counter.value)
            }
            None => None,
        }
    }

    async fn purge(&mut self) {
        self.values.clear();
        self.freq_bin.clear();
    }

    async fn len(&self) -> usize {
        self.values.len()
    }

    async fn cap(&self) -> Option<usize> {
        self.capacity
    }

    async fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;

    #[tokio::test(flavor = "multi_thread")]
    async fn get() {
        let mut lfu = LFU::with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        assert_eq!(lfu.get(&10).await, Some(&10));
        assert_eq!(lfu.get(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn get_mut() {
        let mut lfu = LFU::with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        lfu.get_mut(&10).await.map(|v| *v += 1);
        assert_eq!(lfu.get(&10).await, Some(&11));
        assert_eq!(lfu.get(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn peek() {
        let mut lfu = LFU::with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        assert_eq!(lfu.peek(&10).await, Some(&10));
        assert_eq!(lfu.peek(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn peek_mut() {
        let mut lfu = LFU::with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        lfu.peek_mut(&10).await.map(|v| *v += 1);
        assert_eq!(lfu.peek(&10).await, Some(&11));
        assert_eq!(lfu.peek(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn eviction() {
        let mut lfu = LFU::with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, Some(&20));
        assert_eq!(lfu.get(&3).await, Some(&30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_put() {
        let mut lfu = LFU::with_capacity(2);
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
        let mut lfu = LFU::with_capacity(2);
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
        let mut lfu = LFU::with_capacity(2);
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
        let mut lfu = LFU::with_capacity(2);
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
        let mut lfu = LFU::with_capacity(2);
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
        let mut lfu = LFU::with_capacity(2);
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
        let mut lfu = LFU::with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(1, 20).await;
        lfu.put(1, 30).await;
        lfu.put(5, 50).await;

        assert_eq!(lfu.get(&1).await, Some(&30));
        assert_eq!(lfu.len().await, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn purge() {
        let mut lfu = LFU::with_capacity(2);
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
