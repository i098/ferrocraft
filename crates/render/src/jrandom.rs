//! `java.util.Random`: the 48-bit LCG that vanilla seeds per star field, rain column and lightning bolt.

pub struct JavaRandom(i64);

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        JavaRandom((seed ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i32
    }

    /// `nextInt(bound)` for `bound > 0`.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        if bound.count_ones() == 1 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                return val;
            }
        }
    }

    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    pub fn next_long(&mut self) -> i64 {
        (i64::from(self.next(32)) << 32).wrapping_add(i64::from(self.next(32)))
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub fn next_double(&mut self) -> f64 {
        (((self.next(26) as i64) << 27) + self.next(27) as i64) as f64 / (1u64 << 53) as f64
    }

    /// Two consecutive `nextGaussian()` results: the polar method yields a pair, the second is cached by Java.
    pub fn next_gaussian_pair(&mut self) -> (f64, f64) {
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                return (v1 * m, v2 * m);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values of `new Random(42)` on a real JVM.
    #[test]
    fn matches_the_jvm_sequence() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(10), 0);
        assert_eq!(r.next_int(10), 3);
        assert_eq!(r.next_int(100), 48);
        assert_eq!(r.next_int(16), 0);
        assert_eq!(r.next_int(11), 3);
    }
}
