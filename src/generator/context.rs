use std::{collections::BTreeMap, sync::Mutex};

use anyhow::Result;
use madsim::{runtime::NodeHandle, time};

use super::GeneratorId;
use crate::{history::SerializableHistoryList, op::Op, utils::IteratorExt};

/// The global context
#[non_exhaustive]
pub struct Global<'a, T = Result<Op>> {
    /// The thread pool
    pub thread_pool: Mutex<BTreeMap<GeneratorId, NodeHandle>>,
    /// The original raw generator
    pub gen: Mutex<Option<Box<dyn Iterator<Item = T> + Send + 'a>>>,
    /// The start time of the simulation
    pub start_time: time::Instant,
    /// The history list
    pub history: Mutex<SerializableHistoryList>,
}

impl<'a, T: 'a> Global<'a, T> {
    /// Create a new global context
    pub fn new(gen: impl Iterator<Item = T> + Send + 'a) -> Self {
        Self {
            thread_pool: Mutex::new(BTreeMap::new()),
            gen: Mutex::new(Some(
                Box::new(gen) as Box<dyn Iterator<Item = T> + Send + 'a>
            )),
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

    /// Take the next `n` ops from the raw generator.
    pub fn take_seq(&self, n: usize) -> Box<dyn Iterator<Item = T> + 'a> {
        if let Some(gen) = self.gen.lock().expect("Failed to lock gen").as_mut() {
            Box::new(gen.split_at(n)) as Box<dyn Iterator<Item = T> + 'a>
        } else {
            Box::new(std::iter::empty()) as Box<dyn Iterator<Item = T>>
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::generator::elle_rw::ElleRwGenerator;

    #[test]
    fn test_alloc_and_free_generator() {
        let rt = madsim::runtime::Runtime::new();
        let gen = Global::new(Box::new(ElleRwGenerator::new().unwrap()));
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 0);
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 1);
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 2);
        gen.free_generator(1);
        assert_eq!(gen.alloc_new_generator(rt.create_node().build()), 1);
    }
}
