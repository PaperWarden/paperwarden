//! Resource bounds for one job (`THREAT_MODEL.md`: bound file size, time and
//! output; `PRODUCT_INVARIANTS.md` 11).
//!
//! The defaults are provisional. They are replaced per operation once engines
//! are measured on target devices.

use std::time::Duration;

const MIB: u64 = 1024 * 1024;

/// Upper bounds a job may not exceed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobLimits {
    max_input_bytes: u64,
    max_output_bytes: u64,
    max_duration: Duration,
}

impl JobLimits {
    /// Creates limits.
    ///
    /// # Errors
    ///
    /// Returns an error if any limit is zero, which would make every job fail.
    pub fn new(
        max_input_bytes: u64,
        max_output_bytes: u64,
        max_duration: Duration,
    ) -> Result<Self, &'static str> {
        if max_input_bytes == 0 || max_output_bytes == 0 || max_duration.is_zero() {
            return Err("job limits must be greater than zero");
        }
        Ok(Self {
            max_input_bytes,
            max_output_bytes,
            max_duration,
        })
    }

    /// The largest input, in bytes, a job may read.
    pub fn max_input_bytes(&self) -> u64 {
        self.max_input_bytes
    }

    /// The largest output, in bytes, a job may write.
    pub fn max_output_bytes(&self) -> u64 {
        self.max_output_bytes
    }

    /// The longest a job may run before it is stopped at its next checkpoint.
    pub fn max_duration(&self) -> Duration {
        self.max_duration
    }
}

impl Default for JobLimits {
    /// 256 MiB in, 512 MiB out, 10 minutes. Provisional; see the module docs.
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * MIB,
            max_output_bytes: 512 * MIB,
            max_duration: Duration::from_secs(600),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_limits_are_rejected() {
        let second = Duration::from_secs(1);
        assert!(JobLimits::new(0, 1, second).is_err());
        assert!(JobLimits::new(1, 0, second).is_err());
        assert!(JobLimits::new(1, 1, Duration::ZERO).is_err());
        assert!(JobLimits::new(1, 1, second).is_ok());
    }

    #[test]
    fn defaults_are_positive() {
        let limits = JobLimits::default();
        assert!(limits.max_input_bytes() > 0);
        assert!(limits.max_output_bytes() >= limits.max_input_bytes());
        assert!(!limits.max_duration().is_zero());
    }
}
