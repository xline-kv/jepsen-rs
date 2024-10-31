#[cfg(test)]
mod test {
    use std::sync::Arc;

    use tap::Tap as _;

    use crate::{
        generator::{
            controller::GeneratorGroupStrategy, raw_gen::TestOpGen, DelayAsyncIter, Generator,
            GeneratorBuilder, GeneratorGroup, Global,
        },
        nemesis::NemesisType,
        op::{nemesis::OpOrNemesis, Op},
    };

    #[madsim::test]
    async fn test_nemesis_and_op_generator_intergration() {
        let global = Arc::new(Global::<_, String>::new(TestOpGen::default()));
        let gen = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(2)))
            .build();
        let nemesis = Generator::once(global.clone(), NemesisType::SplitOne(1).into());
        let group =
            GeneratorGroup::new([gen, nemesis]).with_strategy(GeneratorGroupStrategy::Chain);
        let res = group.collect().await;
        assert_eq!(
            res,
            [
                Op::Write(1, 1),
                Op::Txn(vec![Op::Read(1, Some(1)), Op::Write(1, 1)])
            ]
            .map(OpOrNemesis::Op)
            .to_vec()
            .tap_mut(|ops| ops.push(NemesisType::SplitOne(1).into()))
        );
    }
}
