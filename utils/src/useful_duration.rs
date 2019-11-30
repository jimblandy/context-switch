use std::fmt;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct UsefulDuration(Duration);

impl From<Duration> for UsefulDuration {
    fn from(d: Duration) -> Self {
        UsefulDuration(d)
    }
}

impl From<f64> for UsefulDuration {
    fn from(secs: f64) -> Self {
        UsefulDuration(Duration::from_secs_f64(secs))
    }
}

impl From<UsefulDuration> for f64 {
    fn from(d: UsefulDuration) -> Self {
        d.0.as_secs_f64()
    }
}

impl fmt::Display for UsefulDuration {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let d = f64::from(*self);
        if d == 0.0 {
            write!(fmt, "0s")
        } else if d < 1.5e-6 {
            write!(fmt, "{:.3}ns", d * 1e9)
        } else if d < 1.5e-3 {
            write!(fmt, "{:.3}µs", d * 1e6)
        } else if d < 1.5 {
            write!(fmt, "{:.3}ms", d * 1e3)
        } else {
            write!(fmt, "{:.3}s", d)
        }
    }
}
