use std::ops::{Deref, DerefMut};

use j4rs::Instance;
use serde::{Deserialize, Serialize};

use crate::{
    op::Op,
    utils::{clj_from_json, clj_jsonify},
};

type ErrorType = Vec<String>;

/// This struct is used to serialize the final history structure to json, and
/// parse to Clojure's history data structure.
///
/// We only need to serialize the history, but here implements the Deserialize
/// trait as well.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableHistory<F = ElleRwOpFunctionType, ERR = ErrorType> {
    pub index: u64,
    #[serde(rename = "type")]
    pub type_: HistoryType,
    pub f: F,
    pub value: Op,
    pub time: u64,
    pub process: u64,
    pub error: Option<ERR>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryType {
    Invoke,
    Ok,
    Fail,
    Info,
}

/// elle.rw type of functions that being applied to db
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElleRwOpFunctionType {
    #[serde(rename = "r")]
    Read,
    #[serde(rename = "w")]
    Write,
    Txn,
}

/// A list of history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableHistoryList(pub Vec<SerializableHistory>);

impl Deref for SerializableHistoryList {
    type Target = Vec<SerializableHistory>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for SerializableHistoryList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Convertion

impl TryFrom<Instance> for SerializableHistoryList {
    type Error = anyhow::Error;
    fn try_from(value: Instance) -> std::result::Result<Self, Self::Error> {
        Ok(serde_json::from_str(&clj_jsonify(value)?)?)
    }
}
impl TryFrom<SerializableHistoryList> for Instance {
    type Error = anyhow::Error;
    fn try_from(value: SerializableHistoryList) -> std::result::Result<Self, Self::Error> {
        Ok(clj_from_json(&serde_json::to_string(&value)?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_edn, utils::print_clj};

    #[test]
    fn test_history_list_conversion() -> anyhow::Result<()> {
        let his_edn = read_edn(include_str!("../assets/ex_history.edn"))?;
        let res: SerializableHistoryList = his_edn.try_into()?;
        assert_eq!(res.len(), 4);
        let res: Instance = res.try_into()?;
        print_clj(res);
        Ok(())
    }
}
