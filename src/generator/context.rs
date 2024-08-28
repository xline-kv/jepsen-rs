use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use madsim::{runtime::NodeHandle, time};

use super::RawGenerator;
use crate::{client::Client, history::SerializableHistoryList, op::Op};

/// The id of the generator. Each [`GeneratorId`] corresponds to one thread.
#[derive(Clone)]
pub struct GeneratorId<C: Client> {
    id: u64,
    handle_pool: Arc<Mutex<BTreeMap<u64, NodeHandle>>>,
    client: Arc<C>,
}

impl<C: Client> GeneratorId<C> {
    /// Create a new generator id
    pub async fn new(id_set: Arc<Mutex<BTreeMap<u64, NodeHandle>>>, client: Arc<C>) -> Self {
        Self {
            id: Self::alloc_id(&id_set, &client).await,
            handle_pool: id_set,
            client,
        }
    }

    /// Get the id
    pub fn get(&self) -> u64 {
        self.id
    }

    /// Find the minimal usable id in the thread pool
    fn get_next_id(id_set: &Arc<Mutex<BTreeMap<u64, NodeHandle>>>) -> u64 {
        let pool = id_set.lock().expect("Failed to lock thread pool");
        for (index, id) in pool.keys().enumerate() {
            if index as u64 != *id {
                return index as u64;
            }
        }
        pool.len() as u64
    }

    /// Allocate a new generator id, get a new [`NodeHandle`] from client and
    /// assoc with this id.
    async fn alloc_id(id_set: &Arc<Mutex<BTreeMap<u64, NodeHandle>>>, client: &Arc<C>) -> u64 {
        let id = Self::get_next_id(id_set);
        let res = id_set
            .lock()
            .expect("Failed to lock thread pool")
            .insert(id, client.new_handle().await);
        debug_assert!(res.is_none(), "insert must be success");
        id
    }
    /// Free the generator id and it's handle.
    fn free_id(&self, id: u64) -> bool {
        self.handle_pool
            .lock()
            .expect("Failed to lock thread pool")
            .remove(&id)
            .is_some()
    }
}

impl<C: Client> Drop for GeneratorId<C> {
    fn drop(&mut self) {
        let res = self.free_id(self.id);
        debug_assert!(res, "free must be success");
    }
}

/// The global context
#[non_exhaustive]
pub struct Global<'a, C: Client, T: Send = Result<Op>> {
    /// The id allocator and handle pool
    pub handle_pool: Arc<Mutex<BTreeMap<u64, NodeHandle>>>,
    /// The original raw generator
    pub gen: Mutex<Option<Box<dyn RawGenerator<Item = T> + Send + 'a>>>,
    /// The start time of the simulation
    pub start_time: time::Instant,
    /// The history list
    pub history: Mutex<SerializableHistoryList>,
    /// The client
    client: Arc<C>,
}

impl<'a, C: Client, T: Send + 'a> Global<'a, C, T> {
    /// Create a new global context
    pub fn new(gen: impl RawGenerator<Item = T> + Send + 'a, client: Arc<C>) -> Self {
        Self {
            handle_pool: Mutex::new(BTreeMap::new()).into(),
            gen: Mutex::new(Some(
                Box::new(gen) as Box<dyn RawGenerator<Item = T> + Send + 'a>
            )),
            start_time: time::Instant::now(),
            history: Mutex::new(SerializableHistoryList::default()),
            client,
        }
    }

    /// Alloc a new generator id
    pub async fn get_id(&self) -> GeneratorId<C> {
        GeneratorId::new(Arc::clone(&self.handle_pool), Arc::clone(&self.client)).await
    }

    /// Take the next `n` ops from the raw generator.
    pub fn take_seq(&self, n: usize) -> Box<dyn Iterator<Item = T> + Send + 'a> {
        if let Some(gen) = self.gen.lock().expect("Failed to lock gen").as_mut() {
            Box::new(gen.gen_n(n).into_iter()) as Box<dyn Iterator<Item = T> + Send + 'a>
        } else {
            Box::new(std::iter::empty()) as Box<dyn Iterator<Item = T> + Send>
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::TestClient;

    #[madsim::test]
    async fn test_alloc_and_free_id() {
        let id_set = Arc::new(Mutex::new(BTreeMap::new()));
        let client = Arc::new(TestClient {});
        let id0 = GeneratorId::new(id_set.clone(), client.clone()).await;
        assert_eq!(id0.get(), 0);
        let id1 = GeneratorId::new(id_set.clone(), client.clone()).await;
        assert_eq!(id1.get(), 1);
        let id2 = GeneratorId::new(id_set.clone(), client.clone()).await;
        assert_eq!(id2.get(), 2);
        drop(id1);
        assert!(id_set.lock().unwrap().keys().cloned().collect::<Vec<u64>>() == vec![0, 2]);
        let id1 = GeneratorId::new(id_set.clone(), client.clone()).await;
        assert_eq!(id1.get(), 1);
    }
}
