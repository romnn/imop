// #[async_trait]
// pub trait Storage<K, V>
// where
//     K: Clone + Hash + Eq,
// {
//     async fn load(&mut self, k: K, v: V) -> PutResult;

//     async fn get<'a, Q>(&'a mut self, k: &'a Q) -> Option<&'a V>
//     where
//         K: Borrow<Q>,
//         Q: ToOwned<Owned = K> + Hash + Eq + Sync;

// }
