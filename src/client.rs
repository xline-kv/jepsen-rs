use std::sync::{
    mpsc::{self, Sender},
    Arc, Mutex,
};

use madsim::runtime::NodeHandle;
use tokio::task::JoinHandle;

use crate::{
    checker::SerializableCheckResult,
    generator::{GeneratorGroup, Global},
    op::Op,
};

/// The interface of a client, needs to be implemented by the external user.
pub trait Client {
    type ERR: Send;
    /// get a new sender from the client, and spawn a thread to receive ops.
    async fn new_sender<C: Client + Send + Sync + 'static>(
        &'static self,
        global: Arc<Global<'static, C, Self::ERR>>,
        id: u64,
        op: Op,
    ) -> Sender<Op>;
    /// client received an op, send it to cluster and deal the result. The
    /// history (both invoke and result) will be recorded in this function.
    async fn handle_op<C: Client + Send + Sync + 'static>(
        &'static self,
        global: &Arc<Global<'_, C, Self::ERR>>,
        id: u64,
        op: Op,
    );
    async fn start_test<'a, C: Client + Send + Sync + 'static>(
        &'static self,
        global: Arc<Global<'a, C, Self::ERR>>,
        gen: GeneratorGroup<'a, C, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR>;
}

/// A client only for testing
pub(crate) struct TestClient {
    handle: NodeHandle,
    rx_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl TestClient {
    pub fn new() -> Self {
        Self {
            handle: madsim::runtime::Handle::current().create_node().build(),
            rx_handles: Mutex::new(vec![]),
        }
    }
}

impl Client for TestClient {
    type ERR = crate::history::ErrorType;
    async fn new_sender<C: Client + Send + Sync + 'static>(
        &'static self,
        global: Arc<Global<'static, C, Self::ERR>>,
        id: u64,
        op: Op,
    ) -> Sender<Op> {
        let (tx, rx) = mpsc::channel();
        let x = self.handle.spawn(async move {
            while let Ok(op) = rx.recv() {
                self.handle_op(&global, id, op);
            }
        });
        self.rx_handles
            .lock()
            .expect("cannot lock client handles vec")
            .push(x);
        tx
    }

    async fn handle_op<C: Client + Send + Sync + 'static>(
        &'static self,
        global: &Arc<Global<'_, C, Self::ERR>>,
        id: u64,
        op: Op,
    ) {
        todo!()
    }

    async fn start_test<'a, C: Client + Send + Sync + 'static>(
        &'static self,
        global: Arc<Global<'a, C, Self::ERR>>,
        gen: GeneratorGroup<'a, C, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR> {
        todo!()
    }
}
