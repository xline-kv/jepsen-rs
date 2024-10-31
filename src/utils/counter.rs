#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counter {
    cur: usize,
    total: usize,
}

impl Counter {
    #[inline]
    pub fn new(total: usize) -> Self {
        Self { cur: total, total }
    }

    #[inline]
    pub fn set(&mut self, total: usize) {
        self.total = total;
        self.cur = total;
    }

    #[inline]
    pub fn get_cur(&self) -> usize {
        self.cur
    }

    #[inline]
    pub fn get_total(&self) -> usize {
        self.total
    }

    #[inline]
    pub fn count(&mut self) -> Result<usize, &'static str> {
        if self.cur == 0 {
            Err("counter over")
        } else {
            self.cur -= 1;
            Ok(self.cur)
        }
    }

    #[inline]
    pub fn over(&self) -> bool {
        self.cur == 0
    }

    #[inline]
    pub fn reset(&mut self) {
        self.cur = self.total;
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let mut c = Counter::new(3);
        assert!(!c.over());
        _ = c.count();
        assert_eq!(c.get_cur(), 2);
        assert!(!c.over());
        _ = c.count();
        _ = c.count();
        assert!(c.over());
        assert_eq!(c.count().unwrap_err(), "counter over");
        c.reset();
        assert_eq!(c.get_cur(), 3);
    }
}
