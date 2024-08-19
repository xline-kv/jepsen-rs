use std::fmt::Display;

#[derive(Debug, Clone)]
pub enum Op {
    Read(u64),
    Write(u64, u64),
    Txn(Vec<Op>),
}

impl Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        match self {
            Op::Read(v) => write!(f, ":r {} nil", v)?,
            Op::Write(k, v) => write!(f, ":w {} {}", k, v)?,
            Op::Txn(v) => {
                for (index, op) in v.iter().enumerate() {
                    if index != 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", op)?;
                }
            }
        }
        write!(f, "]")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_op_to_string() {
        assert_eq!(Op::Read(1).to_string(), "[:r 1 nil]".to_string());
        assert_eq!(Op::Write(1, 2).to_string(), "[:w 1 2]".to_string());
        assert_eq!(
            Op::Txn(vec![Op::Read(1), Op::Write(2, 3)]).to_string(),
            "[[:r 1 nil] [:w 2 3]]".to_string()
        );
    }
}
