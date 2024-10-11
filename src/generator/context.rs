use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use madsim::time;

use super::RawGenerator;
use crate::{
    history::{ErrorType, SerializableHistoryList},
    op::{Op, OpFunctionType},
};

type IdSetType = Arc<Mutex<BTreeSet<u64>>>;

/// The global context
#[non_exhaustive]
pub struct Global<'a, T: Send = Op, ERR: Send = ErrorType> {
    /// The id allocator and handle pool.
    /// This is like a dispatcher, when an [`Op`] generated, it will be sent to
    /// the corresponding sender, aka a madsim thread. This thread will try
    /// to receive the `Op` and execute it.
    pub id_set: IdSetType,
    /// The original raw generator
    pub gen: Mutex<Option<Box<dyn RawGenerator<Item = T> + Send + 'a>>>,
    /// The start time of the simulation
    pub start_time: time::Instant,
    /// The history list
    pub history: Mutex<SerializableHistoryList<OpFunctionType, ERR>>,
}

impl<'a, T: Send + 'a, ERR: Send> Global<'a, T, ERR> {
    /// Create a new global context
    pub fn new(gen: impl RawGenerator<Item = T> + Send + 'a) -> Self {
        let h: SerializableHistoryList<OpFunctionType, ERR> = Default::default();
        Self {
            id_set: Mutex::new(BTreeSet::new()).into(),
            gen: Mutex::new(Some(
                Box::new(gen) as Box<dyn RawGenerator<Item = T> + Send + 'a>
            )),
            start_time: time::Instant::now(),
            history: Mutex::new(h),
        }
    }

    /// Take the next `n` ops from the raw generator.
    pub fn take_seq(&self, n: usize) -> Vec<T> {
        if let Some(gen) = self.gen.lock().expect("Failed to lock gen").as_mut() {
            gen.gen_n(n)
        } else {
            Vec::new()
        }
    }
}
