use std::{
    fmt,
    ops::{Deref, DerefMut},
};

use anyhow::{anyhow, Result};
use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Serialize,
};
use serde_json::{json, Value};

/// An operation that can be executed on a database
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Read(u64, Option<u64>),
    Write(u64, u64),
    Txn(Vec<Op>),
}

/// Op type of functions that being applied to db
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpFunctionType {
    #[serde(rename = "r")]
    Read,
    #[serde(rename = "w")]
    Write,
    Txn,
}

impl From<&Op> for OpFunctionType {
    fn from(op: &Op) -> Self {
        match op {
            Op::Read(_, _) => OpFunctionType::Read,
            Op::Write(_, _) => OpFunctionType::Write,
            Op::Txn(_) => OpFunctionType::Txn,
        }
    }
}

/// A list of [`Op`]s
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ops(pub Vec<Op>);

impl Deref for Ops {
    type Target = Vec<Op>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Ops {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Ops {
    /// Reverse the order of the ops
    pub fn rev(self) -> Self {
        Self(self.0.into_iter().rev().collect())
    }
}

// Serialize and Deserialize

/// Parse an [`Op`] from JSON
fn parse_op(json: &Value) -> Result<Op> {
    match json {
        Value::Array(arr) => {
            // If the first value is a string, it must not be a Txn, whose first element is
            // Vec
            if let Some(op_type) = arr[0].as_str() {
                // Handle Read or Write
                let key = arr[1].as_u64().ok_or(anyhow!("Invalid key"))?;
                let value = arr[2].as_u64();
                match op_type {
                    "r" => Ok(Op::Read(key, value)),
                    "w" => Ok(Op::Write(key, value.ok_or(anyhow!("Invalid value"))?)),
                    _ => Err(anyhow!("Unknown op type")),
                }
            } else {
                // Handle Txn
                let ops = arr.iter().map(parse_op).collect::<Result<Vec<_>, _>>()?;
                Ok(Op::Txn(ops))
            }
        }
        _ => Err(anyhow!("Invalid JSON format")),
    }
}

/// Convert an [`Op`] to JSON
fn op_to_json(op: &Op) -> Value {
    match op {
        Op::Read(key, value) => json!(["r", key, value]),
        Op::Write(key, value) => json!(["w", key, value]),
        Op::Txn(ops) => {
            let json_ops: Vec<Value> = ops.iter().map(op_to_json).collect();
            Value::Array(json_ops)
        }
    }
}

impl Serialize for Op {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let json_value = op_to_json(self);
        json_value.serialize(serializer)
    }
}

/// Temp Struct for [`Op`] deserialization
struct OpVisitor;
impl<'de> Visitor<'de> for OpVisitor {
    type Value = Op;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JSON representation of an Op")
    }
    fn visit_seq<A>(self, mut seq: A) -> Result<Op, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut extract_arr: Vec<Value> = vec![];
        while let Some(value) = seq.next_element()? {
            extract_arr.push(value);
        }
        parse_op(&serde_json::Value::Array(extract_arr)).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Op {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(OpVisitor)
    }
}

#[cfg(test)]
mod test {
    use j4rs::Instance;

    use super::*;
    use crate::ffi::{FromSerde, ToDe};

    #[test]
    fn test_op_serde() {
        let res = [
            (r#"["w",6,1]"#, Op::Write(6, 1)),
            (r#"["r",8,null]"#, Op::Read(8, None)),
            (
                r#"[["w",6,1],["r",8,null]]"#,
                Op::Txn(vec![Op::Write(6, 1), Op::Read(8, None)]),
            ),
        ];
        for (json_str, op) in res {
            assert_eq!(serde_json::to_string(&op).unwrap().trim(), json_str.trim());
            assert_eq!(serde_json::from_str::<Op>(json_str).unwrap(), op);
        }
    }

    #[test]
    fn test_ops_serde() {
        let json_str = r#"
        [[["w",6,1],["w",8,1]],[["w",9,1],["r",8,null]],[["w",6,2],["r",6,null]],[["w",9,2]],[["r",8,null],["w",9,3]]]
        "#;

        let ops = Ops(vec![
            Op::Txn(vec![Op::Write(6, 1), Op::Write(8, 1)]),
            Op::Txn(vec![Op::Write(9, 1), Op::Read(8, None)]),
            Op::Txn(vec![Op::Write(6, 2), Op::Read(6, None)]),
            Op::Txn(vec![Op::Write(9, 2)]),
            Op::Txn(vec![Op::Read(8, None), Op::Write(9, 3)]),
        ]);

        assert_eq!(serde_json::to_string(&ops).unwrap().trim(), json_str.trim());
        assert_eq!(serde_json::from_str::<Ops>(json_str).unwrap(), ops);
    }

    #[test]
    fn test_convertion_between_ops_and_instance() {
        let ops = Ops(vec![
            Op::Txn(vec![Op::Write(6, 1), Op::Write(8, 1)]),
            Op::Txn(vec![Op::Write(9, 1), Op::Read(8, None)]),
        ]);

        let inst: Instance = Instance::from_ser(ops.clone()).unwrap();
        let res: Ops = inst.to_de().unwrap();
        assert_eq!(ops, res);
    }
}
