use serde_derive::{Deserialize, Serialize};
use std::hash::Hash;

pub struct LFU<K, V>
where
    K: Hash + Eq,
{
    inner: super::memory::LFU<K, V>,
    // // inner: RwLock<RawLRU<K, V>>,
    // values: HashMap<K, ValueCounter<V>>,
    // frequency_bin: HashMap<usize, LinkedHashSet<K>>,
    // capacity: Option<usize>,
    // min_frequency: usize,
}
