use std::{collections::HashMap, sync::Arc};

use madsim::runtime::NodeHandle;

use crate::generator::Generator;

/// The global context
#[non_exhaustive]
pub struct Global {
    pub thread_pool: HashMap<u64, NodeHandle>,
    pub gen: Arc<dyn Generator>,
}

/// The context of an operation
#[non_exhaustive]
pub struct Context {
    /// A timestamp for the operation
    pub time: u64,
    /// The process that performs the operation
    pub process: u64,
}
