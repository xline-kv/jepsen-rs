use std::collections::HashSet;

use madsim::rand::Rng;
use serde::{Deserialize, Serialize};

use crate::utils::{select_numbers_from_range, OverflowingAddRange};

pub type ServerId = u64;

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
#[derive(Debug, Default)]
pub enum NemesisType {
    #[default]
    Noop,
    SplitOne(ServerId),
}

/// The trait for a cluster which could apply nemesis. This trait contains some
/// basic methods to implement.
#[async_trait::async_trait]
pub trait NemesisCluster {
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

    fn majority(&self) -> usize {
        self.size() / 2 + 1
    }
    fn clog_or_resume_link_both(&self, fst: ServerId, snd: ServerId, resume: bool) {
        debug_assert!(fst < self.size() as u64);
        debug_assert!(snd < self.size() as u64);
        if resume {
            self.unclog_link_both(fst, snd)
        } else {
            self.clog_link_both(fst, snd)
        }
    }
    fn clog_or_resume_link_single(&self, fst: ServerId, snd: ServerId, resume: bool) {
        debug_assert!(fst < self.size() as u64);
        debug_assert!(snd < self.size() as u64);
        if resume {
            self.unclog_link_single(fst, snd)
        } else {
            self.clog_link_single(fst, snd)
        }
    }
}

/// The trait for a nemesis cluster to execute nemesis command.
#[async_trait::async_trait]
pub trait Nemesis: NemesisCluster {
    async fn partition_one(&self, server: ServerId) {
        self.partition_halves(HashSet::from([server])).await;
    }
    /// partition the cluster into two halves, one part is the servers parsed
    /// into.
    async fn partition_halves(&self, servers: impl Into<HashSet<ServerId>> + Send) {
        self.partition_halves_inner(servers, false).await
    }
    /// resume the previously partitioned cluster
    async fn partition_halves_resume(&self, servers: impl Into<HashSet<ServerId>> + Send) {
        self.partition_halves_inner(servers, true).await
    }
    async fn partition_halves_inner(
        &self,
        servers: impl Into<HashSet<ServerId>> + Send,
        resume: bool,
    ) {
        let set1 = servers.into();
        assert!(
            set1.len() < self.size(),
            "set1 must be smaller than cluster size"
        );
        let total: HashSet<_> = (0..self.size()).map(|x| x as u64).collect();
        let set2: HashSet<_> = total.difference(&set1).collect();
        for x in set1.iter() {
            for y in set2.iter() {
                self.clog_or_resume_link_both(*x, **y, resume);
            }
        }
    }

    /// randomly select `n` servers to be partitioned
    async fn partition_random_n(&self, n: usize) {
        assert!(n < self.size(), "n must be smaller than cluster size");
        let mut part = HashSet::with_capacity(n);
        while part.len() < n {
            part.insert(madsim::rand::thread_rng().gen_range(0..self.size()) as ServerId);
        }
        dbg!(&part);
        self.partition_halves(part).await
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
    async fn partition_majorities_ring(&self) {
        let total: HashSet<_> = (0..self.size()).collect();
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
                    self.clog_link_both(i as u64, *x as u64)
                }
            });
        }
    }

    /// Make leader not able to connect majority. This function will not change
    /// connections between other nodes.
    async fn partition_leader_and_majority(&self) {
        let leader = self.get_leader_without_term().await;
        (0..self.size())
            .map(|x| x as u64)
            .filter(|x| x != &leader)
            .take(self.majority() - 1)
            .for_each(|x| self.clog_link_both(x, leader))
    }

    /// This is a one-direction partition, make leader could send message to
    /// majority, but cannot receive the message from majority.
    async fn leader_send_to_majority_but_cannot_receive(&self) {
        let leader = self.get_leader_without_term().await;
        (0..self.size())
            .map(|x| x as u64)
            .filter(|x| x != &leader)
            .take(self.majority() - 1)
            .for_each(|x| self.clog_link_single(x, leader))
    }
}

impl<T: NemesisCluster> Nemesis for T {}

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
    async fn test_partition_halves() {
        let cluster = MockCluster::new(5);
        cluster
            .partition_halves((0..=2).collect::<HashSet<_>>())
            .await;
        cluster.assert_eq(vec![vec![1, 2], vec![0, 2], vec![0, 1], vec![4], vec![3]]);

        let cluster = MockCluster::new(6);
        cluster
            .partition_halves((0..=2).collect::<HashSet<_>>())
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
        cluster.partition_majorities_ring().await;
        cluster.assert_eq(vec![vec![1, 3], vec![0, 2], vec![1, 3], vec![0, 2]]);

        let cluster = MockCluster::new(6);
        cluster.partition_majorities_ring().await;
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
        cluster.partition_leader_and_majority().await;
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
        cluster.partition_random_n(3).await;
        assert_eq!(cluster.get_all_reachable_nodes(0).len(), 3);
    }

    #[madsim::test]
    async fn test_leader_send_to_majority_but_cannot_receive() {
        let cluster = MockCluster::new(5);
        cluster.leader_send_to_majority_but_cannot_receive().await;
        let leader = cluster.get_leader_without_term().await;
        let leader_connections_num = (0..cluster.size())
            .map(|x| x as u64)
            .filter(|x| *x != leader)
            .filter(|x| cluster.connected_one_way(*x, leader))
            .count();
        assert!(leader_connections_num < cluster.majority());
    }
}
