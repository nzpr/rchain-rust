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
