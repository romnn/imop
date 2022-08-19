use lru::LruCache;
use std::hash::Hash;

// pub trait Cache<K: Hash + Eq, V> {
pub trait Cache<K, V> {
    fn contains(&self, id: &K) -> bool;
}

pub struct InMemoryCache<K, V> {
    inner: LruCache<K, V>,
}

impl<K, V> InMemoryCache<K, V>
where
    K: Hash + Eq,
{
    pub fn new(capacity: Option<usize>) -> Self {
        let inner = match capacity {
            Some(c) => LruCache::new(c),
            None => LruCache::unbounded(),
        };
        Self { inner }
    }
}

impl<K, V> Cache<K, V> for InMemoryCache<K, V>
where
    K: Hash + Eq,
{
    fn contains(&self, id: &K) -> bool {
        self.inner.peek(id).is_some()
    }
}
