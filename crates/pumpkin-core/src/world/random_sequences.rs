use std::collections::HashMap;

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::random::RandomGenerator;
use pumpkin_util::random::RandomImpl;
use pumpkin_util::random::xoroshiro128::Xoroshiro;

/// Vanilla `RandomSupport.upgradeSeedTo128bitUnmixed`.
const fn upgrade_seed_to_128bit_unmixed(seed: i64) -> (u64, u64) {
    let lo = (seed as u64) ^ 0x6A09_E667_F3BC_C909;
    (lo, lo.wrapping_add(0x9E37_79B9_7F4A_7C15))
}

/// Vanilla `RandomSupport.seedFromHashOf`: the MD5 of the string as two big-endian longs.
fn seed_from_hash_of(value: &str) -> (u64, u64) {
    let bytes = md5::compute(value.as_bytes()).0;
    let mut lo = [0; 8];
    let mut hi = [0; 8];
    lo.copy_from_slice(&bytes[..8]);
    hi.copy_from_slice(&bytes[8..]);
    (u64::from_be_bytes(lo), u64::from_be_bytes(hi))
}

/// Vanilla `RandomSequence`: one Xoroshiro source per sequence id.
pub struct RandomSequence {
    rng: RandomGenerator,
}

impl RandomSequence {
    /// Vanilla `RandomSequence(long, Optional<Identifier>)`.
    #[must_use]
    pub fn new(seed: i64, sequence: Option<&str>) -> Self {
        let (mut lo, mut hi) = upgrade_seed_to_128bit_unmixed(seed);
        if let Some(sequence) = sequence {
            let (key_lo, key_hi) = seed_from_hash_of(sequence);
            lo ^= key_lo;
            hi ^= key_hi;
        }
        Self {
            rng: RandomGenerator::Xoroshiro(Xoroshiro::from_seed128_mixed(lo, hi)),
        }
    }

    /// Vanilla `RandomSequence.random()`.
    pub const fn random(&mut self) -> &mut RandomGenerator {
        &mut self.rng
    }

    pub fn random_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            return min;
        }
        self.rng.next_inbetween_i32(min, max)
    }

    fn to_nbt(&self) -> NbtCompound {
        let (lo, hi) = match &self.rng {
            RandomGenerator::Xoroshiro(rng) => rng.state(),
            _ => (0, 0),
        };
        let mut nbt = NbtCompound::new();
        nbt.put("source", NbtTag::LongArray(vec![lo as i64, hi as i64]));
        nbt
    }

    fn from_nbt(nbt: &NbtCompound) -> Option<Self> {
        let Some(NbtTag::LongArray(source)) = nbt.get("source") else {
            return None;
        };
        let [lo, hi] = source.as_slice() else {
            return None;
        };
        Some(Self {
            rng: RandomGenerator::Xoroshiro(Xoroshiro::from_state(*lo as u64, *hi as u64)),
        })
    }
}

/// Vanilla `RandomSequences`, the server's `random_sequences` saved data.
pub struct RandomSequences {
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: HashMap<String, RandomSequence>,
}

impl Default for RandomSequences {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomSequences {
    #[must_use]
    pub fn new() -> Self {
        Self {
            salt: 0,
            include_world_seed: true,
            include_sequence_id: true,
            sequences: HashMap::new(),
        }
    }

    /// Vanilla `createSequence(Identifier, long, int, boolean, boolean)`.
    fn create_sequence(
        sequence: &str,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> RandomSequence {
        let seed = (if include_world_seed { world_seed } else { 0 }) ^ i64::from(salt);
        RandomSequence::new(seed, include_sequence_id.then_some(sequence))
    }

    /// Vanilla `RandomSequences.get`: the sequence, created from the current defaults when missing.
    pub fn get_or_create(&mut self, sequence: &Identifier, world_seed: i64) -> &mut RandomSequence {
        self.get_or_create_key(&sequence.to_string(), world_seed)
    }

    /// [`Self::get_or_create`] with the sequence id as a string.
    pub fn get_or_create_key(&mut self, sequence: &str, world_seed: i64) -> &mut RandomSequence {
        let (salt, include_world_seed, include_sequence_id) =
            (self.salt, self.include_world_seed, self.include_sequence_id);
        self.sequences
            .entry(sequence.to_string())
            .or_insert_with(|| {
                Self::create_sequence(
                    sequence,
                    world_seed,
                    salt,
                    include_world_seed,
                    include_sequence_id,
                )
            })
    }

    pub fn reset(&mut self, sequence: &Identifier, world_seed: i64) {
        self.reset_with_options(
            sequence,
            world_seed,
            self.salt,
            self.include_world_seed,
            self.include_sequence_id,
        );
    }

    pub fn reset_with_options(
        &mut self,
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        let key = sequence.to_string();
        let created = Self::create_sequence(
            &key,
            world_seed,
            salt,
            include_world_seed,
            include_sequence_id,
        );
        self.sequences.insert(key, created);
    }

    pub fn clear(&mut self) -> usize {
        let count = self.sequences.len();
        self.sequences.clear();
        count
    }

    pub const fn set_seed_defaults(
        &mut self,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        self.salt = salt;
        self.include_world_seed = include_world_seed;
        self.include_sequence_id = include_sequence_id;
    }

    #[must_use]
    pub fn get_sequence_keys(&self) -> Vec<String> {
        self.sequences.keys().cloned().collect()
    }

    /// Vanilla `RandomSequences.CODEC`: the `data` compound of `random_sequences.dat`.
    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut sequences = NbtCompound::new();
        let mut keys: Vec<_> = self.sequences.keys().collect();
        keys.sort_unstable();
        for key in keys {
            sequences.put_compound(key, self.sequences[key].to_nbt());
        }
        let mut nbt = NbtCompound::new();
        nbt.put_int("salt", self.salt);
        nbt.put_bool("include_world_seed", self.include_world_seed);
        nbt.put_bool("include_sequence_id", self.include_sequence_id);
        nbt.put_compound("sequences", sequences);
        nbt
    }

    #[must_use]
    pub fn from_nbt(nbt: &NbtCompound) -> Self {
        let mut sequences = HashMap::new();
        if let Some(saved) = nbt.get_compound("sequences") {
            for (key, value) in &saved.child_tags {
                if let NbtTag::Compound(value) = value
                    && let Some(sequence) = RandomSequence::from_nbt(value)
                {
                    sequences.insert(key.to_string(), sequence);
                }
            }
        }
        Self {
            salt: nbt.get_int("salt").unwrap_or(0),
            include_world_seed: nbt.get_bool("include_world_seed").unwrap_or(true),
            include_sequence_id: nbt.get_bool("include_sequence_id").unwrap_or(true),
            sequences,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_seed_matches_vanilla_derivation() {
        // upgradeSeedTo128bitUnmixed(0) xor MD5("minecraft:test"), then mixed.
        let (lo, hi) = upgrade_seed_to_128bit_unmixed(0);
        let (key_lo, key_hi) = seed_from_hash_of("minecraft:test");
        let mut expected = Xoroshiro::from_seed128_mixed(lo ^ key_lo, hi ^ key_hi);
        let mut sequence = RandomSequence::new(0, Some("minecraft:test"));
        assert_eq!(sequence.random().next_i64(), expected.next_i64());
        // Without the id it is the plain `RandomSource.create` Xoroshiro seeding.
        let mut plain = RandomSequence::new(42, None);
        assert_eq!(
            plain.random().next_i64(),
            Xoroshiro::from_seed(42).next_i64()
        );
        // MD5("") is d41d8cd98f00b204e9800998ecf8427e.
        assert_eq!(
            seed_from_hash_of(""),
            (0xd41d_8cd9_8f00_b204, 0xe980_0998_ecf8_427e)
        );
    }

    #[test]
    fn sequences_round_trip_through_nbt() {
        let mut sequences = RandomSequences::new();
        sequences.set_seed_defaults(7, false, true);
        let first = sequences
            .get_or_create_key("minecraft:blocks/stone", 99)
            .random()
            .next_i64();
        let saved = sequences.to_nbt();
        let mut loaded = RandomSequences::from_nbt(&saved);
        let mut fresh = RandomSequences::new();
        fresh.set_seed_defaults(7, false, true);
        let fresh_rng = fresh
            .get_or_create_key("minecraft:blocks/stone", 99)
            .random();
        assert_eq!(fresh_rng.next_i64(), first);
        assert_eq!(
            loaded
                .get_or_create_key("minecraft:blocks/stone", 99)
                .random()
                .next_i64(),
            fresh_rng.next_i64()
        );
        assert_eq!(loaded.salt, 7);
        assert!(!loaded.include_world_seed);
    }
}
