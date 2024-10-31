use std::collections::VecDeque;

use madsim::rand::Rng;

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

/// A register to store and recover the nemesis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NemesisRegister<T: Clone = NemesisRecord> {
    queue: VecDeque<T>,
    strategy: NemesisRegisterStrategy,
}

impl<T: Clone> NemesisRegister<T> {
    /// Create a new nemesis register
    pub fn new(strategy: NemesisRegisterStrategy) -> Self {
        Self {
            queue: VecDeque::new(),
            strategy,
        }
    }

    /// Set the strategy of the nemesis register and return self.
    #[inline]
    pub fn with_strategy(mut self, strategy: NemesisRegisterStrategy) -> Self {
        self.set_strategy(strategy);
        self
    }

    /// Set the strategy of the nemesis register
    #[inline]
    pub fn set_strategy(&mut self, strategy: NemesisRegisterStrategy) {
        self.strategy = strategy;
    }

    /// Put a [`NemesisRecord`] to the register, and pop a [`NemesisRecord`] if
    /// the queue exceeds its limit.
    ///
    /// # Returns
    ///
    /// Returns a `(Option<NemesisRecord>, Option<NemesisRecord>)`. The first
    /// element is the record that needs to be executed, and the second element
    /// is the record that needs to be recovered.
    ///
    /// Note that in Random mode, the input [`NemesisRecord`] may be same as
    /// output [`NemesisRecord`]. You need to deal with it by your self.
    pub fn put(&mut self, n: T) -> (T, Option<T>) {
        self.queue.push_back(n.clone());
        match self.strategy {
            NemesisRegisterStrategy::FIFO(max_size) => {
                if self.queue.len() <= max_size {
                    return (n, None);
                }
                let front = self.queue.pop_front().unwrap();
                (n, Some(front))
            }
            NemesisRegisterStrategy::RandomQueue(max_size) => {
                if self.queue.len() <= max_size {
                    return (n, None);
                }
                let index = madsim::rand::thread_rng().gen_range(0..self.queue.len());
                let front = self.queue.remove(index).expect("index must be valid");
                (n, Some(front))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[madsim::test]
    async fn test_nemesis_register() {
        let mut register = NemesisRegister::new(NemesisRegisterStrategy::FIFO(2));
        assert_eq!(register.put(1), (1, None));
        assert_eq!(register.put(2), (2, None));
        assert_eq!(register.put(3), (3, Some(1)));
        assert_eq!(register.put(4), (4, Some(2)));

        let mut register = NemesisRegister::new(NemesisRegisterStrategy::RandomQueue(2));
        assert_eq!(register.put(1), (1, None));
        assert_eq!(register.put(2), (2, None));
        assert!(matches!(register.put(3), (3, Some(_))));
        assert!(matches!(register.put(4), (4, Some(_))));
    }
}
