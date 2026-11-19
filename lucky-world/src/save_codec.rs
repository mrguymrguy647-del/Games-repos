//! How engine types the save file needs are written: the random number
//! generators' exact state and colours (Duck Forge's types have no serde).

/// `duckforge::Rng` keeps its state private. `Rng::new(seed)` stores
/// `scramble(seed)`, and `scramble` is a bijection, so the state is read from
/// its `Debug` text and a seed that scrambles back to it is worked out.
pub mod rng {
    use duckforge::prelude::Rng;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn state(r: &Rng) -> u32 {
        let words = format!("{r:?}");
        let digits: String = words.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.parse().expect("Rng's Debug text holds its state")
    }

    /// The inverse of Duck Forge's `scramble` (x ^= x>>16; x *= 0x7FEB352D;
    /// x ^= x>>15; x *= 0x846CA68B; x ^= x>>16).
    fn unscramble(mut x: u32) -> u32 {
        x ^= x >> 16;
        x = x.wrapping_mul(inverse(0x846C_A68B));
        x ^= x >> 15;
        x ^= x >> 30;
        x = x.wrapping_mul(inverse(0x7FEB_352D));
        x ^= x >> 16;
        x
    }

    /// The multiplicative inverse of an odd number modulo 2^32 (Newton's method).
    fn inverse(a: u32) -> u32 {
        let mut x = a;
        for _ in 0..5 {
            x = x.wrapping_mul(2u32.wrapping_sub(a.wrapping_mul(x)));
        }
        x
    }

    pub fn from_state(state: u32) -> Rng {
        let r = Rng::new(unscramble(state));
        debug_assert_eq!(self::state(&r), state);
        r
    }

    pub fn serialize<S: Serializer>(r: &Rng, s: S) -> Result<S::Ok, S::Error> {
        state(r).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Rng, D::Error> {
        Ok(from_state(u32::deserialize(d)?))
    }
}

pub mod color {
    use duckforge::prelude::Color;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(c: &Color, s: S) -> Result<S::Ok, S::Error> {
        [c.r, c.g, c.b, c.a].serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        let [r, g, b, a] = <[u8; 4]>::deserialize(d)?;
        Ok(Color::rgba(r, g, b, a))
    }
}

pub mod colors {
    use duckforge::prelude::Color;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(c: &[Color], s: S) -> Result<S::Ok, S::Error> {
        c.iter().map(|c| [c.r, c.g, c.b, c.a]).collect::<Vec<_>>().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, C: FromIterator<Color>>(d: D) -> Result<C, D::Error> {
        Ok(Vec::<[u8; 4]>::deserialize(d)?.into_iter().map(|[r, g, b, a]| Color::rgba(r, g, b, a)).collect())
    }
}

pub mod color3 {
    use duckforge::prelude::Color;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(c: &[Color; 3], s: S) -> Result<S::Ok, S::Error> {
        c.map(|c| [c.r, c.g, c.b, c.a]).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[Color; 3], D::Error> {
        Ok(<[[u8; 4]; 3]>::deserialize(d)?.map(|[r, g, b, a]| Color::rgba(r, g, b, a)))
    }
}

/// Keys of the war's counters are `&'static str` in the code; a loaded
/// save's keys are leaked once (there are a few dozen of them).
pub mod static_keys {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S: Serializer, V: Serialize>(m: &BTreeMap<&'static str, V>, s: S) -> Result<S::Ok, S::Error> {
        m.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(d: D) -> Result<BTreeMap<&'static str, V>, D::Error> {
        let m = BTreeMap::<String, V>::deserialize(d)?;
        Ok(m.into_iter().map(|(k, v)| (intern(k), v)).collect())
    }

    fn intern(k: String) -> &'static str {
        use std::sync::Mutex;
        static KNOWN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
        let mut known = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&s) = known.iter().find(|&&s| s == k) {
            return s;
        }
        let s: &'static str = Box::leak(k.into_boxed_str());
        known.push(s);
        s
    }
}
