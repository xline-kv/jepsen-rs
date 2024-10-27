pub mod elle_rw;
use std::{collections::HashSet, path::PathBuf};

use anyhow::Result;
use default_struct_builder::DefaultBuilder;
use j4rs::{Instance, InvocationArg};
use log::{info, trace};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::{
    ffi::{historify, java_to_string, FromSerde, ToDe},
    history::SerializableHistoryList,
    init_jvm, CljNs, CLOJURE,
};

fn default_out_dir() -> PathBuf {
    PathBuf::from("./out")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SerializableCheckResult {
    #[serde(rename = ":valid?")]
    pub valid: ValidType,
    #[serde(rename = ":anomaly-types", default)]
    pub anomaly_types: Vec<String>,
    #[serde(rename = ":anomalies")]
    pub anomalies: Option<serde_json::Value>,
    #[serde(rename = ":not", default)]
    pub not: HashSet<String>,
    #[serde(rename = ":also-not", default)]
    pub also_not: HashSet<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, DefaultBuilder)]
#[non_exhaustive]
pub struct CheckOption {
    #[builder(into)]
    #[serde(rename = ":consistency-models")]
    consistency_models: Option<Vec<ConsistencyModel>>,
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
                if s == ":unknown" {
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
            ValidType::Unknown => serializer.serialize_str(":unknown"),
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

/// A trait impl to clojure checker.
pub trait Checker {
    /// Get the ns of this checker.
    fn ns(&self) -> &CljNs;
}

/// A trait that execute the check function.
pub trait Check {
    /// Check the history and write history to disk, returns the check result.
    ///
    /// The history will be written to `history.edn` in the output directory.
    fn check<F: Serialize, V: Serialize, ERR: Serialize>(
        &self,
        history: &SerializableHistoryList<F, V, ERR>,
        option: CheckOption,
    ) -> Result<SerializableCheckResult>;
}

/// Impl Check for all checker.
impl<T: Checker> Check for T {
    fn check<F: Serialize, V: Serialize, ERR: Serialize>(
        &self,
        history: &SerializableHistoryList<F, V, ERR>,
        option: CheckOption,
    ) -> Result<SerializableCheckResult> {
        init_jvm();
        let h = historify(Instance::from_ser(history)?)?;
        trace!("historify done");
        info!("check with option: {:?}", &option);

        // save history to output path.
        // Because the clojure invoke macros needs owned Instances, but the history
        // Instance needs to be used twice (write to disk and check), we can only invoke
        // clojure ns manually with [`InvocationArg`].

        let output = option.directory.join("history.edn");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        let h_arg = [InvocationArg::from(h)];
        let s = CLOJURE.var("pr-str")?.invoke(&h_arg)?;
        std::fs::write(&output, java_to_string(&s)?.replace("{:", "\n{:"))?;
        info!("history saved to `{}`", output.display());

        // check
        let op_clj = InvocationArg::from(Instance::from_ser(option)?);
        let res = self
            .ns()
            .var("check")?
            .invoke(&[op_clj, h_arg.into_iter().next().unwrap()])?;
        trace!("check done");
        res.to_de::<SerializableCheckResult>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::{read_edn, ToDe};

    #[test]
    fn test_deser_from_json_result_should_be_ok() -> anyhow::Result<()> {
        let edn = include_str!("../../assets/check_result.edn");
        let ins = read_edn(edn)?;
        let _res: SerializableCheckResult = ins.to_de()?;
        Ok(())
    }

    #[test]
    fn test_check_option_serialization() {
        let option = CheckOption::default()
            .analyzer("wr-graph")
            .consistency_models([ConsistencyModel::CursorStability]);
        let json = serde_json::to_string(&option).unwrap();
        assert_eq!(
            r#"{":consistency-models":[":cursor-stability"],":directory":"./out",":analyzer":"wr-graph"}"#,
            json
        );
    }

    #[test]
    fn test_check_result_deserialization() -> anyhow::Result<()> {
        let result = r#"{":valid?":":unknown",":anomaly-types":[":empty-transaction-graph"],":anomalies":{":empty-transaction-graph":true},":not":[],":also-not":[]}"#;
        let res: SerializableCheckResult = serde_json::from_str(result)?;
        assert!(matches!(res.valid, ValidType::Unknown));
        Ok(())
    }
}
