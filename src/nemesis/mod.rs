pub mod register;

use std::collections::{HashMap, HashSet};

use madsim::rand::Rng;
use serde::{Deserialize, Serialize};

use crate::utils::{select_numbers_from_range, OverflowingAddRange};

pub type ServerId = u64;
/// Record the link that has been clogged.
///
/// A Net nemesis should return a [`NetRecord`], indicating the clogged links.
/// This record will be used in [`NemesisRegister`] to resume the nemesis.
pub type NetRecord = HashMap<ServerId, HashSet<ServerId>>;

#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SerializableNemesisType {
    #[serde(rename = ":bitflip-wal")]
    BitflipWal,
    #[serde(rename = ":bitflip-snap")]
    BitflipSnap,
    #[serde(rename = ":truncate-wal")]
    TruncateWal,
    #[serde(rename = ":pause")]
    Pause,
    #[serde(rename = ":kill")]
    Kill,
    #[serde(rename = ":partition")]
    Partition,
    #[serde(rename = ":clock")]
    Clock,
}

#[non_exhaustive]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum NemesisType {
    /// do nothing. No-op will not be recorded to history.
    #[default]
    Noop,
    Kill(HashSet<ServerId>),
    Pause(HashSet<ServerId>),
    SplitOne(ServerId),
    PartitionHalves(HashSet<ServerId>),
    PartitionRandomN(usize),
    PartitionMajoritiesRing,
    PartitionLeaderAndMajority,
    LeaderSendToMajorityButCannotReceive,
}

/// This enum is to record nemesis operation.
///
/// The cluster should be able to execute or resume each nemesis by one nemesis
/// record.
#[non_exhaustive]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum NemesisRecord {
    /// do nothing. No-op will not be recorded to history.
    #[default]
    Noop,
    Kill(HashSet<ServerId>),
    Pause(HashSet<ServerId>),
    /// To record the link that has been clogged.
    Net(NetRecord),
    // Note: Bitflip has no recovery mechanism, so it is not in NemesisRecord.
}

impl AsRef<NemesisRecord> for NemesisRecord {
    fn as_ref(&self) -> &NemesisRecord {
        self
    }
}

impl From<NetRecord> for NemesisRecord {
    fn from(record: NetRecord) -> Self {
        Self::Net(record)
    }
}

/// The trait for a cluster which could apply nemesis. This trait contains some
/// basic methods to implement.
#[async_trait::async_trait]
pub trait NemesisCluster {
    // impl by external cluster
    async fn kill(&self, servers: &[ServerId]);
    async fn restart(&self, servers: &[ServerId]);
    async fn pause(&self, servers: &[ServerId]);
    async fn resume(&self, servers: &[ServerId]);
    async fn get_leader_without_term(&self) -> ServerId;
    /// clog link for both side.
    fn clog_link_both(&self, fst: ServerId, snd: ServerId);
    /// unclog link for both side.
    fn unclog_link_both(&self, fst: ServerId, snd: ServerId);
    /// clog link for one side.
    fn clog_link_single(&self, fst: ServerId, snd: ServerId);
    /// unclog link for one side.
    fn unclog_link_single(&self, fst: ServerId, snd: ServerId);
    fn size(&self) -> usize;
}

/// Trait for cluster to calculate a nemesis. `calculate` means to transform a
/// Nemesis to a [`NemesisRecord`].
///
/// Every nemesis function in this trait should return a [`NemesisRecord`] for
/// recovering.
#[async_trait::async_trait]
pub trait NemesisCalculator: NemesisCluster {
    /// Calculate the nemesis record for the nemesis type.
    async fn calculate_nemesis(&self, nemesis: NemesisType) -> NemesisRecord {
        match nemesis {
            NemesisType::Noop => NemesisRecord::Noop,
            NemesisType::Kill(servers) => self.kill_servers(servers),
            NemesisType::Pause(servers) => self.pause_servers(servers),
            NemesisType::SplitOne(server) => self.partition_one(server),
            NemesisType::PartitionHalves(servers) => self.partition_halves(servers),
            NemesisType::PartitionRandomN(n) => self.partition_random_n(n),
            NemesisType::PartitionMajoritiesRing => self.partition_majorities_ring(),
            NemesisType::PartitionLeaderAndMajority => self.partition_leader_and_majority().await,
            NemesisType::LeaderSendToMajorityButCannotReceive => {
                self.leader_send_to_majority_but_cannot_receive().await
            }
        }
    }

    // utils
    #[inline]
    fn majority(&self) -> usize {
        self.size() / 2 + 1
    }
    /// clog link for both side and record the link that has been clogged.
    #[inline]
    fn clog_link_both_record(&self, fst: ServerId, snd: ServerId, recorder: &mut NetRecord) {
        self.clog_link_single_record(fst, snd, recorder);
        self.clog_link_single_record(snd, fst, recorder);
    }
    #[inline]
    fn clog_link_single_record(&self, fst: ServerId, snd: ServerId, recorder: &mut NetRecord) {
        recorder.entry(fst).or_default().insert(snd);
    }

    // nemesis calculator

    /// kill the servers in the cluster.
    #[must_use]
    fn kill_servers(&self, servers: impl IntoIterator<Item = ServerId> + Send) -> NemesisRecord {
        let servers = servers.into_iter().collect::<HashSet<_>>();
        NemesisRecord::Kill(servers)
    }

    /// pause the servers in the cluster.
    #[must_use]
    fn pause_servers(&self, servers: impl IntoIterator<Item = ServerId> + Send) -> NemesisRecord {
        let servers = servers.into_iter().collect::<HashSet<_>>();
        NemesisRecord::Pause(servers)
    }
    /// partition one server from the cluster.
    #[must_use]
    fn partition_one(&self, server: ServerId) -> NemesisRecord {
        self.partition_halves(HashSet::from([server]))
    }

    /// partition the cluster into two halves, one part is the servers parsed
    /// into.
    #[must_use]
    fn partition_halves(&self, servers: impl Into<HashSet<ServerId>> + Send) -> NemesisRecord {
        let set1 = servers.into();
        assert!(
            set1.len() < self.size(),
            "set1 must be smaller than cluster size"
        );
        let total: HashSet<_> = (0..self.size()).map(|x| x as u64).collect();
        let set2: HashSet<_> = total.difference(&set1).collect();
        let mut recorder = HashMap::new();
        for x in set1.iter() {
            for y in set2.iter() {
                self.clog_link_both_record(*x, **y, &mut recorder);
            }
        }
        recorder.into()
    }

    /// randomly select `n` servers to be partitioned
    #[must_use]
    fn partition_random_n(&self, n: usize) -> NemesisRecord {
        assert!(n < self.size(), "n must be smaller than cluster size");
        let mut part = HashSet::with_capacity(n);
        while part.len() < n {
            part.insert(madsim::rand::thread_rng().gen_range(0..self.size()) as ServerId);
        }
        dbg!(&part);
        self.partition_halves(part)
    }

    /// Partition the cluster into a ring-like relation. Each node is able to
    /// connect to majority.
    ///
    /// 4 nodes case:
    ///
    ///     0 <--> 1
    ///     ^      ^
    ///     |      |
    ///     v      v
    ///     3 <--> 2
    ///
    /// 6 nodes case:
    ///
    ///     0 ---- 1
    ///    /  \  /  \
    ///   5  --  --  2
    ///    \  /  \  /
    ///     4 ---- 3
    #[must_use]
    fn partition_majorities_ring(&self) -> NemesisRecord {
        let total: HashSet<_> = (0..self.size()).collect();
        let mut recorder = HashMap::new();
        for i in 0..self.size() {
            let expected: HashSet<_> =
                select_numbers_from_range(self.size() - 1, self.majority() - 1) // majority minus itself
                    .into_iter()
                    .map(|x| x.overflowing_add_range(i + 1, 0..self.size()))
                    .filter(|x| x >= &i) // a link only needs to clog one time
                    .collect();
            let to_be_clogged = total.difference(&expected);
            to_be_clogged.for_each(|x| {
                if *x >= i {
                    self.clog_link_both_record(i as u64, *x as u64, &mut recorder);
                }
            });
        }
        recorder.into()
    }

    /// Make leader not able to connect majority. This function will not change
    /// connections between other nodes.
    #[must_use]
    async fn partition_leader_and_majority(&self) -> NemesisRecord {
        let leader = self.get_leader_without_term().await;
        let mut recorder = HashMap::new();
        (0..self.size())
            .map(|x| x as u64)
            .filter(|x| x != &leader)
            .take(self.majority() - 1)
            .for_each(|x| self.clog_link_both_record(x, leader, &mut recorder));
        recorder.into()
    }

    /// This is a one-direction partition, make leader could send message to
    /// majority, but cannot receive the message from majority.
    #[must_use]
    async fn leader_send_to_majority_but_cannot_receive(&self) -> NemesisRecord {
        let leader = self.get_leader_without_term().await;
        let mut recorder = HashMap::new();
        (0..self.size())
            .map(|x| x as u64)
            .filter(|x| x != &leader)
            .take(self.majority() - 1)
            .for_each(|x| self.clog_link_single_record(x, leader, &mut recorder));
        recorder.into()
    }
}

/// The trait for a nemesis cluster to execute nemesis record.
#[async_trait::async_trait]
pub trait NemesisExecutor: NemesisCluster {
    /// Execute the nemesis record.
    async fn execute(&self, nemesis_record: impl AsRef<NemesisRecord> + Send) {
        match nemesis_record.as_ref() {
            NemesisRecord::Noop => {}
            NemesisRecord::Kill(servers) => {
                self.kill(servers.iter().cloned().collect::<Vec<_>>().as_slice())
                    .await;
            }
            NemesisRecord::Pause(servers) => {
                self.pause(servers.iter().cloned().collect::<Vec<_>>().as_slice())
                    .await;
            }
            NemesisRecord::Net(net_record) => {
                for (k, v) in net_record.iter() {
                    v.iter().for_each(|x| self.clog_link_single(*k, *x));
                }
            }
        }
    }
}

impl<T: NemesisCluster> NemesisCalculator for T {}
impl<T: NemesisCluster> NemesisExecutor for T {}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, VecDeque},
        sync::Mutex,
    };

    use tap::Tap;

    use super::*;

    #[derive(Debug, Default)]
    struct MockCluster {
        /// ServerId -> [ServerIds] to other connected servers
        connections: Mutex<HashMap<ServerId, HashSet<ServerId>>>,
    }

    impl MockCluster {
        fn new(n: usize) -> Self {
            let mut map = HashMap::new();
            for i in 0..n {
                map.insert(
                    i as u64,
                    (0..n as u64).collect::<HashSet<_>>().tap_mut(|x| {
                        x.remove(&(i as u64));
                    }),
                );
            }
            Self {
                connections: map.into(),
            }
        }
        fn connected_one_way(&self, fst: ServerId, snd: ServerId) -> bool {
            let l = self.connections.lock().unwrap();
            return l.get(&fst).unwrap().contains(&snd);
        }
        fn connected_two_way(&self, fst: ServerId, snd: ServerId) -> bool {
            let l = self.connections.lock().unwrap();
            return l.get(&fst).unwrap().contains(&snd) && l.get(&snd).unwrap().contains(&fst);
        }
        /// Assert that the current connections are as expected, easier to write
        /// tests.
        fn assert_eq(&self, other: Vec<Vec<ServerId>>) {
            assert_eq!(
                *self.connections.lock().unwrap(),
                other
                    .into_iter()
                    .enumerate()
                    .map(|x| (x.0 as u64, x.1.into_iter().collect()))
                    .collect()
            );
        }

        /// Get all reachable nodes from `start`, using bfs.
        fn get_all_reachable_nodes(&self, start: ServerId) -> HashSet<ServerId> {
            let mut res = HashSet::new();
            let l = self.connections.lock().unwrap();
            let mut q = VecDeque::from([start]);
            while let Some(x) = q.pop_front() {
                res.insert(x);
                q.extend(l.get(&x).unwrap().iter().filter(|x| !res.contains(x)));
            }
            res
        }
    }

    #[async_trait::async_trait]
    impl NemesisCluster for MockCluster {
        async fn kill(&self, _servers: &[ServerId]) {
            unimplemented!()
        }
        async fn restart(&self, _servers: &[ServerId]) {
            unimplemented!()
        }
        async fn pause(&self, _servers: &[ServerId]) {
            unimplemented!()
        }
        async fn resume(&self, _servers: &[ServerId]) {
            unimplemented!()
        }
        async fn get_leader_without_term(&self) -> ServerId {
            0
        }
        fn clog_link_both(&self, fst: ServerId, snd: ServerId) {
            let mut l = self.connections.lock().unwrap();
            l.get_mut(&fst).unwrap().remove(&snd);
            l.get_mut(&snd).unwrap().remove(&fst);
        }
        fn unclog_link_both(&self, fst: ServerId, snd: ServerId) {
            let mut l = self.connections.lock().unwrap();
            l.get_mut(&fst).unwrap().insert(snd);
            l.get_mut(&snd).unwrap().insert(fst);
        }
        fn clog_link_single(&self, fst: ServerId, snd: ServerId) {
            let mut l = self.connections.lock().unwrap();
            l.get_mut(&fst).unwrap().remove(&snd);
        }
        fn unclog_link_single(&self, fst: ServerId, snd: ServerId) {
            let mut l = self.connections.lock().unwrap();
            l.get_mut(&fst).unwrap().insert(snd);
        }
        fn size(&self) -> usize {
            self.connections.lock().unwrap().len()
        }
    }

    #[test]
    fn test_mock_cluster() {
        let cluster = MockCluster::new(4);
        cluster.assert_eq(vec![
            vec![1, 2, 3],
            vec![0, 2, 3],
            vec![0, 1, 3],
            vec![0, 1, 2],
        ]);
    }

    #[madsim::test]
    async fn test_execute_nemesis_record() {
        let cluster = MockCluster::new(3);
        // clog link for 0 -> 1 and 0 -> 2
        cluster
            .execute(NemesisRecord::Net([(0, [1, 2].into())].into()))
            .await;
        cluster.assert_eq(vec![vec![], vec![0, 2], vec![0, 1]]);
        // clog link for 1 -> 2
        cluster
            .execute(NemesisRecord::Net([(1, [2].into())].into()))
            .await;
        cluster.assert_eq(vec![vec![], vec![0], vec![0, 1]]);
    }

    #[madsim::test]
    async fn test_partition_halves() {
        let cluster = MockCluster::new(5);
        cluster
            .execute(cluster.partition_halves((0..=2).collect::<HashSet<_>>()))
            .await;
        cluster.assert_eq(vec![vec![1, 2], vec![0, 2], vec![0, 1], vec![4], vec![3]]);

        let cluster = MockCluster::new(6);
        cluster
            .execute(cluster.partition_halves((0..=2).collect::<HashSet<_>>()))
            .await;
        cluster.assert_eq(vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
            vec![4, 5],
            vec![3, 5],
            vec![3, 4],
        ]);
    }

    #[madsim::test]
    async fn test_partition_majorities_ring() {
        let cluster = MockCluster::new(4);
        cluster.execute(cluster.partition_majorities_ring()).await;
        cluster.assert_eq(vec![vec![1, 3], vec![0, 2], vec![1, 3], vec![0, 2]]);

        let cluster = MockCluster::new(6);
        cluster.execute(cluster.partition_majorities_ring()).await;
        cluster.assert_eq(vec![
            vec![1, 3, 5],
            vec![0, 2, 4],
            vec![1, 3, 5],
            vec![0, 2, 4],
            vec![1, 3, 5],
            vec![0, 2, 4],
        ]);
    }

    #[madsim::test]
    async fn test_partition_leader_and_majority() {
        let cluster = MockCluster::new(5);
        cluster
            .execute(cluster.partition_leader_and_majority().await)
            .await;
        let leader = cluster.get_leader_without_term().await;
        let leader_connections_num = (0..cluster.size())
            .map(|x| x as u64)
            .filter(|x| *x != leader)
            .filter(|x| cluster.connected_two_way(leader, *x))
            .count();
        assert!(leader_connections_num < cluster.majority());
        dbg!(cluster.connections.lock().unwrap().get(&leader).unwrap());
    }

    #[madsim::test]
    async fn test_partition_random_n() {
        let cluster = MockCluster::new(6);
        cluster.execute(cluster.partition_random_n(3)).await;
        assert_eq!(cluster.get_all_reachable_nodes(0).len(), 3);
    }

    #[madsim::test]
    async fn test_leader_send_to_majority_but_cannot_receive() {
        let cluster = MockCluster::new(5);
        cluster
            .execute(cluster.leader_send_to_majority_but_cannot_receive().await)
            .await;
        let leader = cluster.get_leader_without_term().await;
        let leader_connections_num = (0..cluster.size())
            .map(|x| x as u64)
            .filter(|x| *x != leader)
            .filter(|x| cluster.connected_one_way(*x, leader))
            .count();
        assert!(leader_connections_num < cluster.majority());
    }
}
