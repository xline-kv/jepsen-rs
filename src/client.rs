use std::sync::Arc;

use madsim::runtime::NodeHandle;

use crate::{
    checker::SerializableCheckResult,
    generator::{GeneratorGroup, Global},
    op::Op,
};

/// The interface of a client, needs to be implemented by the external user.
pub trait Client {
    type ERR: Send;
    /// get a new node handle from the client
    async fn new_handle(&self) -> NodeHandle;
    /// client receive an op, send it to cluster and return the result. The
    /// history will also be recorded in this function.
    async fn handle_op<C: Client>(
        &mut self,
        global: Arc<Global<'_, C, Self::ERR>>,
        id: u64,
        op: Op,
    ) -> Result<Op, Self::ERR>;
    async fn start_test<'a, C: Client>(
        &mut self,
        global: Arc<Global<'a, C, Self::ERR>>,
        gen: GeneratorGroup<'a, C, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR>;
}

/// A client only for testing
pub(crate) struct TestClient;

impl Client for TestClient {
    type ERR = crate::history::ErrorType;
    async fn new_handle(&self) -> NodeHandle {
        madsim::runtime::Handle::current().create_node().build()
    }

    async fn handle_op<C: Client>(
        &mut self,
        global: Arc<Global<'_, C, Self::ERR>>,
        id: u64,
        op: Op,
    ) -> Result<Op, Self::ERR> {
        todo!()
    }

    async fn start_test<'a, C: Client>(
        &mut self,
        global: Arc<Global<'a, C, Self::ERR>>,
        gen: GeneratorGroup<'a, C, Self::ERR>,
    ) -> Result<SerializableCheckResult, Self::ERR> {
        todo!()
    }
}
