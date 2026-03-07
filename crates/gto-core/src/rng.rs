use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub type DeterministicRng = ChaCha8Rng;

pub const DEFAULT_RNG_SEED: u64 = 0x0ddc_0ffe_e15e_beef;

pub fn rng_from_seed(seed: u64) -> DeterministicRng {
    let mut expanded_seed = [0u8; 32];
    expanded_seed[..8].copy_from_slice(&seed.to_le_bytes());
    DeterministicRng::from_seed(expanded_seed)
}

pub fn default_rng() -> DeterministicRng {
    rng_from_seed(DEFAULT_RNG_SEED)
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use crate::{default_rng, rng_from_seed};

    #[test]
    fn equal_seeds_produce_equal_rng_streams() {
        let mut left = rng_from_seed(42);
        let mut right = rng_from_seed(42);

        assert_eq!(left.random::<u64>(), right.random::<u64>());
        assert_eq!(left.random::<u64>(), right.random::<u64>());
    }

    #[test]
    fn default_rng_is_stable() {
        let mut first = default_rng();
        let mut second = default_rng();

        assert_eq!(first.random::<u64>(), second.random::<u64>());
    }
}
