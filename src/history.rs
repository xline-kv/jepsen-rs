use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use madsim::time;
use serde::{Deserialize, Serialize};

use crate::{
    generator::Global,
    nemesis::AllNemesis,
    op::{nemesis::OpOrNemesis, Op, OpFunctionType, OpOrNemesisFuncType},
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
pub struct SerializableHistory<
    F: Serialize = OpOrNemesisFuncType,
    V: Serialize = HistoryValue,
    ERR = ErrorType,
> {
    #[serde(rename = ":index")]
    pub index: u64,
    #[serde(rename = ":type")]
    pub type_: HistoryType,
    #[serde(rename = ":f")]
    pub f: F,
    #[serde(rename = ":value")]
    pub value: V,
    #[serde(rename = ":time")]
    pub time: u64,
    #[serde(rename = ":process")]
    pub process: HistoryProcess,
    #[serde(rename = ":error")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ERR>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum HistoryValue {
    /// A string type is for nemesis discription.
    String(String),
    /// A Op type is for Op generator result.
    Op(Op),
}

impl From<Op> for HistoryValue {
    fn from(op: Op) -> Self {
        Self::Op(op)
    }
}

impl From<String> for HistoryValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HistoryProcess {
    #[serde(rename = ":nemesis")]
    Nemesis,
    #[serde(untagged)]
    Gen(u64),
}

/// A list of Serializable history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableHistoryList<
    F: Serialize = OpFunctionType,
    V: Serialize = HistoryValue,
    ERR = ErrorType,
>(pub Vec<SerializableHistory<F, V, ERR>>);

impl<F: Serialize, V: Serialize, ERR> Default for SerializableHistoryList<F, V, ERR> {
    fn default() -> Self {
        Self(vec![])
    }
}

impl<F: Serialize, V: Serialize, ERR> Deref for SerializableHistoryList<F, V, ERR> {
    type Target = Vec<SerializableHistory<F, V, ERR>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<F: Serialize, V: Serialize, ERR> DerefMut for SerializableHistoryList<F, V, ERR> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<F: PartialEq + Serialize, V: PartialEq + Serialize, ERR: PartialEq> PartialEq
    for SerializableHistoryList<F, V, ERR>
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<ERR: Send> SerializableHistoryList<OpOrNemesisFuncType, HistoryValue, ERR> {
    /// Get the current timestamp.
    fn timestamp(&self, global: &Arc<Global<OpOrNemesis, ERR>>) -> u64 {
        time::Instant::now()
            .duration_since(global.start_time)
            .as_nanos() as u64
    }
    /// Push an invoke history to the history list.
    pub fn push_invoke(&mut self, global: &Arc<Global<OpOrNemesis, ERR>>, process: u64, value: Op) {
        let f: OpFunctionType = (&value).into();
        let f: OpOrNemesisFuncType = f.into();
        let value = value.into();
        let item = SerializableHistory {
            index: self.0.len() as u64,
            type_: HistoryType::Invoke,
            f,
            value,
            time: self.timestamp(global),
            process: HistoryProcess::Gen(process),
            error: None,
        };
        self.0.push(item);
    }

    /// Push a result to the history list.
    pub fn push_result(
        &mut self,
        global: &Arc<Global<OpOrNemesis, ERR>>,
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
        let f: OpOrNemesisFuncType = f.into();
        let item = SerializableHistory {
            index: self.0.len() as u64,
            type_: result_type,
            f,
            value: value.into(),
            time: self.timestamp(global),
            process: HistoryProcess::Gen(process),
            error,
        };
        self.0.push(item);
    }

    /// Push a nemesis to the history list.
    pub fn push_nemesis(&mut self, global: &Arc<Global<OpOrNemesis, ERR>>, value: AllNemesis) {
        let item = SerializableHistory {
            index: self.0.len() as u64,
            type_: HistoryType::Info,
            f: OpOrNemesisFuncType::Nemesis((&value).into()),
            value: HistoryValue::String(value.into()),
            time: self.timestamp(global),
            process: HistoryProcess::Nemesis,
            error: None,
        };
        self.0.push(item);
    }
}

#[cfg(test)]
mod tests {
    use j4rs::Instance;

    use super::*;
    use crate::{
        ffi::{equals_clj, print_clj, read_edn, FromSerde, ToDe},
        generator::raw_gen::TestOpGen,
        nemesis::NemesisType,
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

    #[test]
    fn test_push_op_and_nemesis_to_history_and_conversion() -> anyhow::Result<()> {
        let global: Arc<Global<'_, OpOrNemesis, ErrorType>> =
            Arc::new(Global::new(TestOpGen::default()));
        let nm = AllNemesis::Execute(NemesisType::PartitionHalves([1, 2].into_iter().collect()));
        let mut res = SerializableHistoryList::default();
        res.push_nemesis(&global, nm);
        let his = res.0.first().unwrap();
        assert_eq!(his.type_, HistoryType::Info);
        assert_eq!(
            his.f,
            OpOrNemesisFuncType::Nemesis(crate::nemesis::SerializableNemesisType::Partition)
        );
        assert_eq!(his.process, HistoryProcess::Nemesis);
        assert!(match his.value {
            HistoryValue::String(ref s) => s.starts_with("Execute: {\"PartitionHalves\""),
            _ => false,
        });
        res.push_invoke(&global, 1, Op::Read(1, None));
        let res: Instance = Instance::from_ser(res)?; // test serialization ok
        print_clj(res);
        Ok(())
    }
}
