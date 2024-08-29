use std::collections::HashMap;

use anyhow::Result;
use jepsen_rs::{
    client::{Client, ElleRwClusterClient, JepsenClient},
    generator::{controller::GeneratorGroupStrategy, elle_rw::ElleRwGenerator, GeneratorGroup},
    op::Op,
};

#[derive(Debug, Clone, Default)]
pub struct TestCluster {
    db: HashMap<u64, u64>,
}

impl TestCluster {
    pub fn new() -> Self {
        Self { db: HashMap::new() }
    }
}

impl ElleRwClusterClient for TestCluster {
    fn get(&self, key: u64) -> Option<u64> {
        self.db.get(&key).cloned()
    }
    fn put(&mut self, key: u64, value: u64) {
        self.db.insert(key, value);
    }
}

#[test]
pub fn intergration_test() -> Result<()> {
    let rt = madsim::runtime::Runtime::new();
    let handle = rt.handle();
    let node_handle = handle.create_node().build();
    let cluster = TestCluster::new();
    let raw_gen = ElleRwGenerator::new()?;
    let client = JepsenClient::new_with_handle(cluster, raw_gen, node_handle);
    let client = Box::leak(client.into());
    println!("client created");

    rt.block_on(async move {
        // get generators, transform and merge them
        let g1 = client
            .new_generator(100)
            .await
            .filter(|o| matches!(o, Op::Txn(txn) if txn.len() == 1));
        let g2 = client.new_generator(50).await;
        let g3 = client.new_generator(50).await;
        println!("generators created");

        let gen_g = GeneratorGroup::new([g1, g2, g3])
            .with_strategy(GeneratorGroupStrategy::RoundRobin(usize::MAX));
        println!("generator group created");
        let res = client.run(gen_g).await.unwrap_or_else(|e| panic!("{}", e));
        println!("{:?}", res);
    });
    Ok(())
}
