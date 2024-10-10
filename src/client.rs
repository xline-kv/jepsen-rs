use std::sync::Arc;

use anyhow::Result;
use log::{debug, info, trace};

use crate::{
    checker::{elle_rw::ElleRwChecker, Check, CheckOption, SerializableCheckResult},
    generator::{Generator, GeneratorBuilder, GeneratorGroup, Global, RawGenerator},
    history::HistoryType,
    op::Op,
    utils::AsyncIter,
};

/// The interface of a cluster client, needs to be implemented by the external
/// user.
#[async_trait::async_trait]
pub trait ElleRwClusterClient {
    async fn get(&self, key: u64) -> std::result::Result<Option<u64>, String>;
    async fn put(&self, key: u64, value: u64) -> std::result::Result<(), String>;
}

/// The interface of a jepsen client.
#[async_trait::async_trait]
pub trait Client {
    type ERR: Send + 'static;
    /// client received an op, send it to cluster and deal the result. The
    /// history (both invoke and result) will be recorded in this function.
    async fn handle_op(&'static self, id: u64, op: Op);
    async fn run(
        &'static self,
        gen: GeneratorGroup<'_, Op, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR>;
    fn new_generator(&self, n: usize) -> Generator<'static, Op, Self::ERR>;
}

/// A client that leads the jepsen test, execute between the generator and the
/// cluster.
pub struct JepsenClient<EC: ElleRwClusterClient + Send + Sync + 'static> {
    cluster_client: EC,
    pub global: Arc<Global<'static, Op, <Self as Client>::ERR>>,
}

impl<EC: ElleRwClusterClient + Send + Sync + 'static> JepsenClient<EC> {
    pub fn new(cluster: EC, raw_gen: impl RawGenerator<Item = Op> + Send + 'static) -> Self {
        Self {
            cluster_client: cluster,
            global: Arc::new(Global::new(raw_gen)),
        }
    }

    /// Recursively handle an op, return the result.
    #[allow(clippy::await_holding_lock)]
    #[async_recursion::async_recursion]
    pub async fn handle_op_inner(&self, op: Op) -> std::result::Result<Op, String> {
        match op {
            Op::Read(key, _) => {
                let value = self.cluster_client.get(key).await?;
                Ok(Op::Read(key, value))
            }
            Op::Write(key, value) => {
                self.cluster_client.put(key, value).await?;
                Ok(Op::Write(key, value))
            }
            Op::Txn(ops) => Ok(Op::Txn(
                futures_util::future::join_all(ops.into_iter().map(|op| self.handle_op_inner(op)))
                    .await
                    .into_iter()
                    .collect::<Result<_, _>>()?,
            )),
        }
    }
}

#[async_trait::async_trait]
impl<EC: ElleRwClusterClient + Send + Sync + 'static> Client for JepsenClient<EC> {
    type ERR = String;

    fn new_generator(&self, n: usize) -> Generator<'static, Op, Self::ERR> {
        debug!("Jepsen client make new generator with {} ops", n);
        let global = self.global.clone();
        let seq = global.take_seq(n);
        GeneratorBuilder::new(global)
            .seq(tokio_stream::iter(seq))
            .build()
    }

    async fn handle_op(&'static self, id: u64, op: Op) {
        trace!(
            "Jepsen client thread {} receive and handles an op: {:?}",
            id,
            op
        );
        self.global
            .history
            .lock()
            .unwrap()
            .push_invoke(&self.global, id, op.clone());
        let res = self.handle_op_inner(op.clone()).await;
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
            self.handle_op(id, op).await;
        }
        info!("all receiver threads exited, check result...");

        // let his = serde_json::to_string(&self.global.history.lock().unwrap().
        // deref()).unwrap(); std::fs::write("test.json", his);

        let check_result = ElleRwChecker::default()
            .check(&self.global.history.lock().unwrap(), CheckOption::default());
        check_result.map_err(|err| err.to_string())
    }
}
