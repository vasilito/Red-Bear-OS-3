use std::time::{Duration, Instant};

/// Represents an amount of time for a driver to give up to the OS scheduler.
pub struct Timeout {
    instant: Instant,
    duration: Duration,
}

impl Timeout {
    /// Create a new `Timeout` from a `Duration`.
    #[inline]
    pub fn new(duration: Duration) -> Self {
        Self {
            instant: Instant::now(),
            duration,
        }
    }

    /// Create a new `Timeout` by specifying the amount of microseconds.
    #[inline]
    pub fn from_micros(micros: u64) -> Self {
        Self::new(Duration::from_micros(micros))
    }

    /// Create a new `Timeout` by specifying the amount of milliseconds.
    #[inline]
    pub fn from_millis(millis: u64) -> Self {
        Self::new(Duration::from_millis(millis))
    }

    /// Create a new `Timeout` by specifying the amount of seconds.
    #[inline]
    pub fn from_secs(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }

    /// Execute the `Timeout`.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if the duration of the `Timeout` has already elapsed
    /// between creating the `Timeout` and calling this function.
    #[inline]
    pub fn run(&self) -> Result<(), ()> {
        if self.instant.elapsed() < self.duration {
            // Sleeps in Redox are only evaluated on PIT ticks (a few ms), which is not
            // short enough for a reasonably responsive timeout. However, the clock is
            // highly accurate. So, we yield instead of sleep to reduce latency.
            //TODO: allow timeout that spins instead of yields?
            std::thread::yield_now();
            Ok(())
        } else {
            Err(())
        }
    }
}
