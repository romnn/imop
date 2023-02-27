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

#[derive(Default, Debug)]
pub struct Memory<K, V> {
    values: HashMap<K, V>,
}

impl<K, V> Memory<K, V> {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

#[async_trait]
impl<K, V> super::Backend<K, V> for Memory<K, V>
where
    V: Send + Sync + Clone,
    K: Eq + Hash + Send + Sync,
{
    async fn insert<'a>(&'a mut self, k: K, v: V) {
        self.values.insert(k, v);
    }

    async fn get<'a, Q>(&'a self, k: &'a Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync,
    {
        self.values.get(k).cloned()
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
