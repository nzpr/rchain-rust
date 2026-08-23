//! QuCalc closure superposition over `census_inventory.json`.
//!
//! The single invariant this crate enforces structurally: **`ways` is a coefficient, never
//! expanded.** A closure class with 173,280,448 ways is ONE [`WeightedClass`], not 173M terms.
//! This is the ρ-calculus merge monoid (Law 9) applied to the census data: identical terms
//! collapse into a term + multiplicity instead of being duplicated.

use std::collections::BTreeMap;
use std::path::Path;

/// A weighted event class: the phase-excursion class id plus its signed amplitude and its
/// multiplicity (`ways`). `ways` is carried as a `u64` coefficient — it is *never* materialized
/// into individual terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeightedClass {
    pub class: u32,
    pub signed: i64,
    pub ways: u64,
}

/// A superposition: a multiset of [`WeightedClass`]. Canonical form is most-ways-first.
pub type Superposition = Vec<WeightedClass>;

/// Merge monoid: combine two superpositions, summing `signed` and `ways` per class.
///
/// This is the Law-9 "merge multiplicities into coefficients" step — the performance-critical
/// guarantee: merging two 600M-way terms is one integer add, not 600M term copies.
pub fn merge(a: &[WeightedClass], b: &[WeightedClass]) -> Superposition {
    let mut m: BTreeMap<u32, WeightedClass> = BTreeMap::new();
    for w in a.iter().chain(b) {
        m.entry(w.class)
            .and_modify(|e| {
                e.signed += w.signed;
                e.ways += w.ways;
            })
            .or_insert(*w);
    }
    m.into_values().collect()
}

/// "Most ways first": the canonical order — sort by `ways` descending, `class` ascending as a
/// deterministic tie-break.
pub fn most_ways_first(mut ws: Superposition) -> Superposition {
    ws.sort_by(|x, y| y.ways.cmp(&x.ways).then_with(|| x.class.cmp(&y.class)));
    ws
}

/// Fold to the scalar receipt: the aggregate `(signed_sum, ways_sum)`.
///
/// The phase (±1 real vs ±i imaginary) is a *separate* Pauli-product predicate over the twist
/// history (`pauli_closed ∧ count_balanced`) and is deliberately out of scope here; this fold
/// emits the multiplicity-weighted aggregates the census stores.
pub fn fold(ws: &[WeightedClass]) -> (i64, u64) {
    let signed = ws.iter().map(|w| w.signed).sum();
    let ways = ws.iter().map(|w| w.ways).sum();
    (signed, ways)
}

// --- Census loading ---------------------------------------------------------

#[derive(serde::Deserialize)]
struct CensusJson {
    closures: BTreeMap<String, ClosureJson>,
}

#[derive(serde::Deserialize)]
struct ClosureJson {
    preparation: String,
    #[allow(dead_code)]
    branches: Vec<String>,
    #[serde(rename = "event_classes")]
    event_classes: BTreeMap<String, BTreeMap<String, ClassJson>>,
}

#[derive(serde::Deserialize)]
struct ClassJson {
    signed: i64,
    ways: u64,
}

/// A loaded census: closure name -> (preparation, per-branch weighted classes).
pub struct Census {
    pub closures: BTreeMap<String, (String, BTreeMap<String, Vec<WeightedClass>>)>,
}

impl Census {
    /// Load `census_inventory.json`, expanding each `event_classes[branch][class]` entry into a
    /// [`WeightedClass`] (the class-id string parses as `u32`).
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let cj: CensusJson = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let mut closures = BTreeMap::new();
        for (name, c) in cj.closures {
            let mut branches = BTreeMap::new();
            for (branch, classes) in c.event_classes {
                let mut ws = Vec::new();
                for (class, cc) in classes {
                    ws.push(WeightedClass {
                        class: class.parse().map_err(|_| format!("bad class id {class:?}"))?,
                        signed: cc.signed,
                        ways: cc.ways,
                    });
                }
                branches.insert(branch, ws);
            }
            closures.insert(name, (c.preparation, branches));
        }
        Ok(Census { closures })
    }

    /// Build the full superposition for a closure (merge all branches), canonicalized most-ways-first.
    pub fn closure(&self, name: &str) -> Option<Superposition> {
        let (_, branches) = self.closures.get(name)?;
        let mut sup: Superposition = Vec::new();
        for ws in branches.values() {
            sup = merge(&sup, ws);
        }
        Some(most_ways_first(sup))
    }
}

// --- Pauli predicate (port of zfa-core `pauli.rs` + `history.rs`) -------------
//
// ZFA (half-spin closure) is the two-faced predicate over a twist history:
//   `achieves_zfa(h) = pauli_closed(h) ∧ count_balanced(h)`.
// The arithmetic is **exact integer complex** (entries in {-1, 0, 1}), not
// floating point, so the predicate is deterministic and replay-safe.

/// An exact integer complex number `(re, im)`; entries stay in {-1, 0, 1}.
type C = (i32, i32);

/// The 8-twist alphabet, value → SU(2) Pauli generator:
///   0 `^` = +σ_y   1 `v` = -σ_y   2 `>` = +σ_x   3 `<` = -σ_x
///   4 `/` = +σ_z   5 `\` = -σ_z   6 `+` = +I      7 `-` = -I
/// Positive = even values (0,2,4,6); negative = odd values (1,3,5,7).
fn twist_matrix(t: u8) -> (C, C, C, C) {
    // returns (a, b, c, d) for [[a, b], [c, d]]
    match t {
        0 => ((0, 0), (0, -1), (0, 1), (0, 0)),   // +σ_y
        1 => ((0, 0), (0, 1), (0, -1), (0, 0)),   // -σ_y
        2 => ((0, 0), (1, 0), (1, 0), (0, 0)),    // +σ_x
        3 => ((0, 0), (-1, 0), (-1, 0), (0, 0)),  // -σ_x
        4 => ((1, 0), (0, 0), (0, 0), (-1, 0)),   // +σ_z
        5 => ((-1, 0), (0, 0), (0, 0), (1, 0)),   // -σ_z
        6 => ((1, 0), (0, 0), (0, 0), (1, 0)),    // +I
        7 => ((-1, 0), (0, 0), (0, 0), (-1, 0)),  // -I
        _ => ((1, 0), (0, 0), (0, 0), (1, 0)),    // defensive identity
    }
}

fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn cadd(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}

/// The Pauli matrix product (fold) of a twist history, left-to-right.
pub fn pauli_fold(twists: &[u8]) -> (C, C, C, C) {
    twists.iter().fold(((1, 0), (0, 0), (0, 0), (1, 0)), |acc, &t| {
        let (a, b, c, d) = acc;
        let (e, f, g, h) = twist_matrix(t);
        // [[a,b],[c,d]] · [[e,f],[g,h]]
        (
            cadd(cmul(a, e), cmul(b, g)),
            cadd(cmul(a, f), cmul(b, h)),
            cadd(cmul(c, e), cmul(d, g)),
            cadd(cmul(c, f), cmul(d, h)),
        )
    })
}

/// The scalar phase of a Pauli-closed fold: {+I, −I, +iI, −iI}.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    PlusI,
    MinusI,
    PlusImag,
    MinusImag,
}

impl Phase {
    /// Encode as an i64: 1 = +I, −1 = −I, 2 = +iI, −2 = −iI.
    pub fn code(self) -> i64 {
        match self {
            Phase::PlusI => 1,
            Phase::MinusI => -1,
            Phase::PlusImag => 2,
            Phase::MinusImag => -2,
        }
    }

    fn as_c(self) -> C {
        match self {
            Phase::PlusI => (1, 0),
            Phase::MinusI => (-1, 0),
            Phase::PlusImag => (0, 1),
            Phase::MinusImag => (0, -1),
        }
    }
}

const PHASES: [Phase; 4] = [Phase::PlusI, Phase::MinusI, Phase::PlusImag, Phase::MinusImag];

/// The scalar phase of the fold, or `None` if it is not Pauli-closed.
pub fn pauli_phase(twists: &[u8]) -> Option<Phase> {
    let (a, b, c, d) = pauli_fold(twists);
    if b != (0, 0) || c != (0, 0) || a != d {
        return None;
    }
    PHASES.iter().copied().find(|&p| p.as_c() == a)
}

/// True iff the Pauli fold lands in the scalar group {±I, ±iI}.
pub fn pauli_closed(twists: &[u8]) -> bool {
    pauli_phase(twists).is_some()
}

/// Count balance: `count_pos == count_neg` (even == odd twist values).
pub fn count_balanced(twists: &[u8]) -> bool {
    let (pos, neg) = twists
        .iter()
        .fold((0i64, 0i64), |(p, n), &t| if t % 2 == 0 { (p + 1, n) } else { (p, n + 1) });
    pos == neg
}

/// ZFA = half-spin closure: Pauli-closed AND count-balanced.
pub fn achieves_zfa(twists: &[u8]) -> bool {
    pauli_closed(twists) && count_balanced(twists)
}

#[cfg(test)]
mod pauli_tests {
    use super::*;

    #[test]
    fn empty_history_is_zfa() {
        assert!(pauli_closed(&[]));
        assert!(count_balanced(&[]));
        assert_eq!(pauli_phase(&[]), Some(Phase::PlusI));
    }

    #[test]
    fn up_down_pair_closes_neg_identity() {
        // ^v = σ_y · −σ_y = −I
        assert!(pauli_closed(&[0, 1]));
        assert!(count_balanced(&[0, 1]));
        assert!(achieves_zfa(&[0, 1]));
        assert_eq!(pauli_phase(&[0, 1]), Some(Phase::MinusI));
    }

    #[test]
    fn plus_minus_pair_closes_neg_identity() {
        // +- = I · −I = −I
        assert!(pauli_closed(&[6, 7]));
        assert_eq!(pauli_phase(&[6, 7]), Some(Phase::MinusI));
    }

    #[test]
    fn xy_plane_loop_is_closed() {
        // ^<v> = σ_y · −σ_x · −σ_y · σ_x = −I
        assert!(pauli_closed(&[0, 3, 1, 2]));
        assert!(count_balanced(&[0, 3, 1, 2]));
    }

    #[test]
    fn single_twist_not_closed_and_not_balanced() {
        assert!(!pauli_closed(&[0]));
        assert!(!count_balanced(&[0]));
        assert!(!achieves_zfa(&[0]));
    }

    #[test]
    fn two_non_conjugate_not_closed() {
        // ^> = σ_y σ_x = −iσ_z — off-diagonal, not scalar
        assert!(!pauli_closed(&[0, 2]));
        assert!(pauli_phase(&[0, 2]).is_none());
    }
}

// --- Dialectical synthesis (port of `ai_demonstration.py::qlf_ai_coprocessor`) ---
//
// The neuro-symbolic coprocessor: Thesis and Antithesis are ZFA twist sequences; the
// shared "middle term" is the gauge pair `+-`; Blanket Fusion concatenates the two
// premises and annihilates the gauge pair ("Delayed Choice"), and the residue must be a
// stable ZFA closure (a fluxoid) — the Synthesis.

/// Twist values for the 8-symbol alphabet (see [`to_symbols`]).
pub const UP: u8 = 0;
pub const DOWN: u8 = 1;
pub const RIGHT: u8 = 2;
pub const LEFT: u8 = 3;
pub const SLASH: u8 = 4;
pub const BSLASH: u8 = 5;
pub const PLUS: u8 = 6;
pub const MINUS: u8 = 7;

/// Render a twist sequence back to its `^v<>\ /+-` symbol string.
pub fn to_symbols(twists: &[u8]) -> String {
    twists
        .iter()
        .map(|&t| match t {
            UP => '^',
            DOWN => 'v',
            RIGHT => '>',
            LEFT => '<',
            SLASH => '/',
            BSLASH => '\\',
            PLUS => '+',
            MINUS => '-',
            _ => '?',
        })
        .collect()
}

/// Parse a symbol string into twist values (`None` on an unknown symbol).
pub fn from_symbols(s: &str) -> Option<Vec<u8>> {
    s.chars()
        .map(|c| match c {
            '^' => Some(UP),
            'v' => Some(DOWN),
            '>' => Some(RIGHT),
            '<' => Some(LEFT),
            '/' => Some(SLASH),
            '\\' => Some(BSLASH),
            '+' => Some(PLUS),
            '-' => Some(MINUS),
            _ => None,
        })
        .collect()
}

/// Annihilate the first adjacent gauge pair (`+-` or `-+`) — the "Delayed Choice" step
/// that cancels the shared middle term between the two premises.
pub fn annihilate_gauge(twists: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(twists.len());
    let mut i = 0;
    while i < twists.len() {
        if i + 1 < twists.len()
            && ((twists[i] == PLUS && twists[i + 1] == MINUS)
                || (twists[i] == MINUS && twists[i + 1] == PLUS))
        {
            i += 2; // the gauge pair annihilates
        } else {
            out.push(twists[i]);
            i += 1;
        }
    }
    out
}

/// The result of a dialectical synthesis.
pub struct Synthesis {
    /// The two premises concatenated (before gauge annihilation).
    pub intersection: Vec<u8>,
    /// The residue after the middle-term gauge pair annihilates.
    pub geometry: Vec<u8>,
    /// Whether the residue is a stable ZFA closure (the synthesis holds).
    pub zfa: bool,
    /// The scalar phase of the residue, if Pauli-closed.
    pub phase: Option<Phase>,
}

/// Blanket Fusion of the Aristotle syllogism: Subject (S), Middle term (M = `+`/`-`),
/// Predicate (P). The two premises are `S+` and `-P`; fusing them and annihilating the
/// middle gauge pair yields the synthesis, verified ZFA-closed.
pub fn dialectical_synthesis(subject: &[u8], predicate: &[u8]) -> Synthesis {
    let premise1 = [subject, &[PLUS]].concat(); // S + middle_pos
    let premise2 = [&[MINUS], predicate].concat(); // middle_neg + P
    let mut intersection = premise1;
    intersection.extend_from_slice(&premise2);
    let geometry = annihilate_gauge(&intersection);
    let zfa = achieves_zfa(&geometry);
    let phase = pauli_phase(&geometry);
    Synthesis {
        intersection,
        geometry,
        zfa,
        phase,
    }
}

#[cfg(test)]
mod synthesis_tests {
    use super::*;

    #[test]
    fn annihilates_middle_gauge_pair() {
        // ^<+->v  ->  ^<>v
        assert_eq!(
            annihilate_gauge(&[UP, LEFT, PLUS, MINUS, RIGHT, DOWN]),
            vec![UP, LEFT, RIGHT, DOWN]
        );
    }

    #[test]
    fn socrates_syllogism_fuses_to_stable_fluxoid() {
        // Socrates -> Man -> Mortal : ^< (S) bounded to >v (P) via +- (M)
        let s = dialectical_synthesis(&[UP, LEFT], &[RIGHT, DOWN]);
        assert_eq!(to_symbols(&s.intersection), "^<+->v");
        assert_eq!(to_symbols(&s.geometry), "^<>v");
        assert!(s.zfa, "the R=4 fluxoid must be ZFA-closed");
        assert_eq!(s.phase, Some(Phase::PlusI));
    }

    #[test]
    fn parse_and_render_round_trip() {
        assert_eq!(to_symbols(&from_symbols("^<>v").unwrap()), "^<>v");
    }
}

// --- Neuro layer: deterministic name -> topology (port of quantum-os allocateTwists) ---

/// `allocateTwists(name)`: each byte `b` of the name yields one positive twist
/// `(b & 3) * 2` and one negative twist `((b >> 2) & 3) * 2 + 1`.
///
/// Always count-balanced and deterministic — the "neuro" transition map from a semantic
/// name to a ZFA topology. Pauli closure (order-dependent) is checked separately at
/// grant time.
pub fn allocate_twists(name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() * 2);
    for &b in name.as_bytes() {
        out.push((b & 3) * 2); // positive (even): 0,2,4,6
        out.push(((b >> 2) & 3) * 2 + 1); // negative (odd): 1,3,5,7
    }
    out
}

#[cfg(test)]
mod neuro_tests {
    use super::*;

    #[test]
    fn allocate_twists_is_count_balanced_and_deterministic() {
        let a = allocate_twists("mortal");
        let b = allocate_twists("mortal");
        assert_eq!(a, b, "deterministic");
        assert!(count_balanced(&a), "count-balanced by construction");
        assert_eq!(a.len(), 6 * 2, "one pos + one neg twist per character");
    }
}
