// Expands a grammar's rules into names, and the small PRNG that drives it.
//
// No dependency is pulled in just for randomness: splitmix64 is a few lines
// and is more than enough entropy quality for picking among a handful of
// alternatives.

use crate::grammar::{Grammar, Part};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DEPTH: usize = 100;

pub struct Rng(u64);

impl Rng {
    pub fn new_seeded(seed: u64) -> Self {
        // splitmix64 stalls if the state is ever exactly zero.
        Rng(seed | 1)
    }

    pub fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        let pid = std::process::id() as u64;
        let seed = nanos ^ pid.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        Rng(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    // Picks an index with probability proportional to its weight. `weights`
    // must be non-empty and every entry at least 1, which parsing already
    // guarantees for a rule's alternatives.
    fn weighted_index(&mut self, weights: &[u32]) -> usize {
        let total: u64 = weights.iter().map(|&w| w as u64).sum();
        let mut target = self.next_u64() % total;
        for (i, &w) in weights.iter().enumerate() {
            if target < w as u64 {
                return i;
            }
            target -= w as u64;
        }
        weights.len() - 1
    }

    // Picks a uniformly random index in `0..len`. `len` must be non-zero,
    // which parsing already guarantees for a character class.
    fn uniform_index(&mut self, len: usize) -> usize {
        (self.next_u64() % len as u64) as usize
    }
}

pub fn generate(grammar: &Grammar, start: &str, rng: &mut Rng) -> Result<String, String> {
    let mut out = String::new();
    expand(grammar, start, rng, &mut out, 0)?;
    Ok(out)
}

fn expand(
    grammar: &Grammar,
    rule_name: &str,
    rng: &mut Rng,
    out: &mut String,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "generation exceeded the maximum depth ({MAX_DEPTH}) while expanding rule '{rule_name}' - the grammar likely has a rule that refers back to itself with no way to stop"
        ));
    }

    let rule = grammar
        .rules
        .get(rule_name)
        .expect("grammar was resolved, so every reference points at a real rule");
    let weights: Vec<u32> = rule.alternatives.iter().map(|a| a.weight).collect();
    let alt = &rule.alternatives[rng.weighted_index(&weights)];

    for part in &alt.parts {
        match part {
            Part::Literal(text) => out.push_str(text),
            Part::Reference { name, .. } => expand(grammar, name, rng, out, depth + 1)?,
            Part::CharClass(chars) => out.push(chars[rng.uniform_index(chars.len())]),
        }
    }

    Ok(())
}
