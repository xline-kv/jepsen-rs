use std::ops::Range;

use madsim::{
    rand::{self, Rng},
    time::{self, Duration},
};

use crate::utils::OverflowingAddRange;

/// The delay strategy of the generator. You can delay the generator when
/// every `Op` generation.
#[derive(Debug, Clone, Default)]
pub enum DelayStrategy {
    /// No delay.
    #[default]
    None,
    /// Delay for a fixed time.
    Fixed(Duration),
    /// Delay for a random time, and the time is between `0` and `2 * Duration`.
    Random(Duration),
    // Exponential(u64),
}

impl DelayStrategy {
    /// Delay for a period of time.
    pub async fn delay(&self) {
        match self {
            DelayStrategy::None => {}
            DelayStrategy::Fixed(t) => {
                time::sleep(*t).await;
            }
            DelayStrategy::Random(dt) => {
                let dt = dt.as_millis() as u64;
                let t = rand::thread_rng().gen_range(0..=2 * dt);
                time::sleep(Duration::from_millis(t)).await;
            }
        }
    }
}

/// The strategy of the generator group Scheduling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratorGroupStrategy {
    /// Round robin algorithm, the `usize` is the number of the last generated
    /// number.
    RoundRobin(usize),
    /// Randomly select a generator.
    Random,
    /// The generator group will be scheduled in a chain.
    Chain,
}

impl Default for GeneratorGroupStrategy {
    fn default() -> Self {
        Self::RoundRobin(0)
    }
}

impl GeneratorGroupStrategy {
    pub fn choose(&mut self, range: Range<usize>) -> usize {
        match self {
            Self::RoundRobin(ref mut last_choose) => {
                *last_choose = last_choose.overflowing_add_range(1, range);
                *last_choose
            }
            Self::Random => rand::thread_rng().gen_range(range),
            Self::Chain => range.start,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_group_choose_test() {
        let range = 0..3;
        let mut gs = GeneratorGroupStrategy::RoundRobin(usize::MAX);
        assert_eq!(gs.choose(range.clone()), 0);
        assert_eq!(gs, GeneratorGroupStrategy::RoundRobin(0));
        assert_eq!(gs.choose(range.clone()), 1);
        assert_eq!(gs.choose(range.clone()), 2);
        assert_eq!(gs.choose(range.clone()), 0);
        assert_eq!(gs.choose(range.clone()), 1);
        assert_eq!(gs.choose(range.clone()), 2);
        let mut gs = GeneratorGroupStrategy::Chain;
        assert_eq!(gs.choose(range.clone()), 0);
    }
}
