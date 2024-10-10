pub mod counter;
pub mod iter;
use std::ops::Range;

pub use counter::*;
pub use iter::*;

pub trait OverflowingAddRange {
    type Output;
    fn overflowing_add_range(&self, num: Self::Output, range: Range<Self::Output>) -> Self::Output;
}

impl OverflowingAddRange for usize {
    type Output = usize;
    /// Returns `num` + `self` and ensures that the result is always in `range`.
    /// ```
    /// use jepsen_rs::utils::OverflowingAddRange;
    /// assert_eq!(2, 1.overflowing_add_range(4, 1..4));  // 1 + 4 = 5, 5 is not in range [1, 4).
    ///                                                   // So fold 4 to 1, 5 to 2, the returned value is 2.
    /// assert_eq!(1, 1.overflowing_add_range(9, 1..4));
    /// assert_eq!(1, 114514.overflowing_add_range(1, 1..4));
    /// ```
    fn overflowing_add_range(&self, num: Self::Output, range: Range<Self::Output>) -> Self::Output {
        if !range.contains(self) {
            let next = range.start;
            return next.overflowing_add_range(num.saturating_sub(1), range);
        }
        let (mut new, _) = self.overflowing_add(num);
        if !range.contains(&new) {
            new = (new - range.end) % range.len() + range.start;
        }
        assert!(range.contains(&new));
        new
    }
}

#[cfg(test)]
pub fn log_init() {
    use log::LevelFilter;

    _ = pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_millis()
        .filter_module("j4rs", LevelFilter::Info)
        .parse_default_env()
        .try_init();
}

/// Generate a vector of `n` numbers from 0 to `right_endpoint - 1`. The `0` and
/// `right_endpoint - 1` themselves must be included in the returned vector, and
/// the other numbers will try to be equally spaced.
pub fn select_numbers_from_range(mut right_endpoint: usize, n: usize) -> Vec<usize> {
    assert!(n >= 2, "n must be at least 2 to include both endpoints.");
    right_endpoint -= 1;
    let total_steps = n - 1;
    let step_size = right_endpoint / total_steps;
    let mut ret: Vec<_> = (0..=total_steps).map(|i| i * step_size).collect();
    *ret.last_mut().unwrap() = right_endpoint;
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_numbers_from_range() {
        assert_eq!(select_numbers_from_range(3, 2), vec![0, 2]);
        assert_eq!(select_numbers_from_range(3, 3), vec![0, 1, 2]);
        assert_eq!(select_numbers_from_range(7, 3), vec![0, 3, 6]);
        assert_eq!(select_numbers_from_range(7, 4), vec![0, 2, 4, 6]);
        assert_eq!(select_numbers_from_range(7, 5), vec![0, 1, 2, 3, 6]);
    }
}
