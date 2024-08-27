use std::default;

use madsim::{rand::Rng, time::Duration};

/// The delay strategy of the generator. You can delay the generator when
/// every `Op` generation.
#[derive(Debug, Clone, Default)]
pub enum DelayStrategy {
    /// No delay.
    #[default]
    None,
    /// Delay for a fixed time.
    Fixed(Duration),
    /// Delay for a random time, and the time is between `0` and `2 * max`.
    Random(Duration),
    // Exponential(u64),
}

impl DelayStrategy {
    pub async fn delay(&self) {
        match self {
            DelayStrategy::None => {}
            DelayStrategy::Fixed(t) => {
                madsim::time::sleep(*t).await;
            }
            DelayStrategy::Random(dt) => {
                let dt = dt.as_millis() as u64;
                let t = madsim::rand::thread_rng().gen_range(0..=2 * dt);
                madsim::time::sleep(Duration::from_millis(t)).await;
            }
        }
    }
}

/// The strategy of the generator group Scheduling
#[derive(Debug, Clone, Default)]
pub enum GeneratorGroupStrategy {
    #[default]
    RoundRobin,
    Random,
    Chain,
}
