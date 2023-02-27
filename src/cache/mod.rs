#![allow(warnings)]

// pub mod error;
// pub mod image;
pub mod lfu;
// pub mod backend;
pub mod deser;
pub mod filesystem;
pub mod memory;
// pub mod newmemory;

pub use filesystem::Filesystem;
pub use lfu::LFU;
pub use memory::Memory;

use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::borrow::Borrow;
use std::hash::Hash;
use std::pin::Pin;

pub enum PutResult {
    Put,
    Update,
}

#[async_trait]
pub trait StreamingCache<K>
where
    K: Clone + Hash + Eq,
{
    async fn put(
        &mut self,
        k: K,
        v: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    ) -> PutResult;

    async fn get<'a, Q>(
        &'a mut self,
        k: &'a Q,
    ) -> Option<Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync;
}

#[async_trait]
pub trait Cache<K, V>
where
    K: Clone + Hash + Eq,
{
    async fn put(&mut self, k: K, v: V) -> PutResult;

    async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Hash + Eq + Sync;

    async fn peek<'a, Q>(&'a self, k: &'a Q) -> Option<V>
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
    async fn put<'a>(&'a mut self, k: K, v: V);

    async fn get<'a, Q>(&'a self, k: &'a Q) -> Option<V>
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

#[async_trait]
pub trait StreamingBackend<K> {
    async fn put<'a>(
        &'a mut self,
        k: K,
        v: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    );

    async fn get<'a, Q>(
        &'a self,
        k: &'a Q,
    ) -> Option<Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized + Sync;
}
