#[derive(Default)]
pub struct Stats {
    x0s: usize,
    x1s: f64,
    x2s: f64,
}

impl Stats {
    pub fn new() -> Stats {
        Default::default()
    }

    pub fn push(&mut self, x: f64) {
        self.x0s += 1;
        self.x1s += x;
        self.x2s += x * x;
    }

    pub fn mean(&self) -> f64 {
        self.x1s / self.x0s as f64
    }

    pub fn population_stddev(&self) -> f64 {
        (self.x0s as f64 * self.x2s - self.x1s * self.x1s).sqrt() / self.x0s as f64
    }
}
