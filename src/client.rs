use std::{
    collections::BTreeMap,
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
};

use anyhow::Result;
use madsim::runtime::NodeHandle;
use tokio::task::JoinHandle;

use crate::{
    checker::{elle_rw::ElleRwChecker, Check, CheckOption, SerializableCheckResult},
    generator::{GeneratorGroup, Global},
    history::HistoryType,
    op::Op,
    utils::AsyncIter,
};

/// The interface of a cluster client, needs to be implemented by the external
/// user.
pub trait ClusterClient {
    fn get(&self, key: u64) -> Option<u64>;
    fn put(&mut self, key: u64, value: u64);
}

struct TestCluster;
impl TestCluster {
    fn new() -> Self {
        Self {}
    }
}
impl ClusterClient for TestCluster {
    fn get(&self, key: u64) -> Option<u64> {
        None
    }
    fn put(&mut self, key: u64, value: u64) {}
}

/// The interface of a jepsen client.
pub trait Client {
    type ERR: Send;
    /// alloc a new sender, and spawn a thread to receive ops.
    async fn alloc_thread(
        &'static self,
        global: Arc<Global<'static, Result<Op>, Self::ERR>>,
        id: u64,
    );
    /// client received an op, send it to cluster and deal the result. The
    /// history (both invoke and result) will be recorded in this function.
    async fn handle_op(
        &'static self,
        global: &Arc<Global<'_, Result<Op>, Self::ERR>>,
        id: u64,
        op: Op,
    );
    async fn start_test(
        &'static self,
        global: Arc<Global<'static, Result<Op>, Self::ERR>>,
        gen: GeneratorGroup<'_, Result<Op>, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR>;
}

/// A client that leads the jepsen test, execute between the generator and the
/// cluster.
pub(crate) struct JepsenClient {
    cluster_client: Mutex<Box<dyn ClusterClient + Send + Sync>>,
    node_handle: NodeHandle,
    /// Get sender and join handle from generator id.
    sender_map: Mutex<BTreeMap<u64, Sender<Op>>>,
    join_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl JepsenClient {
    pub fn new() -> Self {
        Self {
            cluster_client: Mutex::new(Box::new(TestCluster::new())),
            node_handle: madsim::runtime::Handle::current().create_node().build(),
            sender_map: Mutex::new(BTreeMap::new()),
            join_handles: vec![].into(),
        }
    }
    pub fn drop_senders(&self) {
        self.sender_map.lock().unwrap().clear();
    }
    /// Recursively handle an op, return the result.
    pub fn handle_op_inner(&self, op: Op) -> Result<Op, String> {
        match op {
            Op::Read(key, _) => {
                let value = self.cluster_client.lock().unwrap().get(key);
                Ok(Op::Read(key, value))
            }
            Op::Write(key, value) => {
                self.cluster_client.lock().unwrap().put(key, value);
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

impl Client for JepsenClient {
    type ERR = String;
    async fn alloc_thread(
        &'static self,
        global: Arc<Global<'static, Result<Op>, Self::ERR>>,
        id: u64,
    ) {
        let mut lock = self
            .sender_map
            .lock()
            .expect("cannot lock client handles vec");

        // Reuse the sender and thread. That means the generator pointed by this id has
        // been dropped and reallocated.
        if lock.contains_key(&id) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let x = self.node_handle.spawn(async move {
            while let Ok(op) = rx.recv() {
                self.handle_op(&global, id, op).await;
            }
        });
        let res = lock.insert(id, tx);
        debug_assert!(res.is_none(), "client alloc thread in duplicate id");
        self.join_handles.lock().unwrap().push(x);
    }

    async fn handle_op(
        &'static self,
        global: &Arc<Global<'_, Result<Op>, Self::ERR>>,
        id: u64,
        op: Op,
    ) {
        global
            .history
            .lock()
            .unwrap()
            .push_invoke(global, id, op.clone());
        let res = self.handle_op_inner(op.clone());
        match res {
            Ok(op) => {
                global
                    .history
                    .lock()
                    .unwrap()
                    .push_result(global, id, HistoryType::Ok, op, None);
            }
            Err(err) => {
                global.history.lock().unwrap().push_result(
                    global,
                    id,
                    HistoryType::Fail,
                    op,
                    Some(err),
                );
            }
        }
    }

    // There will be only one thread to run start_test, so the `join_handles` lock
    // will be held only by one thread, which could be safely held across await
    // point.
    #[allow(clippy::await_holding_lock)]
    async fn start_test(
        &'static self,
        global: Arc<Global<'static, Result<Op>, Self::ERR>>,
        mut gen: GeneratorGroup<'_, Result<Op>, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR> {
        while let Some((op, id)) = gen.next_with_id().await {
            let op = op.map_err(|err| err.to_string())?;
            self.alloc_thread(global.clone(), id).await;
            if let Some(sender) = self.sender_map.lock().unwrap().get_mut(&id) {
                sender.send(op).unwrap();
            } else {
                unreachable!("sender must exist after alloc");
            }
        }
        self.drop_senders();

        for handle in self.join_handles.lock().unwrap().drain(..) {
            handle.await.expect("join thread error");
        }

        let check_result = ElleRwChecker::default().check(
            &global.history.lock().unwrap(),
            Some(CheckOption::default()),
        );
        check_result.map_err(|err| err.to_string())
    }
}
