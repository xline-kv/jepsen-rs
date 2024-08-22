use serde::{Deserialize, Serialize};

/// This struct is used to serialize the final history structure to json, and
/// parse to Clojure's history data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableHistory {
    #[serde(rename = ":type")]
    pub type_: String,
}
