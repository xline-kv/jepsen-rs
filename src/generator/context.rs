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
    pub gen: Arc<dyn RawGenerator<Item = u64>>,
    /// The start time of the simulation
    pub start_time: time::Instant,
    /// The history list
    pub history: Mutex<SerializableHistoryList>,
}

impl Global {
    /// Create a new global context
    pub fn new(gen: Arc<dyn RawGenerator<Item = u64>>) -> Self {
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
mod tests {}
