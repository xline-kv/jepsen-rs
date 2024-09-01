pub mod elle_rw;
use std::path::PathBuf;

use anyhow::Result;
use default_struct_builder::DefaultBuilder;
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

#[derive(Debug, Clone, Serialize, Deserialize, DefaultBuilder)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct CheckOption {
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    consistency_models: Option<ConsistencyModel>,
    #[serde(default = "default_out_dir")]
    directory: PathBuf,
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    anomalies: Option<Vec<String>>,
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    analyzer: Option<String>,
}

impl Default for CheckOption {
    fn default() -> Self {
        Self {
            consistency_models: None,
            directory: default_out_dir(),
            anomalies: None,
            analyzer: None,
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
        option: CheckOption,
    ) -> Result<SerializableCheckResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deser_from_json_result() -> anyhow::Result<()> {
        let json = include_str!("../../assets/check_result.json");
        let _res: SerializableCheckResult = serde_json::from_str(json)?;
        Ok(())
    }

    #[test]
    fn test_check_option_serialization() {
        let option = CheckOption::default()
            .analyzer("wr-graph")
            .consistency_models(ConsistencyModel::CursorStability);
        let json = serde_json::to_string(&option).unwrap();
        assert_eq!(
            r#"{"consistency-models":"cursor-stability","directory":"./out","analyzer":"wr-graph"}"#,
            json
        );
    }
}
