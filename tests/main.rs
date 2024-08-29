use std::collections::HashMap;

use jepsen_rs::{client::Client, generator::Global, history::HistoryType, op::Op};

#[derive(Debug, Clone, Default)]
pub struct TestCluster {
    db: HashMap<u64, u64>,
}

impl TestCluster {
    pub fn new() -> Self {
        Self { db: HashMap::new() }
    }
    pub fn get(&self, key: u64) -> Option<u64> {
        self.db.get(&key).cloned()
    }
    pub fn put(&mut self, key: u64, value: u64) {
        self.db.insert(key, value);
    }
}
