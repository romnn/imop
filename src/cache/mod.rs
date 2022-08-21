pub mod error;
pub mod filesystem;
pub mod image;
pub mod lfu;
pub mod memory;

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
    K: std::fmt::Debug + Clone + Hash + Eq,
    V: std::fmt::Debug,
{
    // type Iter: Iterator;

    fn put(&mut self, k: K, v: V) -> PutResult<K, V>;
    // where
    //     K: Clone;

    fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
    where
        // Q: ToOwned<Owned = K>;
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + std::fmt::Debug;
    // Q: Eq + Hash + ?Sized;

    fn get_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        // Q: ToOwned<Owned = K>;
        K: Borrow<Q>,
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + Clone + std::fmt::Debug;
    // Q: Eq + Hash + ?Sized;

    fn peek<'a, Q>(&self, k: &'a Q) -> Option<&'a V>
    where
        // Q: ToOwned<Owned = K>;
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn peek_mut<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a mut V>
    where
        // Q: ToOwned<Owned = K>;
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn contains<Q>(&self, k: &Q) -> bool
    where
        // Q: ToOwned<Owned = K>;
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        // Q: ToOwned<Owned = K>;
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized;

    fn purge(&mut self);

    fn len(&self) -> usize;

    fn cap(&self) -> Option<usize>;

    // fn iter(&self) -> impl Iterator<Item=(K, V)>;
    // fn iter<'a I: Iterator<Item = (Rc<K>, &V)>>(&self) -> I;
    // fn iter<'a, I: Iterator<Item = (K, V)>>(&self) -> I;
    // fn iter(&self) -> Self::Iter;

    fn is_empty(&self) -> bool;
}
