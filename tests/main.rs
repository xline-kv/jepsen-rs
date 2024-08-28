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

    pub fn handle_op_inner(&mut self, op: Op) -> Result<Op, String> {
        match op {
            Op::Read(key, _) => {
                let value = self.get(key);
                Ok(Op::Read(key, value))
            }
            Op::Write(key, value) => {
                self.put(key, value);
                Ok(Op::Write(key, value))
            }
            Op::Txn(ops) => Ok(Op::Txn(
                ops.into_iter()
                    .map(|op| self.handle_op_inner(op))
                    .collect::<Result<_, _>>()?,
            )),
        }
    }
}

impl Client for TestCluster {
    type ERR = String;
    async fn handle_op<C: Client>(
        &mut self,
        global: std::sync::Arc<Global<'_, C, Self::ERR>>,
        id: u64,
        op: Op,
    ) -> Result<Op, Self::ERR> {
        global
            .history
            .lock()
            .unwrap()
            .push_invoke(&global, id, op.clone());
        let res = self.handle_op_inner(op.clone());
        match res {
            Ok(op) => {
                global.history.lock().unwrap().push_result(
                    &global,
                    id,
                    HistoryType::Ok,
                    op.clone(),
                    None,
                );
                return Ok(op);
            }
            Err(err) => {
                global.history.lock().unwrap().push_result(
                    &global,
                    id,
                    HistoryType::Fail,
                    op,
                    Some(err.clone()),
                );
                Err(err)
            }
        }
    }
    async fn new_handle(&self) -> madsim::runtime::NodeHandle {
        madsim::runtime::Handle::current().create_node().build()
    }
    async fn start_test<'a, C: Client>(
        &mut self,
        global: std::sync::Arc<Global<'a, C, Self::ERR>>,
        gen: jepsen_rs::generator::GeneratorGroup<'a, C, Self::ERR>,
    ) -> Result<jepsen_rs::checker::SerializableCheckResult, Self::ERR> {
        todo!()
    }
}
