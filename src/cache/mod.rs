pub mod error;
pub mod filesystem;
pub mod image;
pub mod lfu;
pub mod memory;

pub use self::error::Error;
pub use self::image::{CachedImage, ImageCache};
pub use filesystem::FileSystemImageCache;
pub use lfu::LFUCache;
pub use memory::InMemoryImageCache;

use std::borrow::Borrow;
use std::hash::Hash;
use std::rc::Rc;

pub enum PutResult<K, V> {
    Put,
    Update,
    Evicted { key: K, value: V },
}

pub trait Cache<K, V>
where
    K: Clone + Hash + Eq,
{
    fn put(&mut self, k: K, v: V) -> PutResult<K, V>;

    fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone;

    fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone;

    fn peek<'a, Q>(&self, k: &'a Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn contains<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn purge(&mut self);

    fn len(&self) -> usize;

    fn cap(&self) -> Option<usize>;

    fn is_empty(&self) -> bool;
}
