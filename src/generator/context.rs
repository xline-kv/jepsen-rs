use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use madsim::{runtime::NodeHandle, time};

use super::GeneratorId;
use crate::{generator::RawGenerator, history::SerializableHistoryList};

/// The global context
#[non_exhaustive]
pub struct Global {
    /// The thread pool
    pub thread_pool: Mutex<BTreeMap<GeneratorId, NodeHandle>>,
    /// The original raw generator
    pub gen: Arc<dyn RawGenerator>,
    /// The start time of the simulation
    pub start_time: time::Instant,
    /// The history list
    pub history: Mutex<SerializableHistoryList>,
}

impl Global {
    /// Create a new global context
    pub fn new(gen: Arc<dyn RawGenerator>) -> Self {
        Self {
            thread_pool: Mutex::new(BTreeMap::new()),
            gen,
            start_time: time::Instant::now(),
            history: Mutex::new(SerializableHistoryList::default()),
        }
    }
    /// Find the minimal usable id in the thread pool
    pub fn get_next_id(&self) -> GeneratorId {
        let pool = self.thread_pool.lock().expect("Failed to lock thread pool");
        for (index, id) in pool.keys().enumerate() {
            if index as u64 != *id {
                return index as u64;
            }
        }
        pool.len() as u64
    }
    /// Allocate a new generator
    pub fn alloc_new_generator(&self, handle: NodeHandle) -> GeneratorId {
        let id = self.get_next_id();
        self.thread_pool
            .lock()
            .expect("Failed to lock thread pool")
            .insert(id, handle);
        id
    }
    /// Free the generator
    pub fn free_generator(&self, id: GeneratorId) {
        self.thread_pool
            .lock()
            .expect("Failed to lock thread pool")
            .remove(&id);
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::generator::elle_rw::ElleRwGenerator;

    #[test]
    fn test_alloc_and_free_generator() {
        let rt = madsim::runtime::Runtime::new();
        let gen = Global::new(Arc::new(ElleRwGenerator::new().unwrap()));
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 0);
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 1);
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 2);
        gen.free_generator(1);
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 1);
    }
}
