use std::collections::VecDeque;

use super::NemesisRecord;

/// The strategy to register and recover nemesis. When a nemesis is executed, it
/// should be put into nemesis register, and at one time, it will be removed
/// from register and resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NemesisRegisterStrategy {
    /// Use a FIFO queue to store and recover the nemesis. `usize` indicates the
    /// maximum size of the queue. when pushing a nemesis into a full queue, the
    /// front nemesis will be dropped, aka. recover. when pushing a nemesis into
    /// a non-full queue, no recover will happen.
    FIFO(usize),

    /// A random queue to store and recover the nemesis. `usize` indicates the
    /// maximum size of the queue. when pushing a nemesis into a full queue, a
    /// random nemesis will be dropped, aka. recover. when pushing a nemesis
    /// into a non-full queue, no recover will happen.
    RandomQueue(usize),
}

impl Default for NemesisRegisterStrategy {
    fn default() -> Self {
        Self::FIFO(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NemesisRegister {
    queue: VecDeque<NemesisRecord>,
    strategy: NemesisRegisterStrategy,
}

impl NemesisRegister {
    /// Create a new nemesis register
    pub fn new(strategy: NemesisRegisterStrategy) -> Self {
        Self {
            queue: VecDeque::new(),
            strategy,
        }
    }

    /// Set the strategy of the nemesis register
    pub fn with_strategy(mut self, strategy: NemesisRegisterStrategy) -> Self {
        self.strategy = strategy;
        self
    }
}
