use std::{collections::HashMap, sync::Mutex};

use anyhow::Result;
use jepsen_rs::{
    client::{Client, ElleRwClusterClient, JepsenClient},
    generator::{controller::GeneratorGroupStrategy, elle_rw::ElleRwGenerator, GeneratorGroup},
    op::Op,
};
use log::{info, LevelFilter};

#[derive(Debug, Default)]
pub struct TestCluster {
    db: Mutex<HashMap<u64, u64>>,
}

impl TestCluster {
    pub fn new() -> Self {
        Self {
            db: HashMap::new().into(),
        }
    }
}

#[async_trait::async_trait]
impl ElleRwClusterClient for TestCluster {
    async fn get(&self, key: u64) -> Result<Option<u64>, String> {
        Ok(self.db.lock().unwrap().get(&key).cloned())
    }
    async fn put(&self, key: u64, value: u64) -> Result<(), String> {
        self.db.lock().unwrap().insert(key, value);
        Ok(())
    }
}

#[test]
pub fn intergration_test() -> Result<()> {
    _ = pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_millis()
        .filter_module("j4rs", LevelFilter::Info)
        .parse_default_env()
        .try_init();
    let mut rt = madsim::runtime::Runtime::new();
    rt.set_allow_system_thread(true);

    let cluster = TestCluster::new();
    let raw_gen = ElleRwGenerator::new()?;
    let client = JepsenClient::new(cluster, raw_gen);
    let client = Box::leak(client.into());
    info!("intergration_test: client created");

    rt.block_on(async move {
        // get generators, transform and merge them
        let g1 = client
            .new_generator(100)
            .filter(|o| matches!(o, Op::Txn(txn) if txn.len() == 1))
            .await;
        let g2 = client.new_generator(50);
        let g3 = client.new_generator(50);
        info!("intergration_test: generators created");
        let gen_g = GeneratorGroup::new([g1, g2, g3])
            .with_strategy(GeneratorGroupStrategy::RoundRobin(usize::MAX));
        info!("generator group created");
        let res = client.run(gen_g).await.unwrap_or_else(|e| panic!("{}", e));
        info!("history checked result: {:?}", res);
    });
    Ok(())
}
