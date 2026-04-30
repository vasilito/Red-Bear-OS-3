use alloc::string::String;

use crate::device::DeviceInfo;
use crate::params::DriverParams;
use crate::r#match::DriverMatch;

/// Result of a driver probe attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeResult {
    /// The driver successfully bound to the device.
    Bound,
    /// The device is not supported by this driver and other drivers may still try.
    NotSupported,
    /// A dependency is not yet available, so the manager should retry the probe later.
    Deferred {
        /// Human-readable reason for the deferral.
        reason: String,
    },
    /// The device cannot be driven successfully by this driver.
    Fatal {
        /// Human-readable explanation of the failure.
        reason: String,
    },
}

/// Errors returned by driver lifecycle operations after a device has been matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverError {
    /// The operation requires a resource that is not ready yet.
    NotReady,
    /// The driver encountered an I/O failure while managing the device.
    IoError,
    /// The requested lifecycle operation is not supported by this driver.
    Unsupported,
    /// An implementation-specific static error message.
    Other(&'static str),
}

/// A device driver that can bind to and manage devices.
pub trait Driver: Send + Sync {
    /// Returns the unique driver name, such as `"nvmed"` or `"e1000d"`.
    fn name(&self) -> &str;

    /// Returns a human-readable description of the driver.
    fn description(&self) -> &str;

    /// Returns the probe priority for this driver.
    ///
    /// Higher numbers are probed first. Storage drivers typically use higher priorities than
    /// networking or peripheral drivers so boot-critical hardware claims happen early.
    fn priority(&self) -> i32 {
        0
    }

    /// Returns the driver's static match table.
    fn match_table(&self) -> &[DriverMatch];

    /// Probes a candidate device and decides whether the driver should take ownership.
    fn probe(&self, info: &DeviceInfo) -> ProbeResult;

    /// Detaches the driver from a previously bound device.
    fn remove(&self, info: &DeviceInfo) -> Result<(), DriverError>;

    /// Suspends a bound device.
    fn suspend(&self, info: &DeviceInfo) -> Result<(), DriverError> {
        let _ = info;
        Ok(())
    }

    /// Resumes a previously suspended device.
    fn resume(&self, info: &DeviceInfo) -> Result<(), DriverError> {
        let _ = info;
        Ok(())
    }

    /// Returns the driver's parameter definitions and current values.
    fn params(&self) -> DriverParams {
        DriverParams::default()
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use super::ProbeResult;

    #[test]
    fn probe_result_variants_preserve_payloads() {
        let bound = ProbeResult::Bound;
        let not_supported = ProbeResult::NotSupported;
        let deferred = ProbeResult::Deferred {
            reason: String::from("waiting for scheme"),
        };
        let fatal = ProbeResult::Fatal {
            reason: String::from("device is wedged"),
        };

        assert!(matches!(bound, ProbeResult::Bound));
        assert!(matches!(not_supported, ProbeResult::NotSupported));
        assert!(matches!(
            deferred,
            ProbeResult::Deferred { reason } if reason == "waiting for scheme"
        ));
        assert!(matches!(
            fatal,
            ProbeResult::Fatal { reason } if reason == "device is wedged"
        ));
    }
}
