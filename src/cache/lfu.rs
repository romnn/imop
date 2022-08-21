use super::{Cache, PutResult};
use linked_hash_set::LinkedHashSet;
use std::borrow::Borrow;
use std::collections::hash_map::IntoIter;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Index;
use std::rc::Rc;

#[derive(Debug)]
pub struct LFUCache<K, V>
where
    K: Hash + Eq,
{
    values: HashMap<K, ValueCounter<V>>,
    frequency_bin: HashMap<usize, LinkedHashSet<K>>,
    capacity: Option<usize>,
    min_frequency: usize,
}

#[derive(Debug)]
struct ValueCounter<V> {
    value: V,
    count: usize,
}

impl<V> ValueCounter<V> {
    fn inc(&mut self) {
        self.count += 1;
    }
}

// what do we need
// eviction callback handler
// evictor (normally use default evictor)
// a builder
// a state updater (that can be a struct with state)
// state changes: update, insert, evict

impl<K, V> LFUCache<K, V>
where
    K: Hash + Eq,
{
    pub fn with_capacity(capacity: usize) -> LFUCache<K, V> {
        LFUCache {
            values: HashMap::new(),
            frequency_bin: HashMap::new(),
            capacity: Some(capacity),
            min_frequency: 0,
        }
    }

    fn update_frequency_bin<'a, Q>(&mut self, k: &'a Q)
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
    {
        if let Some(value_counter) = self.values.get_mut(k) {
            if let Some(bin) = self.frequency_bin.get_mut(&value_counter.count) {
                bin.remove(k);
                let count = value_counter.count;
                value_counter.inc();
                if count == self.min_frequency && bin.is_empty() {
                    self.min_frequency += 1;
                }
                self.frequency_bin
                    .entry(count + 1)
                    .or_default()
                    .insert(k.to_owned());
            }
        }
    }

    fn evict(&mut self) {
        let least_frequently_used_keys = self.frequency_bin.get_mut(&self.min_frequency);
        if let Some(least_recently_used) =
            least_frequently_used_keys.and_then(|keys| keys.pop_front())
        {
            if let Some(value_counter) = self.values.remove(&least_recently_used) {
                let bin = self.frequency_bin.get_mut(&value_counter.count).unwrap();
                bin.remove(&least_recently_used);
            }
        }
    }
}

impl<K, V> Cache<K, V> for LFUCache<K, V>
where
    K: Clone + Hash + Eq,
{
    fn put(&mut self, k: K, v: V) -> PutResult<K, V> {
        if let Some(counter) = self.values.get_mut(&k) {
            counter.value = v;
            self.update_frequency_bin(&k);
            return PutResult::Update;
        }
        if let Some(capacity) = self.capacity {
            if self.len() >= capacity {
                self.evict();
            }
        }
        self.values
            .insert(k.clone(), ValueCounter { value: v, count: 1 });
        self.min_frequency = 1;
        self.frequency_bin
            .entry(self.min_frequency)
            .or_default()
            .insert(k);
        PutResult::Put
    }

    fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone,
    {
        self.update_frequency_bin(k);
        self.values.get(k).map(|x| &x.value)
    }

    fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone,
    {
        self.update_frequency_bin(k);
        self.values.get_mut(k).map(|x| &mut x.value)
    }

    fn peek<'a, Q>(&self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        None
    }

    fn peek_mut<'a, Q>(&mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        None
    }

    fn contains<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.values.contains_key(k)
    }

    fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        match self.values.remove(&k) {
            Some(counter) => {
                self.frequency_bin
                    .entry(counter.count)
                    .or_default()
                    .remove(k);
                Some(counter.value)
            }
            None => None,
        }
    }

    fn purge(&mut self) {
        self.values.clear();
        self.frequency_bin.clear();
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn cap(&self) -> Option<usize> {
        self.capacity
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

pub struct LfuIterator<K, V> {
    values: IntoIter<K, ValueCounter<V>>,
}

impl<K, V> Iterator for LfuIterator<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.values.next().map(|(k, v)| (k, v.value))
    }
}

impl<K, V> IntoIterator for LFUCache<K, V>
where
    K: Eq + Hash,
{
    type Item = (K, V);
    type IntoIter = LfuIterator<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        return LfuIterator {
            values: self.values.into_iter(),
        };
    }
}

impl<K: Hash + Eq, V> Index<K> for LFUCache<K, V> {
    type Output = V;
    fn index(&self, index: K) -> &Self::Output {
        return self.values.get(&Rc::new(index)).map(|x| &x.value).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut lfu = LFUCache::with_capacity(20);
        lfu.put(10, 10);
        lfu.put(20, 30);
        assert_eq!(lfu.get(&10).unwrap(), &10);
        assert_eq!(lfu.get(&30), None);
    }

    #[test]
    fn test_lru_eviction() {
        let mut lfu = LFUCache::with_capacity(2);
        lfu.put(1, 1);
        lfu.put(2, 2);
        lfu.put(3, 3);
        assert_eq!(lfu.get(&1), None)
    }

    #[test]
    fn test_key_frequency_update() {
        let mut lfu = LFUCache::with_capacity(2);
        lfu.put(1, 1);
        lfu.put(2, 2);
        lfu.put(1, 3);
        lfu.put(10, 10);
        assert_eq!(lfu.get(&2), None);
        assert_eq!(lfu[10], 10);
    }

    #[test]
    fn test_lfu_indexing() {
        let mut lfu: LFUCache<i32, i32> = LFUCache::with_capacity(2);
        lfu.put(1, 1);
        assert_eq!(lfu[1], 1);
    }

    #[test]
    fn test_lfu_deletion() {
        let mut lfu = LFUCache::with_capacity(2);
        lfu.put(1, 1);
        lfu.put(2, 2);
        lfu.remove(&1);
        assert_eq!(lfu.get(&1), None);
        lfu.put(3, 3);
        lfu.put(4, 4);
        assert_eq!(lfu.get(&2), None);
        assert_eq!(lfu.get(&3), Some(&3));
    }

    #[test]
    fn test_duplicates() {
        let mut lfu = LFUCache::with_capacity(2);
        lfu.put(1, 1);
        lfu.put(1, 2);
        lfu.put(1, 3);
        {
            lfu.put(5, 20);
        }

        assert_eq!(lfu[1], 3);
        assert_eq!(lfu.len(), 2);
    }

    #[test]
    fn test_lfu_consumption() {
        let mut lfu = LFUCache::with_capacity(1);
        lfu.put(&1, 1);
        for (_, v) in lfu {
            assert_eq!(v, 1);
        }
    }

    #[test]
    fn test_lfu_iter() {
        let mut lfu = LFUCache::with_capacity(2);
        lfu.put(&1, 1);
        lfu.put(&2, 2);
        for (key, v) in lfu.into_iter() {
            match key {
                1 => {
                    assert_eq!(v, 1);
                }
                2 => {
                    assert_eq!(v, 2);
                }
                _ => {}
            }
        }
    }
}
