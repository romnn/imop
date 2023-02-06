#![allow(warnings)]

// pub mod error;
// pub mod image;
pub mod lfu;
// pub mod backend;
// pub mod filesystem;
pub mod memory;
// pub mod deser;
// pub mod newmemory;

// pub use self::error::Error;
// pub use self::image::{CachedImage, ImageCache};
// pub use filesystem::FileSystemImageCache;
// pub use lfu::LFUCache;
// pub use memory::InMemoryImageCache;
pub use memory::Memory;

use async_trait::async_trait;
use std::borrow::Borrow;
use std::hash::Hash;

pub enum PutResult {
    Put,
    Update,
    // Evicted { key: K, value: V },
}

// pub enum PutResult<K, V> {
//     Put,
//     Update,
//     Evicted { key: K, value: V },
// }

#[async_trait]
pub trait Cache<K, V>
where
    K: Clone + Hash + Eq,
{
    // async fn put(&mut self, k: K, v: V) -> PutResult<K, V>;
    async fn put(&mut self, k: K, v: V) -> PutResult;

    async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync;
    // Q: Hash + Eq + Sync;
    // Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync;

    async fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + Sync;

    async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn contains<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;

    async fn purge(&mut self);

    async fn len(&self) -> usize;

    async fn cap(&self) -> Option<usize>;

    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[async_trait]
pub trait Backend<K, V>
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
