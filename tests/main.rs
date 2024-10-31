use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex, RwLock},
};

use anyhow::Result;
use jepsen_rs::{
    checker::ValidType,
    client::{Client, ElleRwClusterClient, JepsenClient},
    generator::{
        controller::GeneratorGroupStrategy, elle_rw::ElleRwGenerator, GeneratorGroup,
        NemesisRawGenWrapper,
    },
    nemesis::{
        implementation::NemesisCluster, register::NemesisRegisterStrategy, NemesisType, ServerId,
    },
    op::{nemesis::OpOrNemesis, Op},
};
use log::{info, LevelFilter};

/// Mock cluster
#[derive(Default)]
pub struct TestCluster {
    db: Mutex<HashMap<u64, u64>>,
    size: usize,
    /// In TestCluster, if false_num > quorum, the get/put operation will
    /// fail.
    status: RwLock<Vec<bool>>,
}

impl TestCluster {
    /// Create a new TestCluster.
    pub fn new() -> Self {
        let size = 5;
        Self {
            db: HashMap::new().into(),
            size,
            status: RwLock::new(vec![true; size]),
        }
    }

    #[inline]
    pub fn quorum(&self) -> usize {
        self.size / 2 + 1
    }

    #[inline]
    pub fn nemesis_num(&self) -> usize {
        self.status.read().unwrap().iter().filter(|x| !**x).count()
    }
}

/// The client of TestCluster, to execute get/put/txn operation.
pub struct TestClient(pub Arc<TestCluster>);

impl Deref for TestClient {
    type Target = Arc<TestCluster>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Accept a get/put/txn operation.
#[async_trait::async_trait]
impl ElleRwClusterClient for TestClient {
    async fn get(&self, key: u64) -> Result<Option<u64>, String> {
        if self.nemesis_num() > self.quorum() {
            return Err("nemesis_num > quorum".to_string());
        }
        Ok(self.db.lock().unwrap().get(&key).cloned())
    }
    async fn put(&self, key: u64, value: u64) -> Result<(), String> {
        if self.nemesis_num() > self.quorum() {
            return Err("nemesis_num > quorum".to_string());
        }
        self.db.lock().unwrap().insert(key, value);
        Ok(())
    }
    /// A txn operation should only contains read/write operations.
    async fn txn(&self, mut ops: Vec<Op>) -> Result<Vec<Op>, String> {
        if self.nemesis_num() > self.quorum() {
            return Err("nemesis_num > quorum".to_string());
        }
        let mut lock = self.db.lock().unwrap();
        for op in ops.iter_mut() {
            match op {
                Op::Read(key, value) => {
                    *value = lock.get(key).cloned();
                }
                Op::Write(key, value) => {
                    lock.insert(*key, *value);
                }
                _ => {
                    return Err(
                        "txn cannot be in txn, otherwise there will be a deadlock".to_string()
                    );
                }
            }
        }
        Ok(ops)
    }
}

/// Implementation of NemesisCluster. If the nemesis_num > quorum, the get/put
/// will fail. The kill/restart/pause/resume methods will only record the
/// nemesis_num (the mock implementation), not truely kill/restart/pause/resume
/// them.
#[async_trait::async_trait]
impl NemesisCluster for TestCluster {
    async fn kill(&self, servers: &[ServerId]) {
        let mut lock = self.status.write().unwrap();
        for id in servers {
            lock[*id as usize] = false;
        }
    }
    async fn restart(&self, servers: &[ServerId]) {
        let mut lock = self.status.write().unwrap();
        for id in servers {
            lock[*id as usize] = true;
        }
    }
    async fn pause(&self, servers: &[ServerId]) {
        self.kill(servers).await;
    }
    async fn resume(&self, servers: &[ServerId]) {
        self.restart(servers).await;
    }
    async fn get_leader_without_term(&self) -> ServerId {
        0
    }

    // we do not deal with network in mock cluster.
    fn clog_link_both(&self, _: ServerId, _: ServerId) {}
    fn unclog_link_both(&self, _: ServerId, _: ServerId) {}
    fn clog_link_single(&self, _: ServerId, _: ServerId) {}
    fn unclog_link_single(&self, _: ServerId, _: ServerId) {}
    fn size(&self) -> usize {
        self.size
    }
}

#[test]
pub fn intergration_test_without_nemesis() -> Result<()> {
    _ = pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_millis()
        .filter_module("j4rs", LevelFilter::Info)
        .parse_default_env()
        .try_init();
    let mut rt = madsim::runtime::Runtime::new();
    rt.set_allow_system_thread(true); // needed by j4rs

    let cluster = Arc::new(TestCluster::new());
    let raw_gen = ElleRwGenerator::new()?;
    let jepsen_client = JepsenClient::new(
        cluster.clone(),
        TestClient(cluster),
        NemesisRawGenWrapper(Box::new(raw_gen)),
    );
    let client = Box::leak(jepsen_client.into());
    info!("intergration_test: client created");

    rt.block_on(async move {
        // get generators, transform and merge them
        let g1 = client
            .new_generator(100)
            .filter(|o| matches!(o, OpOrNemesis::Op(Op::Txn(txn)) if txn.len() == 1))
            .await;
        let g2 = client.new_generator(50);
        let g3 = client.new_generator(50);
        info!("intergration_test: generators created");
        let gen_g = GeneratorGroup::new([g1, g2, g3])
            .with_strategy(GeneratorGroupStrategy::RoundRobin(usize::MAX));
        info!("generator group created");
        let res = client.run(gen_g).await.unwrap_or_else(|e| panic!("{}", e));
        info!("history checked result: {:?}", res);
        assert!(matches!(res.valid, ValidType::True));
    });
    Ok(())
}

#[test]
fn intergration_test_with_nemesis() -> Result<()> {
    _ = pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_millis()
        .filter_module("j4rs", LevelFilter::Info)
        .parse_default_env()
        .try_init();
    let mut rt = madsim::runtime::Runtime::new();
    rt.set_allow_system_thread(true); // needed by j4rs

    let cluster = Arc::new(TestCluster::new());
    let raw_gen = ElleRwGenerator::new()?;
    let client = JepsenClient::new(
        cluster.clone(),
        TestClient(cluster),
        NemesisRawGenWrapper(Box::new(raw_gen)),
    )
    // here we allow 2 nemeses at the same time.
    .with_n_register_strategy(NemesisRegisterStrategy::FIFO(2));
    let client = Box::leak(client.into());
    info!("intergration_test: client created");

    rt.block_on(async move {
        // get generators, transform and merge them
        let g1 = client
            .new_generator(100)
            .filter(|o| matches!(o, OpOrNemesis::Op(Op::Txn(txn)) if txn.len() == 1))
            .await;
        let g2 = client.new_generator(50);
        let g3 = client.new_generator(50);

        // 2 nemeses. the 2 will be both active, so the get/put after second nemesis
        // will fail.
        let ng = client.new_nemeses([
            NemesisType::Kill([1, 2].into_iter().collect()),
            NemesisType::Kill([3, 4].into_iter().collect()),
        ]);
        info!("intergration_test: generators created");
        let gen_g = GeneratorGroup::new_with_count([(g1, 20), (g2, 10), (g3, 10), (ng, 1)])
            .with_strategy(GeneratorGroupStrategy::RoundRobin(usize::MAX));
        info!("generator group created");
        let res = client.run(gen_g).await.unwrap_or_else(|e| panic!("{}", e));
        info!("history checked result: {:?}", res);
        assert!(matches!(res.valid, ValidType::True));
    });
    Ok(())
}
