use std::{collections::BTreeSet, sync::Mutex};

use anyhow::Result;
use madsim::time;

use super::{GeneratorId, RawGenerator};
use crate::{history::SerializableHistoryList, op::Op};

/// The global context
#[non_exhaustive]
pub struct Global<'a, T: Send = Result<Op>> {
    /// The thread pool
    pub id_set: Mutex<BTreeSet<GeneratorId>>,
    /// The original raw generator
    pub gen: Mutex<Option<Box<dyn RawGenerator<Item = T> + Send + 'a>>>,
    /// The start time of the simulation
    pub start_time: time::Instant,
    /// The history list
    pub history: Mutex<SerializableHistoryList>,
}

impl<'a, T: Send + 'a> Global<'a, T> {
    /// Create a new global context
    pub fn new(gen: impl RawGenerator<Item = T> + Send + 'a) -> Self {
        Self {
            id_set: Mutex::new(BTreeSet::new()),
            gen: Mutex::new(Some(
                Box::new(gen) as Box<dyn RawGenerator<Item = T> + Send + 'a>
            )),
            start_time: time::Instant::now(),
            history: Mutex::new(SerializableHistoryList::default()),
        }
    }
    /// Find the minimal usable id in the thread pool
    pub fn get_next_id(&self) -> GeneratorId {
        let pool = self.id_set.lock().expect("Failed to lock thread pool");
        for (index, id) in pool.iter().enumerate() {
            if index as u64 != *id {
                return index as u64;
            }
        }
        pool.len() as u64
    }
    /// Allocate a new generator id
    pub fn alloc_id(&self) -> GeneratorId {
        let id = self.get_next_id();
        let res = self
            .id_set
            .lock()
            .expect("Failed to lock thread pool")
            .insert(id);
        debug_assert!(res, "insert must be success");
        id
    }
    /// Free the generator id
    pub fn free_id(&self, id: GeneratorId) -> bool {
        self.id_set
            .lock()
            .expect("Failed to lock thread pool")
            .remove(&id)
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
    use crate::generator::elle_rw::ElleRwGenerator;

    #[test]
    fn test_alloc_and_free_id() {
        let global = Global::new(ElleRwGenerator::new().unwrap());
        assert_eq!(global.alloc_id(), 0);
        assert_eq!(global.alloc_id(), 1);
        assert_eq!(global.alloc_id(), 2);
        assert!(global.free_id(1));
        assert!(!global.free_id(1));
        assert_eq!(global.alloc_id(), 1);
    }
}
