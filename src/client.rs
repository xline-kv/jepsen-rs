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
    generator::{Generator, GeneratorGroup, Global, RawGenerator},
    history::HistoryType,
    op::Op,
    utils::AsyncIter,
};

/// The interface of a cluster client, needs to be implemented by the external
/// user.
pub trait ElleRwClusterClient {
    fn get(&self, key: u64) -> Option<u64>;
    fn put(&mut self, key: u64, value: u64);
}

/// The interface of a jepsen client.
pub trait Client {
    type ERR: Send + 'static;
    /// alloc a new sender if the id is new to client, and spawn a thread to
    /// receive ops corresponding to the sender.
    async fn alloc_thread(&'static self, id: u64);
    /// client received an op, send it to cluster and deal the result. The
    /// history (both invoke and result) will be recorded in this function.
    async fn handle_op(&'static self, id: u64, op: Op);
    async fn run(
        &'static self,
        gen: GeneratorGroup<'_, Op, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR>;
    fn new_generator(
        &self,
        n: usize,
    ) -> impl std::future::Future<Output = Generator<'static, Op, Self::ERR>> + Send;
}

/// A client that leads the jepsen test, execute between the generator and the
/// cluster.
pub struct JepsenClient {
    cluster_client: Mutex<Box<dyn ElleRwClusterClient + Send + Sync>>,
    pub global: Arc<Global<'static, Op, <Self as Client>::ERR>>,
    node_handle: NodeHandle,
    /// Get sender and join handle from generator id.
    sender_map: Mutex<BTreeMap<u64, Sender<Op>>>,
    join_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl JepsenClient {
    pub fn new(
        cluster: impl ElleRwClusterClient + Send + Sync + 'static,
        raw_gen: impl RawGenerator<Item = Op> + Send + 'static,
    ) -> Self {
        Self::new_with_handle(
            cluster,
            raw_gen,
            madsim::runtime::Handle::current().create_node().build(),
        )
    }

    pub fn new_with_handle(
        cluster: impl ElleRwClusterClient + Send + Sync + 'static,
        raw_gen: impl RawGenerator<Item = Op> + Send + 'static,
        node_handle: NodeHandle,
    ) -> Self {
        Self {
            cluster_client: Mutex::new(Box::new(cluster)),
            global: Arc::new(Global::new(raw_gen)),
            node_handle,
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
    async fn alloc_thread(&'static self, id: u64) {
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
                self.handle_op(id, op).await;
            }
        });
        let res = lock.insert(id, tx);
        debug_assert!(res.is_none(), "client alloc thread in duplicate id");
        self.join_handles.lock().unwrap().push(x);
    }

    fn new_generator(
        &self,
        n: usize,
    ) -> impl std::future::Future<Output = Generator<'static, Op, Self::ERR>> + Send {
        let global = self.global.clone();
        let seq = global.take_seq(n);
        Generator::new(global, tokio_stream::iter(seq))
    }

    async fn handle_op(&'static self, id: u64, op: Op) {
        self.global
            .history
            .lock()
            .unwrap()
            .push_invoke(&self.global, id, op.clone());
        let res = self.handle_op_inner(op.clone());
        match res {
            Ok(op) => {
                self.global.history.lock().unwrap().push_result(
                    &self.global,
                    id,
                    HistoryType::Ok,
                    op,
                    None,
                );
            }
            Err(err) => {
                self.global.history.lock().unwrap().push_result(
                    &self.global,
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
    async fn run(
        &'static self,
        mut gen: GeneratorGroup<'_, Op, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR> {
        while let Some((op, id)) = gen.next_with_id().await {
            self.alloc_thread(id).await;
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
            &self.global.history.lock().unwrap(),
            Some(CheckOption::default()),
        );
        check_result.map_err(|err| err.to_string())
    }
}
