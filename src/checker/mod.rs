pub mod elle_rw;
use std::{default, path::PathBuf};

use anyhow::Result;
use derive_builder::Builder;
use j4rs::{errors::Result as jResult, Instance};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::history::SerializableHistoryList;

fn default_out_dir() -> PathBuf {
    PathBuf::from("./out")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SerializableCheckResult {
    #[serde(rename = "valid?")]
    valid: ValidType,
    anomaly_types: Vec<String>,
    anomalies: serde_json::Value,
    not: Vec<String>,
    also_not: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[serde(rename_all = "kebab-case")]
pub struct CheckOption {
    consistency_models: ConsistencyModel,
    #[serde(default = "default_out_dir")]
    directory: PathBuf,
    anomalies: Vec<String>,
}

impl Default for CheckOption {
    fn default() -> Self {
        Self {
            consistency_models: ConsistencyModel::default(),
            directory: default_out_dir(),
            anomalies: vec![],
        }
    }
}

/// `:valid?` value in `check` result
#[derive(Debug, Clone)]
pub enum ValidType {
    True,
    False,
    Unknown,
}

impl<'de> Deserialize<'de> for ValidType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::Bool(b) => Ok(if b { ValidType::True } else { ValidType::False }),
            Value::String(s) => {
                if s == "unknown" {
                    Ok(ValidType::Unknown)
                } else {
                    Err(serde::de::Error::custom("invalid string value"))
                }
            }
            _ => Err(serde::de::Error::custom("invalid type")),
        }
    }
}

impl Serialize for ValidType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ValidType::True => serializer.serialize_bool(true),
            ValidType::False => serializer.serialize_bool(false),
            ValidType::Unknown => serializer.serialize_str("unknown"),
        }
    }
}

/// canonical-model-names in src/elle/consistency_model.clj
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ConsistencyModel {
    ConsistentView,
    ConflictSerializable,
    CursorStability,
    ForwardConsistentView,
    MonotonicSnapshotRead,
    MonotonicView,
    ReadCommitted,
    ReadUncommitted,
    RepeatableRead,
    #[default]
    Serializable,
    SnapshotIsolation,
    StrictSerializable,
    StrongSerializable,
    UpdateSerializable,
    StrongSessionReadUncommitted,
    StrongSessionReadCommitted,
    StrongReadUncommitted,
    StrongReadCommitted,
}

/// Checker trait
pub trait Check {
    /// The check function, returns a map like `{:valid? true}`
    fn check<F: Serialize, ERR: Serialize>(
        &self,
        history: &SerializableHistoryList<F, ERR>,
        option: Option<CheckOption>,
    ) -> Result<SerializableCheckResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_jvm, nsinvoke, utils::print_clj, CLOJURE};

    #[test]
    fn test_deser_from_json_result() -> anyhow::Result<()> {
        let json = include_str!("../../assets/check_result.json");
        let res: SerializableCheckResult = serde_json::from_str(json)?;
        Ok(())
    }
}
