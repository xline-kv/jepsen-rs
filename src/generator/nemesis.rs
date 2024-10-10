use std::fmt;

use super::Generator;
use crate::{
    nemesis::{NemesisRecord, NemesisType},
    op::Op,
};

/// A union of [`NemesisType`] and [`Op`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NemesisOrOp {
    /// Generate nemesis
    NemesisType(NemesisType),
    /// Recover nemesis
    NemesisRecord(NemesisRecord),
    Op(Op),
}

impl From<NemesisType> for NemesisOrOp {
    fn from(nemesis_type: NemesisType) -> Self {
        Self::NemesisType(nemesis_type)
    }
}

impl From<Op> for NemesisOrOp {
    fn from(op: Op) -> Self {
        Self::Op(op)
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use tap::Tap as _;

    use super::*;
    use crate::{
        generator::{
            controller::GeneratorGroupStrategy, GeneratorBuilder, GeneratorGroup, Global,
            RawGenerator,
        },
        nemesis::NemesisCalculator,
        utils::{DelayAsyncIter, OverflowingAddRange},
    };

    #[derive(Default)]
    struct TestOpGen {
        index: usize,
    }

    /// infinitely generate ops
    impl RawGenerator for TestOpGen {
        type Item = NemesisOrOp;
        fn gen(&mut self) -> Self::Item {
            self.index = self.index.overflowing_add_range(1, 0..3);
            NemesisOrOp::from(match self.index {
                0 => Op::Read(1, Some(1)),
                1 => Op::Write(1, 1),
                2 => Op::Txn(vec![Op::Read(1, Some(1)), Op::Write(1, 1)]),
                _ => unreachable!(),
            })
        }
    }

    #[madsim::test]
    async fn test_nemesis_and_op_generator_intergration() {
        let global = Arc::new(Global::<_, String>::new(TestOpGen::default()));
        let gen = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(2)))
            .build();
        let nemesis = Generator::once(global.clone(), NemesisType::SplitOne(1).into());
        let group = GeneratorGroup::new_from_generator(global.clone(), [gen, nemesis])
            .with_strategy(GeneratorGroupStrategy::Chain);
        let res = group.collect().await;
        assert_eq!(
            res,
            [
                Op::Write(1, 1),
                Op::Txn(vec![Op::Read(1, Some(1)), Op::Write(1, 1)])
            ]
            .map(NemesisOrOp::Op)
            .to_vec()
            .tap_mut(|ops| ops.push(NemesisOrOp::NemesisType(NemesisType::SplitOne(1))))
        );
    }
}
