//! This module provides the nemesis framework for Chaos testing.
//! This file contains the type definitions for the nemesis framework.
//! They have the relation like this:
//!
//!    OpOrNemesis
//!    /        \
//!   Op   AllNemesis
//!            /    \
//!  NemesisType     \
//!               NemesisRecord

pub mod implementation;
pub mod register;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub type ServerId = u64;
/// Record the link that has been clogged.
///
/// A Net nemesis should return a [`NetRecord`], indicating the clogged links.
/// This record will be used in [`NemesisRegister`] to resume the nemesis.
pub type NetRecord = HashMap<ServerId, HashSet<ServerId>>;

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SerializableNemesisType {
    BitflipWal,
    BitflipSnap,
    TruncateWal,
    Pause,
    Kill,
    Partition,
    Clock,
    // Following: Recovery types
    /// Recovery from Partition
    Join,
    /// Recovery from Stop
    Start,
    /// Recovery from Kill
    Resume,
}

impl From<&NemesisType> for SerializableNemesisType {
    fn from(nemesis_type: &NemesisType) -> Self {
        match nemesis_type {
            NemesisType::Noop => unreachable!("Noop will not be recorded to history"),
            NemesisType::Kill(_) => SerializableNemesisType::Kill,
            NemesisType::Pause(_) => SerializableNemesisType::Pause,
            NemesisType::SplitOne(_)
            | NemesisType::PartitionHalves(_)
            | NemesisType::PartitionRandomN(_)
            | NemesisType::PartitionMajoritiesRing
            | NemesisType::PartitionLeaderAndMajority
            | NemesisType::LeaderSendToMajorityButCannotReceive => {
                SerializableNemesisType::Partition
            }
        }
    }
}

impl From<&NemesisRecord> for SerializableNemesisType {
    fn from(nemesis_record: &NemesisRecord) -> Self {
        match nemesis_record {
            NemesisRecord::Noop => unreachable!("Noop will not be recorded to history"),
            NemesisRecord::Kill(_) => SerializableNemesisType::Resume,
            NemesisRecord::Pause(_) => SerializableNemesisType::Start,
            NemesisRecord::Net(_) => SerializableNemesisType::Join,
        }
    }
}

impl From<&AllNemesis> for SerializableNemesisType {
    fn from(nemesis_gen: &AllNemesis) -> Self {
        match nemesis_gen {
            AllNemesis::Execute(nemesis_type) => nemesis_type.into(),
            AllNemesis::Recover(nemesis_record) => nemesis_record.into(),
        }
    }
}

/// The enum of input Nemeses instruction.
///
/// It should be convert to [`NemesisRecord`] to execute.
#[non_exhaustive]
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// This enum is to record nemesis operation. It has all infomation of what a
/// nemesis will do.
///
/// A single [`NemesisRecord`] do not have an intention, the cluster should be
/// able to execute or resume each nemesis by one nemesis record.
#[non_exhaustive]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum NemesisRecord {
    /// do nothing.
    ///
    /// No-op will not be recorded to history.
    #[default]
    Noop,
    /// kill the servers in the cluster.
    Kill(HashSet<ServerId>),
    /// pause the servers in the cluster.
    Pause(HashSet<ServerId>),
    /// To record the link that will be clogged.
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

/// A Union type of [`NemesisType`] and [`NemesisRecord`]. Nemesis Generator
/// will generate this.
#[derive(Debug, Clone, PartialEq)]
pub enum AllNemesis {
    Execute(NemesisType),
    Recover(NemesisRecord),
}

impl From<NemesisType> for AllNemesis {
    fn from(nemesis_type: NemesisType) -> Self {
        Self::Execute(nemesis_type)
    }
}

impl From<NemesisRecord> for AllNemesis {
    fn from(nemesis_record: NemesisRecord) -> Self {
        Self::Recover(nemesis_record)
    }
}

impl From<AllNemesis> for String {
    fn from(val: AllNemesis) -> Self {
        match val {
            AllNemesis::Execute(nemesis_type) => {
                format!(
                    "Execute: {}",
                    serde_json::to_string(&nemesis_type)
                        .expect("Serialize NemesisType to json failed")
                )
            }
            AllNemesis::Recover(nemesis_record) => {
                format!(
                    "Recover: {}",
                    serde_json::to_string(&nemesis_record)
                        .expect("Serialize NemesisRecord to json failed")
                )
            }
        }
    }
}
