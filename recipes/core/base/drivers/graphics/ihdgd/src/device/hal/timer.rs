use embedded_hal::timer;
use std::time::{Duration, Instant};
use void::Void;

pub struct HalTimer {
    instant: Instant,
    duration: Duration,
}

impl HalTimer {
    pub fn new(duration: Duration) -> Self {
        Self {
            instant: Instant::now(),
            duration,
        }
    }
}

impl timer::CountDown for HalTimer {
    type Time = Duration;
    fn start<T: Into<Duration>>(&mut self, duration: T) {
        self.instant = Instant::now();
        self.duration = duration.into();
    }

    fn wait(&mut self) -> nb::Result<(), Void> {
        if self.instant.elapsed() < self.duration {
            std::thread::yield_now();
            Err(nb::Error::WouldBlock)
        } else {
            // Since this is periodic it must trigger at the next duration
            self.instant += self.duration;
            Ok(())
        }
    }
}

impl timer::Periodic for HalTimer {}
