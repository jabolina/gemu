//! The process local clock.
//!
//! This is an abstraction over an [`AtomicU64`], exposing only a simple API to be used locally by
//! each process to increase, get and leap it's own local clock for message ordering. For all
//! operations exposed using a [`SeqCst`] so all threads see all operations in the same order.
//!
//! [`AtomicU64`]: std::sync::atomic::AtomicU64
//! [`SeqCst`]: std::sync::atomic::Ordering::SeqCst

use std::sync::atomic;

/// The local clock structure, a wrapper around the [`AtomicU64`].
#[derive(Default)]
struct LocalClock(atomic::AtomicU64);

impl LocalClock {
    /// Creates a new [`LocalClock`].
    ///
    /// All clocks will start with the initial value of 0.
    pub(crate) fn new() -> Self {
        Default::default()
    }

    /// Increments the clock value by 1.
    ///
    /// This operation returns the clock previous value.
    pub(crate) fn inc(&mut self) -> u64 {
        self.0.fetch_add(1, atomic::Ordering::SeqCst)
    }

    /// Returns the clock current value.
    pub(crate) fn get(&self) -> u64 {
        self.0.load(atomic::Ordering::SeqCst)
    }

    /// Leaps the clock current value to the given value.
    pub(crate) fn leap(&mut self, value: u64) {
        self.0.swap(value, atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use crate::internal::clock::LocalClock;

    /// Simple test to verify all operations available.
    #[test]
    fn should_execute_operations() {
        let mut clock = LocalClock::new();

        assert_eq!(clock.inc(), 0);
        assert_eq!(clock.get(), 1);

        clock.leap(10);

        assert_eq!(clock.get(), 10);
    }
}
