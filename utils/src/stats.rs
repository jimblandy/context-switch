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

impl Extend<f64> for Stats {
    fn extend<T: IntoIterator<Item=f64>>(&mut self, iter: T) {
        iter.into_iter().for_each(|x| self.push(x));
    }
}

#[test]
fn pop_stddev() {
    let mut stats = Stats::new();

    stats.extend([2.0_f64, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0].iter().copied());
    assert_eq!(stats.mean(), 5.0);
    assert_eq!(stats.population_stddev(), 2.0);
}
