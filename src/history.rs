use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use madsim::time;
use serde::{Deserialize, Serialize};

use crate::{
    generator::Global,
    op::{Op, OpFunctionType},
};
pub type ErrorType = Vec<String>;

/// This struct is used to serialize the *final* history structure to json, and
/// parse to Clojure's history data structure.
///
/// We only need to serialize the history, but here implements the Deserialize
/// trait as well.
///
/// FIXME: The deserialization in clojure site will ignore the `:` symbol, that
/// causes the unknown check result in checker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerializableHistory<F = OpFunctionType, ERR = ErrorType> {
    #[serde(rename = ":index")]
    pub index: u64,
    #[serde(rename = ":type")]
    pub type_: HistoryType,
    #[serde(rename = ":f")]
    pub f: F,
    #[serde(rename = ":value")]
    pub value: Op,
    #[serde(rename = ":time")]
    pub time: u64,
    #[serde(rename = ":process")]
    pub process: u64,
    #[serde(rename = ":error")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ERR>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryType {
    #[serde(rename = ":invoke")]
    Invoke,
    #[serde(rename = ":ok")]
    Ok,
    #[serde(rename = ":fail")]
    Fail,
    #[serde(rename = ":info")]
    Info,
}

/// A list of Serializable history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableHistoryList<F = OpFunctionType, ERR = ErrorType>(
    pub Vec<SerializableHistory<F, ERR>>,
);

impl<F, ERR> Default for SerializableHistoryList<F, ERR> {
    fn default() -> Self {
        Self(vec![])
    }
}

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

impl<F: PartialEq, ERR: PartialEq> PartialEq for SerializableHistoryList<F, ERR> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<ERR: Send> SerializableHistoryList<OpFunctionType, ERR> {
    /// Get the current timestamp.
    fn timestamp(&self, global: &Arc<Global<Op, ERR>>) -> u64 {
        time::Instant::now()
            .duration_since(global.start_time)
            .as_nanos() as u64
    }
    /// Push an invoke history to the history list.
    pub fn push_invoke(&mut self, global: &Arc<Global<Op, ERR>>, process: u64, value: Op) {
        let f: OpFunctionType = (&value).into();
        let item = SerializableHistory {
            index: self.0.len() as u64,
            type_: HistoryType::Invoke,
            f,
            value,
            time: self.timestamp(global),
            process,
            error: None,
        };
        self.0.push(item);
    }

    /// Push a result to the history list.
    pub fn push_result(
        &mut self,
        global: &Arc<Global<Op, ERR>>,
        process: u64,
        result_type: HistoryType,
        value: Op,
        error: Option<ERR>,
    ) {
        assert!(
            (result_type == HistoryType::Ok) == (error.is_none()),
            "result type mismatch"
        );
        let f: OpFunctionType = (&value).into();
        let item = SerializableHistory {
            index: self.0.len() as u64,
            type_: result_type,
            f,
            value,
            time: self.timestamp(global),
            process,
            error,
        };
        self.0.push(item);
    }
}

#[cfg(test)]
mod tests {
    use j4rs::Instance;

    use super::*;
    use crate::ffi::{
            equals_clj, read_edn,
            FromSerde, ToDe,
        };

    #[test]
    fn test_history_list_conversion() -> anyhow::Result<()> {
        let his_edn = read_edn(include_str!("../assets/ex_history.edn"))?;
        let res: SerializableHistoryList = his_edn.to_de()?;

        // additional test for serialization and deserialization
        let res_from_json: SerializableHistoryList =
            serde_json::from_str(include_str!("../assets/ex_history.json"))?;
        assert_eq!(res, res_from_json);

        let res: Instance = Instance::from_ser(res)?;
        assert!(equals_clj(res, read_edn(include_str!("../assets/ex_history.edn"))?).unwrap());
        Ok(())
    }
}
