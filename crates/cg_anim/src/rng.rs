//! Small, repeatable randomness for particles and camera shake.

/// A PCG-32 generator: random enough for effects, and the same sequence for the same seed
/// on every target.
#[derive(Debug, Clone)]
pub(crate) struct Rng {
    state: u64,
    increment: u64,
}

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed ^ 0x853c_49e6_748f_ea9b);
        rng.next_u32();
        rng
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.increment);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }

    /// Uniform in `0.0..1.0`.
    pub(crate) fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform between `min` and `max` (either order).
    pub(crate) fn range(&mut self, (min, max): (f32, f32)) -> f32 {
        min + (max - min) * self.unit()
    }
}

/// Smooth noise in -1..=1 along `x`, different for each `seed`: random values at whole
/// numbers, eased in between.
pub(crate) fn noise(seed: u32, x: f32) -> f32 {
    let cell = x.floor();
    let t = x - cell;
    let smooth = t * t * (3.0 - 2.0 * t);
    let a = hash(seed, cell as i32);
    let b = hash(seed, cell as i32 + 1);
    a + (b - a) * smooth
}

/// A random value in -1..=1 for each whole number.
fn hash(seed: u32, i: i32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    h as f32 / u32::MAX as f32 * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut c = Rng::new(8);
        let first: Vec<u32> = (0..8).map(|_| a.next_u32()).collect();
        assert_eq!(first, (0..8).map(|_| b.next_u32()).collect::<Vec<_>>());
        assert_ne!(first, (0..8).map(|_| c.next_u32()).collect::<Vec<_>>());
    }

    #[test]
    fn unit_values_spread_over_zero_to_one() {
        let mut rng = Rng::new(1);
        let values: Vec<f32> = (0..10_000).map(|_| rng.unit()).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
        assert!(values.iter().any(|&v| v < 0.01) && values.iter().any(|&v| v > 0.99));
    }

    #[test]
    fn noise_is_continuous_and_bounded() {
        let mut last = noise(3, 0.0);
        for i in 1..1000 {
            let value = noise(3, i as f32 * 0.01);
            assert!((-1.0..=1.0).contains(&value));
            assert!((value - last).abs() < 0.05, "jumps at {i}");
            last = value;
        }
        assert_ne!(noise(1, 0.5), noise(2, 0.5));
    }
}
