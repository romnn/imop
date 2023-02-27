use super::Backend;
use async_trait::async_trait;
use caches::RawLRU;
use linked_hash_set::LinkedHashSet;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

// todo: cache can be async, but should not lock the entire cache
// file cache, reading while being evicting
// rwlock the file in storage
#[derive(Debug)]
pub struct LFU<K, V, S>
where
    K: Hash + Eq,
    V: Send + Sync,
    S: Backend<K, V>,
{
    counts: HashMap<K, usize>,
    backend: S,
    freq_bin: HashMap<usize, LinkedHashSet<K>>,
    capacity: Option<usize>,
    min_frequency: usize,
    phantom: std::marker::PhantomData<V>,
}

impl<K, V, S> LFU<K, V, S>
where
    K: Hash + Eq + Send + Sync,
    V: Send + Sync,
    S: Backend<K, V>,
{
    pub fn new(backend: S) -> Self {
        Self {
            backend,
            counts: HashMap::new(),
            freq_bin: HashMap::new(),
            capacity: None,
            min_frequency: 0,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn with_capacity(mut self, capacity: impl Into<Option<usize>>) -> Self {
        Self {
            capacity: capacity.into(),
            ..self
        }
    }

    fn update_freq_bin<'a, Q>(&mut self, k: &'a Q)
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync,
        // Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
    {
        // if let Some(value_counter) = self.values.get_mut(k) {
        // if let Some(val) = self.backend.get_mut(k).await {
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
            if let Some(val_counter) = self.backend.remove(&least_recently_used).await {
                let count = self.counts.get(&least_recently_used).unwrap();
                self.freq_bin
                    .get_mut(&count)
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
    S: Backend<K, V> + Send + Sync + std::fmt::Debug,
{
    #[inline]
    async fn put(&mut self, k: K, v: V) -> super::PutResult {
        // if let Some(old) = self.backend.get_mut(&k).await {
        if let Some(old) = self.backend.remove(&k).await {
            // *old = v;
            self.backend.insert(k.clone(), v).await;
            self.update_freq_bin(&k);
            return super::PutResult::Update;
        }

        if let Some(capacity) = self.capacity {
            if self.len().await >= capacity {
                self.evict().await;
            }
        }
        self.counts.insert(k.clone(), 1);
        self.backend.insert(k.clone(), v).await;
        self.min_frequency = 1;
        self.freq_bin
            .entry(self.min_frequency)
            .or_default()
            .insert(k);
        super::PutResult::Put
    }

    #[inline]
    // async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync,
    {
        self.update_freq_bin(k);
        self.backend.get(k).await
    }

    // #[inline]
    // async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    // where
    //     K: Borrow<Q>,
    //     Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync,
    // {
    //     self.update_freq_bin(k);
    //     self.backend.get_mut(k).await
    // }

    #[inline]
    // async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.backend.get(k).await
    }

    // #[inline]
    // async fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    // where
    //     K: Borrow<Q>,
    //     Q: Eq + Hash + ?Sized + Sync,
    // {
    //     self.backend.get_mut(k).await
    // }

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
        let value = self.backend.remove(&k).await;
        match (value, count) {
            (Some(value), Some(count)) => {
                self.freq_bin.entry(*count).or_default().remove(k);
                Some(value)
            }
            _ => None,
        }
    }

    #[inline]
    async fn purge(&mut self) {
        self.counts.clear();
        self.backend.clear().await;
        self.freq_bin.clear();
    }

    #[inline]
    async fn len(&self) -> usize {
        self.backend.len().await
    }

    #[inline]
    async fn cap(&self) -> Option<usize> {
        self.capacity
    }

    #[inline]
    async fn is_empty(&self) -> bool {
        self.backend.is_empty().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{Cache, Memory};

    macro_rules! test_lfu_backend {
        ($backend:ident, $factory:expr) => {
            paste::item! {
                #[tokio::test(flavor = "multi_thread")]
                pub async fn [< get _ $backend _ backend  >]() {
                    let lfu = LFU::new($factory);
                    let mut lfu = lfu.with_capacity(20);
                    lfu.put(10, 10).await;
                    lfu.put(20, 30).await;
                    dbg!(&lfu);
                    assert_eq!(lfu.get(&10).await, Some(10));
                    assert_eq!(lfu.get(&30).await, None);
                }
            }
        };
    }

    // fn build_memory_backend() -> Memory {
    // }

    test_lfu_backend!(memory, Memory::new());
    // test_lfu_backend!(fs, Memory::new());
    // let dir = "/Users/roman/dev/imop/tmp";
    // let msgpack = deser::MessagePack {};
    // let s = Filesystem::new(dir, msgpack);

    #[tokio::test(flavor = "multi_thread")]
    async fn get() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        dbg!(&lfu);
        assert_eq!(lfu.get(&10).await, Some(10));
        assert_eq!(lfu.get(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn peek() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(20);
        lfu.put(10, 10).await;
        lfu.put(20, 30).await;
        assert_eq!(lfu.peek(&10).await, Some(10));
        assert_eq!(lfu.peek(&30).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn eviction() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, Some(20));
        assert_eq!(lfu.get(&3).await, Some(30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_put() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        // this will evict 2, not 1
        lfu.put(1, 30).await;
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&2).await, None);
        assert_eq!(lfu.get(&1).await, Some(30));
        assert_eq!(lfu.get(&3).await, Some(30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_get() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        // increase frequency of 1
        lfu.get(&1).await;
        // this will evict 2, not 1
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&2).await, None);
        assert_eq!(lfu.get(&1).await, Some(10));
        assert_eq!(lfu.get(&3).await, Some(30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn key_frequency_update_peek() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(2, 20).await;
        // cache is at max capacity
        lfu.peek(&1).await;
        lfu.peek(&1).await;
        assert_eq!(lfu.peek(&1).await, Some(10));
        // this will evict 1, not 2
        lfu.put(3, 30).await;
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, Some(20));
        assert_eq!(lfu.get(&3).await, Some(30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deletion() {
        let s = Memory::new();
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
        assert_eq!(lfu.get(&3).await, Some(30));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn duplicates() {
        let s = Memory::new();
        let mut lfu = LFU::new(s).with_capacity(2);
        lfu.put(1, 10).await;
        lfu.put(1, 20).await;
        lfu.put(1, 30).await;
        lfu.put(5, 50).await;

        assert_eq!(lfu.get(&1).await, Some(30));
        assert_eq!(lfu.len().await, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn purge() {
        let s = Memory::new();
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
        assert_eq!(lfu.get(&1).await, Some(20));
        assert_eq!(lfu.get(&2).await, Some(20));
        lfu.purge().await;
        assert!(lfu.is_empty().await);
        assert_eq!(lfu.len().await, 0);
        assert_eq!(lfu.get(&1).await, None);
        assert_eq!(lfu.get(&2).await, None);
    }
}

// use super::{Cache, PutResult};
// use linked_hash_set::LinkedHashSet;
// use std::borrow::Borrow;
// use std::collections::hash_map::IntoIter;
// use std::collections::HashMap;
// use std::fmt::Debug;
// use std::hash::Hash;
// use std::ops::Index;
// use std::rc::Rc;

// #[derive(Debug)]
// pub struct LFUCache<K, V>
// where
//     K: Clone + Hash + Eq,
// {
//     values: HashMap<K, ValueCounter<V>>,
//     frequency_bin: HashMap<usize, LinkedHashSet<K>>,
//     capacity: Option<usize>,
//     min_frequency: usize,
// }

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

// // what do we need
// // eviction callback handler
// // evictor (normally use default evictor)
// // a builder
// // a state updater (that can be a struct with state)
// // state changes: update, insert, evict

// impl<K, V> LFUCache<K, V>
// where
//     K: Clone + Hash + Eq,
// {
//     pub fn with_capacity(capacity: usize) -> LFUCache<K, V> {
//         LFUCache {
//             values: HashMap::new(),
//             frequency_bin: HashMap::new(),
//             capacity: Some(capacity),
//             min_frequency: 0,
//         }
//     }

//     fn update_frequency_bin<'a, Q>(&mut self, k: &'a Q)
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
//     {
//         if let Some(value_counter) = self.values.get_mut(k) {
//             if let Some(bin) = self.frequency_bin.get_mut(&value_counter.count) {
//                 bin.remove(k);
//                 let count = value_counter.count;
//                 value_counter.inc();
//                 if count == self.min_frequency && bin.is_empty() {
//                     self.min_frequency += 1;
//                 }
//                 self.frequency_bin
//                     .entry(count + 1)
//                     .or_default()
//                     .insert(k.to_owned());
//             }
//         }
//     }

//     fn evict(&mut self) {
//         let least_frequently_used_keys = self.frequency_bin.get_mut(&self.min_frequency);
//         if let Some(least_recently_used) =
//             least_frequently_used_keys.and_then(|keys| keys.pop_front())
//         {
//             if let Some(value_counter) = self.values.remove(&least_recently_used) {
//                 let bin = self.frequency_bin.get_mut(&value_counter.count).unwrap();
//                 bin.remove(&least_recently_used);
//             }
//         }
//     }
// }

// impl<K, V> Cache<K, V> for LFUCache<K, V>
// where
//     K: Clone + Hash + Eq,
// {
//     fn put(&mut self, k: K, v: V) -> PutResult<K, V> {
//         if let Some(counter) = self.values.get_mut(&k) {
//             counter.value = v;
//             self.update_frequency_bin(&k);
//             return PutResult::Update;
//         }
//         if let Some(capacity) = self.capacity {
//             if self.len() >= capacity {
//                 self.evict();
//             }
//         }
//         self.values
//             .insert(k.clone(), ValueCounter { value: v, count: 1 });
//         self.min_frequency = 1;
//         self.frequency_bin
//             .entry(self.min_frequency)
//             .or_default()
//             .insert(k);
//         PutResult::Put
//     }

//     fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone,
//     {
//         self.update_frequency_bin(k);
//         self.values.get(k).map(|x| &x.value)
//     }

//     fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone,
//     {
//         self.update_frequency_bin(k);
//         self.values.get_mut(k).map(|x| &mut x.value)
//     }

//     fn peek<'a, Q>(&self, k: &'a Q) -> Option<&'a V>
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized,
//     {
//         None
//     }

//     fn peek_mut<'a, Q>(&mut self, k: &'a Q) -> Option<&'a mut V>
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized,
//     {
//         None
//     }

//     fn contains<Q>(&self, k: &Q) -> bool
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized,
//     {
//         self.values.contains_key(k)
//     }

//     fn remove<Q>(&mut self, k: &Q) -> Option<V>
//     where
//         K: Borrow<Q>,
//         Q: Eq + Hash + ?Sized,
//     {
//         match self.values.remove(&k) {
//             Some(counter) => {
//                 self.frequency_bin
//                     .entry(counter.count)
//                     .or_default()
//                     .remove(k);
//                 Some(counter.value)
//             }
//             None => None,
//         }
//     }

//     fn purge(&mut self) {
//         self.values.clear();
//         self.frequency_bin.clear();
//     }

//     fn len(&self) -> usize {
//         self.values.len()
//     }

//     fn cap(&self) -> Option<usize> {
//         self.capacity
//     }

//     fn is_empty(&self) -> bool {
//         self.values.is_empty()
//     }
// }

// pub struct LfuIterator<K, V> {
//     values: IntoIter<K, ValueCounter<V>>,
// }

// impl<K, V> Iterator for LfuIterator<K, V> {
//     type Item = (K, V);

//     fn next(&mut self) -> Option<Self::Item> {
//         self.values.next().map(|(k, v)| (k, v.value))
//     }
// }

// impl<K, V> IntoIterator for LFUCache<K, V>
// where
//     K: Clone + Eq + Hash,
// {
//     type Item = (K, V);
//     type IntoIter = LfuIterator<K, V>;

//     fn into_iter(self) -> Self::IntoIter {
//         return LfuIterator {
//             values: self.values.into_iter(),
//         };
//     }
// }

// impl<K, V> Index<K> for LFUCache<K, V>
// where
//     K: Clone + Hash + Eq,
// {
//     type Output = V;
//     fn index(&self, index: K) -> &Self::Output {
//         return self.values.get(&Rc::new(index)).map(|x| &x.value).unwrap();
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let mut lfu = LFUCache::with_capacity(20);
//         lfu.put(10, 10);
//         lfu.put(20, 30);
//         assert_eq!(lfu.get(&10).unwrap(), &10);
//         assert_eq!(lfu.get(&30), None);
//     }

//     #[test]
//     fn test_lru_eviction() {
//         let mut lfu = LFUCache::with_capacity(2);
//         lfu.put(1, 1);
//         lfu.put(2, 2);
//         lfu.put(3, 3);
//         assert_eq!(lfu.get(&1), None)
//     }

//     #[test]
//     fn test_key_frequency_update() {
//         let mut lfu = LFUCache::with_capacity(2);
//         lfu.put(1, 1);
//         lfu.put(2, 2);
//         lfu.put(1, 3);
//         lfu.put(10, 10);
//         assert_eq!(lfu.get(&2), None);
//         assert_eq!(lfu[10], 10);
//     }

//     #[test]
//     fn test_lfu_indexing() {
//         let mut lfu: LFUCache<i32, i32> = LFUCache::with_capacity(2);
//         lfu.put(1, 1);
//         assert_eq!(lfu[1], 1);
//     }

//     #[test]
//     fn test_lfu_deletion() {
//         let mut lfu = LFUCache::with_capacity(2);
//         lfu.put(1, 1);
//         lfu.put(2, 2);
//         lfu.remove(&1);
//         assert_eq!(lfu.get(&1), None);
//         lfu.put(3, 3);
//         lfu.put(4, 4);
//         assert_eq!(lfu.get(&2), None);
//         assert_eq!(lfu.get(&3), Some(&3));
//     }

//     #[test]
//     fn test_duplicates() {
//         let mut lfu = LFUCache::with_capacity(2);
//         lfu.put(1, 1);
//         lfu.put(1, 2);
//         lfu.put(1, 3);
//         {
//             lfu.put(5, 20);
//         }

//         assert_eq!(lfu[1], 3);
//         assert_eq!(lfu.len(), 2);
//     }

//     #[test]
//     fn test_lfu_consumption() {
//         let mut lfu = LFUCache::with_capacity(1);
//         lfu.put(&1, 1);
//         for (_, v) in lfu {
//             assert_eq!(v, 1);
//         }
//     }

//     #[test]
//     fn test_lfu_iter() {
//         let mut lfu = LFUCache::with_capacity(2);
//         lfu.put(&1, 1);
//         lfu.put(&2, 2);
//         for (key, v) in lfu.into_iter() {
//             match key {
//                 1 => {
//                     assert_eq!(v, 1);
//                 }
//                 2 => {
//                     assert_eq!(v, 2);
//                 }
//                 _ => {}
//             }
//         }
//     }
// }
