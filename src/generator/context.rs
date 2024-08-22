use std::{
    collections::HashMap,
    sync::Arc,
    time::{self, Duration},
};

use madsim::runtime::NodeHandle;

use crate::generator::RawGenerator;

/// The global context
#[non_exhaustive]
pub struct Global {
    /// The thread pool
    pub thread_pool: HashMap<u64, NodeHandle>,
    /// The original generator
    pub gen: Arc<dyn RawGenerator>,
    /// The start time of the simulation
    pub start_time: time::Instant,
}

/// The context of an operation
#[non_exhaustive]
pub struct Context {
    /// A timestamp for the operation
    pub time: Duration,
    /// The process that performs the operation
    pub process: u64,
}
