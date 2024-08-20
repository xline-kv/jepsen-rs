use std::fmt::Display;

use anyhow::{anyhow, Result};
use j4rs::Instance;
use serde_json::{json, Value};

use crate::utils::JsonSerde;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Read(u64),
    Write(u64, u64),
    Txn(Vec<Op>),
}

/// Parse an `Op` from JSON
fn parse_op(json: &Value) -> Result<Op> {
    match json {
        Value::Array(arr) => {
            // If the first value is a string, it must not be a Txn, whose first element is
            // Vec
            if let Some(op_type) = arr[0].as_str() {
                // Handle Read or Write
                let key = arr[1].as_u64().ok_or(anyhow!("Invalid key"))?;
                match op_type {
                    "r" => Ok(Op::Read(key)),
                    "w" => {
                        let value = arr[2].as_u64().ok_or(anyhow!("Invalid value"))?;
                        Ok(Op::Write(key, value))
                    }
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

/// Convert an `Op` to JSON
fn op_to_json(op: &Op) -> Value {
    match op {
        Op::Read(key) => json!(["r", key, Value::Null]),
        Op::Write(key, value) => json!(["w", key, value]),
        Op::Txn(ops) => {
            let json_ops: Vec<Value> = ops.iter().map(op_to_json).collect();
            Value::Array(json_ops)
        }
    }
}

impl JsonSerde for Vec<Op> {
    fn de(s: &str) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let json: Value = serde_json::from_str(s)?;
        if let Value::Array(arr) = json {
            Ok(arr.iter().map(parse_op).collect::<Result<Vec<_>, _>>()?)
        } else {
            Err(anyhow!("Expected top-level JSON array"))
        }
    }
    /// serialize an `Op` to JSON cannot return an error, so we can use `unwrap`
    /// on it safely.
    fn ser(self) -> anyhow::Result<String> {
        let json_ops: Vec<Value> = self.iter().map(op_to_json).collect();
        Ok(Value::Array(json_ops).to_string())
    }
}

impl Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", op_to_json(self))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_op_serde() {
        let json_str = r#"
        [[["w",6,1],["w",8,1]],[["w",9,1],["r",8,null]],[["w",6,2],["r",6,null]],[["w",9,2]],[["r",8,null],["w",9,3]]]
        "#;

        let ops = vec![
            Op::Txn(vec![Op::Write(6, 1), Op::Write(8, 1)]),
            Op::Txn(vec![Op::Write(9, 1), Op::Read(8)]),
            Op::Txn(vec![Op::Write(6, 2), Op::Read(6)]),
            Op::Txn(vec![Op::Write(9, 2)]),
            Op::Txn(vec![Op::Read(8), Op::Write(9, 3)]),
        ];

        assert_eq!(ops.clone().ser().unwrap().trim(), json_str.trim());
        assert_eq!(Vec::<Op>::de(json_str).unwrap(), ops);
    }
}
