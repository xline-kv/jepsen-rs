use std::fmt;

use super::GeneratorGroup;

/// This trait is for the nemesis generator, which will only
/// generate nemesis pairly.
pub trait NemesisGenerator {
    type Item;
    fn gen(&mut self) -> Self::Item;
}

/// Nemesis generator group is a `GeneratorGroup` that can insert nemesis while
/// generating.
struct NemesisGeneratorGroup<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> {
    gen_group: GeneratorGroup<'a, U, ERR>,
}
