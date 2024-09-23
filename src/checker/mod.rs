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
    #[serde(rename = ":valid?")]
    valid: ValidType,
    #[serde(rename = ":anomaly-types")]
    anomaly_types: Vec<String>,
    #[serde(rename = ":anomalies")]
    anomalies: serde_json::Value,
    #[serde(rename = ":not")]
    not: Vec<String>,
    #[serde(rename = ":also-not")]
    also_not: Vec<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, DefaultBuilder)]
#[non_exhaustive]
pub struct CheckOption {
    #[builder(into)]
    #[serde(rename = ":consistency-models")]
    consistency_models: Option<ConsistencyModel>,
    #[serde(default = "default_out_dir")]
    #[serde(rename = ":directory")]
    directory: PathBuf,
    #[builder(into)]
    #[serde(rename = ":anomalies")]
    anomalies: Option<Vec<String>>,
    #[builder(into)]
    #[serde(rename = ":analyzer")]
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
    #[serde(rename = ":consistent-view")]
    ConsistentView,
    #[serde(rename = ":conflict-serializable")]
    ConflictSerializable,
    #[serde(rename = ":cursor-stability")]
    CursorStability,
    #[serde(rename = ":forward-consistent-view")]
    ForwardConsistentView,
    #[serde(rename = ":monotonic-snapshot-read")]
    MonotonicSnapshotRead,
    #[serde(rename = ":monotonic-view")]
    MonotonicView,
    #[serde(rename = ":read-committed")]
    ReadCommitted,
    #[serde(rename = ":read-uncommitted")]
    ReadUncommitted,
    #[serde(rename = ":repeatable-read")]
    RepeatableRead,
    #[serde(rename = ":serializable")]
    #[default]
    Serializable,
    #[serde(rename = ":snapshot-isolation")]
    SnapshotIsolation,
    #[serde(rename = ":strict-serializable")]
    StrictSerializable,
    #[serde(rename = ":strong-serializable")]
    StrongSerializable,
    #[serde(rename = ":update-serializable")]
    UpdateSerializable,
    #[serde(rename = ":strong-session-read-uncommitted")]
    StrongSessionReadUncommitted,
    #[serde(rename = ":strong-session-read-committed")]
    StrongSessionReadCommitted,
    #[serde(rename = ":strong-read-uncommitted")]
    StrongReadUncommitted,
    #[serde(rename = ":strong-read-committed")]
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
            r#"{":consistency-models":":cursor-stability",":directory":"./out",":analyzer":"wr-graph"}"#,
            json
        );
    }
}
