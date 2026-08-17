//! Repetition bounds arithmetic (port of `Multiplier.scala`).
//!
//! A `Multiplier` is a `(min, max)` pair of optional bounds where `None` means unbounded
//! (infinite). Mirrors the Scala `Multiplier` case class and its companion object.

use std::fmt;

use crate::errors::RegexError;

/// `Inf` (the unbounded bound) is represented as `None`, exactly as in Scala.
pub const INF: Option<i32> = None;

// ---------------------------------------------------------------------------
// `Option<i32>` arithmetic, treating `None` as +infinity (port of the Scala
// `OptionIntMath` implicit class).
// ---------------------------------------------------------------------------

fn opt_add(a: Option<i32>, b: Option<i32>) -> Option<i32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        _ => None, // Inf + _ => Inf; _ + Inf => Inf
    }
}

fn opt_sub(a: Option<i32>, b: Option<i32>) -> Result<Option<i32>, RegexError> {
    match (a, b) {
        (Some(_), None) => Err(RegexError::InvalidArgument(
            "Can't substract infinity".to_string(),
        )),
        (None, None) => Ok(Some(0)),
        (None, Some(_)) => Ok(None), // inf - finite => inf
        (Some(x), Some(y)) => Ok(Some(x - y)),
    }
}

fn opt_mul(a: Option<i32>, b: Option<i32>) -> Option<i32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x * y),
        _ => None, // Inf * _ => Inf; _ * Inf => Inf
    }
}

fn opt_ge(a: Option<i32>, b: Option<i32>) -> bool {
    match (a, b) {
        (None, _) => true,       // Inf >= anything
        (Some(_), None) => false, // finite >= Inf
        (Some(x), Some(y)) => x >= y,
    }
}

fn opt_lt(a: Option<i32>, b: Option<i32>) -> bool {
    !opt_ge(a, b)
}

fn opt_le(a: Option<i32>, b: Option<i32>) -> bool {
    match (a, b) {
        (_, None) => true,        // anything <= Inf
        (None, Some(_)) => false, // Inf <= finite
        (Some(x), Some(y)) => x <= y,
    }
}

/// Mirrors the Scala `OptionIntMath.>`; unused by the port (as in the oracle), kept for fidelity.
#[allow(dead_code)]
fn opt_gt(a: Option<i32>, b: Option<i32>) -> bool {
    !opt_le(a, b)
}

fn min_val(first: Option<i32>, second: Option<i32>) -> Option<i32> {
    if opt_le(first, second) {
        first
    } else {
        second
    }
}

fn max_val(first: Option<i32>, second: Option<i32>) -> Option<i32> {
    if opt_ge(first, second) {
        first
    } else {
        second
    }
}

/// A min and a max repetition bound (port of the Scala `Multiplier` case class).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Multiplier {
    min: Option<i32>,
    max: Option<i32>,
}

impl Multiplier {
    pub const PRESET_ZERO: Multiplier = Multiplier {
        min: Some(0),
        max: Some(0),
    };
    pub const PRESET_QUESTION: Multiplier = Multiplier {
        min: Some(0),
        max: Some(1),
    };
    pub const PRESET_ONE: Multiplier = Multiplier {
        min: Some(1),
        max: Some(1),
    };
    pub const PRESET_STAR: Multiplier = Multiplier {
        min: Some(0),
        max: INF,
    };
    pub const PRESET_PLUS: Multiplier = Multiplier {
        min: Some(1),
        max: INF,
    };

    /// Validate and construct a multiplier (port of the Scala constructor `require`).
    pub fn new(min: Option<i32>, max: Option<i32>) -> Result<Self, RegexError> {
        let valid = match (min, max) {
            (Some(x), Some(y)) => (0 <= x) && (x <= y),
            (Some(x), None) => 0 <= x,
            (None, Some(_)) => false,
            (None, None) => true,
        };
        if valid {
            Ok(Multiplier { min, max })
        } else {
            Err(RegexError::InvalidArgument(
                "Invalid multiplier bounds".to_string(),
            ))
        }
    }

    /// Construct without re-validating (used where the invariant is known to hold).
    pub(crate) fn new_unchecked(min: Option<i32>, max: Option<i32>) -> Self {
        Multiplier { min, max }
    }

    /// `Multiplier(min, max)` (both bounds finite).
    pub fn bounds(min: i32, max: i32) -> Result<Self, RegexError> {
        Self::new(Some(min), Some(max))
    }

    /// `Multiplier(n)` (exact repetition).
    pub fn exact(n: i32) -> Result<Self, RegexError> {
        Self::new(Some(n), Some(n))
    }

    pub fn min_bound(&self) -> Option<i32> {
        self.min
    }

    pub fn max_bound(&self) -> Option<i32> {
        self.max
    }

    pub fn mandatory(&self) -> Option<i32> {
        self.min
    }

    pub fn optional(&self) -> Option<i32> {
        match (self.max, self.min) {
            (Some(y), Some(x)) => Some(y - x),
            (None, None) => Some(0),   // Inf - Inf = 0
            (None, Some(_)) => None,   // Inf - finite = Inf
            (Some(_), None) => None,   // unreachable: min = None implies max = None
        }
    }

    /// Find the shared part of two multipliers (largest multiplier safely subtractable from both).
    pub fn common(&self, that: &Multiplier) -> Multiplier {
        let new_mandatory = min_val(self.mandatory(), that.mandatory());
        let new_optional = min_val(self.optional(), that.optional());
        Multiplier::new_unchecked(new_mandatory, opt_add(new_mandatory, new_optional))
    }

    pub fn is_one(&self) -> bool {
        self.min == Some(1) && self.max == Some(1)
    }

    pub fn can_multiply_by(&self, that: &Multiplier) -> bool {
        self.mandatory() == Some(0)
            || opt_ge(
                opt_add(opt_mul(self.optional(), that.mandatory()), Some(1)),
                self.mandatory(),
            )
    }

    /// Multiply two multipliers (throws in Scala when `!canMultiplyBy`).
    pub fn mul(&self, that: &Multiplier) -> Result<Multiplier, RegexError> {
        if self.can_multiply_by(that) {
            Ok(self.mul_unchecked(that))
        } else {
            Err(RegexError::InvalidArgument(format!(
                "Can't multiply {} and {}",
                self, that
            )))
        }
    }

    /// Multiply assuming `canMultiplyBy` holds (no guard).
    pub(crate) fn mul_unchecked(&self, that: &Multiplier) -> Multiplier {
        Multiplier::new_unchecked(opt_mul(self.min, that.min), opt_mul(self.max, that.max))
    }

    /// Add two multipliers (total).
    pub fn add(&self, that: &Multiplier) -> Multiplier {
        Multiplier::new_unchecked(opt_add(self.min, that.min), opt_add(self.max, that.max))
    }

    /// Subtract another multiplier from this one (throws in Scala when not meaningful).
    pub fn sub(&self, that: &Multiplier) -> Result<Multiplier, RegexError> {
        let diff_mandatory = opt_sub(self.mandatory(), that.mandatory())?;
        let diff_optional = opt_sub(self.optional(), that.optional())?;
        Multiplier::new(diff_mandatory, opt_add(diff_mandatory, diff_optional))
    }

    pub fn can_intersect(&self, that: &Multiplier) -> bool {
        !(opt_lt(self.max, that.min) || opt_lt(that.max, self.min))
    }

    /// Intersection (throws in Scala when `!canIntersect`).
    pub fn intersect(&self, that: &Multiplier) -> Result<Multiplier, RegexError> {
        if !self.can_intersect(that) {
            return Err(RegexError::InvalidArgument(format!(
                "Can't intersect {} and {}",
                self, that
            )));
        }
        Ok(self.intersect_unchecked(that))
    }

    /// Intersect assuming `canIntersect` holds (no guard).
    pub(crate) fn intersect_unchecked(&self, that: &Multiplier) -> Multiplier {
        Multiplier::new_unchecked(max_val(self.min, that.min), min_val(self.max, that.max))
    }

    pub fn can_union(&self, that: &Multiplier) -> bool {
        !(opt_lt(opt_add(self.max, Some(1)), that.min)
            || opt_lt(opt_add(that.max, Some(1)), self.min))
    }

    /// Union (throws in Scala when `!canUnion`).
    pub fn union(&self, that: &Multiplier) -> Result<Multiplier, RegexError> {
        if !self.can_union(that) {
            return Err(RegexError::InvalidArgument(format!(
                "Can't union {} and {}",
                self, that
            )));
        }
        Ok(self.union_unchecked(that))
    }

    /// Union assuming `canUnion` holds (no guard).
    pub(crate) fn union_unchecked(&self, that: &Multiplier) -> Multiplier {
        Multiplier::new_unchecked(min_val(self.min, that.min), max_val(self.max, that.max))
    }

    /// Parse an entire character sequence as a multiplier.
    pub fn parse(cs: &str) -> Option<Multiplier> {
        let (mult, count) = Self::try_parse(cs);
        if count == cs.chars().count() {
            Some(mult)
        } else {
            None
        }
    }

    /// Parse a leading multiplier, returning `(multiplier, chars_consumed)`.
    ///
    /// Port of the Scala `Multiplier.tryParse`, which matches the regex
    /// `^\{\s*(\d+)\s*(,\s*(\d+)?)?\s*\}` followed by anything, then falls back to a single
    /// `*` / `?` / `+` character check.
    pub fn try_parse(cs: &str) -> (Multiplier, usize) {
        let bytes = cs.as_bytes();
        if bytes.is_empty() {
            return (Multiplier::PRESET_ONE, 0);
        }

        if bytes[0] == b'{' {
            if let Some((mult, end)) = parse_range(cs) {
                return (mult, end);
            }
        }

        match cs.chars().next() {
            Some('*') => (Multiplier::PRESET_STAR, 1),
            Some('?') => (Multiplier::PRESET_QUESTION, 1),
            Some('+') => (Multiplier::PRESET_PLUS, 1),
            _ => (Multiplier::PRESET_ONE, 0),
        }
    }
}

/// Java-style whitespace: `[ \t\n\x0B\f\r]`.
fn is_java_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

/// Parse `^\{\s*(\d+)\s*(,\s*(\d+)?)?\s*\}` from the start of `cs`, returning the multiplier and
/// the byte offset just past the closing `}`.
fn parse_range(cs: &str) -> Option<(Multiplier, usize)> {
    let bytes = cs.as_bytes();
    let len = bytes.len();
    let mut i = 1; // skip '{'

    i = skip_ws(bytes, i);
    let min_start = i;
    while i < len && is_digit(bytes[i]) {
        i += 1;
    }
    if i == min_start {
        return None; // no minimum digits
    }
    let min: i32 = cs[min_start..i].parse().ok()?;

    let mut j = skip_ws(bytes, i);
    if j < len && bytes[j] == b',' {
        j = skip_ws(bytes, j + 1);
        let max_start = j;
        while j < len && is_digit(bytes[j]) {
            j += 1;
        }
        let max = if j > max_start {
            Some(cs[max_start..j].parse().ok()?)
        } else {
            None // {n,} — unbounded upper bound
        };
        let k = skip_ws(bytes, j);
        if k < len && bytes[k] == b'}' {
            return Some((Multiplier::new_unchecked(Some(min), max), k + 1));
        }
        None
    } else {
        // no comma — exact count {n}
        let k = j; // already at the position after trailing whitespace
        if k < len && bytes[k] == b'}' {
            return Some((Multiplier::new_unchecked(Some(min), Some(min)), k + 1));
        }
        None
    }
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_java_ws(bytes[i]) {
        i += 1;
    }
    i
}

impl fmt::Display for Multiplier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.min, self.max) {
            (Some(x), Some(0)) => write!(f, "{{{x},0}}"),
            (Some(0), Some(1)) => write!(f, "?"),
            (Some(1), Some(1)) => Ok(()),
            (Some(0), None) => write!(f, "*"),
            (Some(1), None) => write!(f, "+"),
            (Some(x), None) => write!(f, "{{{x},}}"),
            (Some(x), Some(y)) if x == y => write!(f, "{{{x}}}"),
            (Some(x), Some(y)) => write!(f, "{{{x},{y}}}"),
            // (None, None) is a valid multiplier but unreachable in the oracle (Scala throws
            // MatchError); render nothing.
            (None, _) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(min: i32, max: i32) -> Multiplier {
        Multiplier::bounds(min, max).unwrap()
    }

    fn minf(min: i32) -> Multiplier {
        Multiplier::new(Some(min), None).unwrap()
    }

    #[test]
    fn try_parse_works() {
        assert_eq!(Multiplier::try_parse("{10}"), (Multiplier::exact(10).unwrap(), 4));
        assert_eq!(Multiplier::try_parse("{2,}"), (minf(2), 4));
        assert_eq!(Multiplier::try_parse("{2,}c"), (minf(2), 4));
        assert_eq!(Multiplier::try_parse("{4 , }def"), (minf(4), 6));
        assert_eq!(Multiplier::try_parse("{  4 , 5 }def"), (m(4, 5), 10));
    }

    #[test]
    fn try_parse_ignores_invalid() {
        assert_eq!(Multiplier::try_parse("{}"), (Multiplier::PRESET_ONE, 0));
        assert_eq!(Multiplier::try_parse("}"), (Multiplier::PRESET_ONE, 0));
        assert_eq!(Multiplier::try_parse("{1"), (Multiplier::PRESET_ONE, 0));
        assert_eq!(Multiplier::try_parse("{,"), (Multiplier::PRESET_ONE, 0));
        assert_eq!(Multiplier::try_parse("{1,"), (Multiplier::PRESET_ONE, 0));
    }

    #[test]
    fn substraction_and_parse_work() {
        assert_eq!(Multiplier::parse("{13,21}"), Some(m(13, 21)));
        assert_eq!(Multiplier::parse("{3,4}").unwrap().common(&Multiplier::parse("{2,5}").unwrap()), Multiplier::parse("{2,3}").unwrap());
        assert_eq!(Multiplier::parse("{3,4}").unwrap().sub(&Multiplier::parse("{2,3}").unwrap()).unwrap(), Multiplier::PRESET_ONE);
        assert_eq!(Multiplier::parse("{2,5}").unwrap().sub(&Multiplier::parse("{2,3}").unwrap()).unwrap(), Multiplier::parse("{0,2}").unwrap());

        assert_eq!(Multiplier::parse("{2,}").unwrap().common(&Multiplier::parse("{1,5}").unwrap()), Multiplier::parse("{1,5}").unwrap());
        assert_eq!(Multiplier::parse("{2,}").unwrap().sub(&Multiplier::parse("{1,5}").unwrap()).unwrap(), Multiplier::PRESET_PLUS);
        assert_eq!(Multiplier::parse("{1,5}").unwrap().sub(&Multiplier::parse("{1,5}").unwrap()).unwrap(), Multiplier::PRESET_ZERO);

        assert_eq!(Multiplier::parse("{3,}").unwrap().common(&Multiplier::parse("{2,}").unwrap()), Multiplier::parse("{2,}").unwrap());
        assert_eq!(Multiplier::parse("{3,}").unwrap().sub(&Multiplier::parse("{2,}").unwrap()).unwrap(), Multiplier::PRESET_ONE);
        assert_eq!(Multiplier::parse("{2,}").unwrap().sub(&Multiplier::parse("{2,}").unwrap()).unwrap(), Multiplier::PRESET_ZERO);

        assert_eq!(Multiplier::parse("{3,}").unwrap().common(&Multiplier::parse("{3,}").unwrap()), Multiplier::parse("{3,}").unwrap());
        assert_eq!(Multiplier::parse("{3,}").unwrap().sub(&Multiplier::parse("{3,}").unwrap()).unwrap(), Multiplier::PRESET_ZERO);
    }

    #[test]
    fn common_operation_works() {
        assert_eq!(Multiplier::PRESET_ONE.common(&Multiplier::PRESET_STAR), Multiplier::PRESET_ZERO);
        assert_eq!(Multiplier::parse("*").unwrap().common(&Multiplier::parse("+").unwrap()), Multiplier::PRESET_STAR);
        assert_eq!(Multiplier::parse("{3,}").unwrap().common(&Multiplier::parse("{2,5}").unwrap()), Multiplier::parse("{2,5}").unwrap());
    }

    #[test]
    fn union_works() {
        let z = Multiplier::PRESET_ZERO;
        let q = Multiplier::PRESET_QUESTION;
        let o = Multiplier::PRESET_ONE;
        let s = Multiplier::PRESET_STAR;
        let p = Multiplier::PRESET_PLUS;

        assert_eq!(z.union(&z).unwrap(), z);
        assert_eq!(z.union(&q).unwrap(), q);
        assert_eq!(z.union(&o).unwrap(), q);
        assert_eq!(z.union(&s).unwrap(), s);
        assert_eq!(z.union(&p).unwrap(), s);

        assert_eq!(q.union(&z).unwrap(), q);
        assert_eq!(q.union(&q).unwrap(), q);
        assert_eq!(q.union(&o).unwrap(), q);
        assert_eq!(q.union(&s).unwrap(), s);
        assert_eq!(q.union(&p).unwrap(), s);

        assert_eq!(o.union(&z).unwrap(), q);
        assert_eq!(o.union(&q).unwrap(), q);
        assert_eq!(o.union(&o).unwrap(), o);
        assert_eq!(o.union(&s).unwrap(), s);
        assert_eq!(o.union(&p).unwrap(), p);

        assert_eq!(s.union(&z).unwrap(), s);
        assert_eq!(s.union(&q).unwrap(), s);
        assert_eq!(s.union(&o).unwrap(), s);
        assert_eq!(s.union(&s).unwrap(), s);
        assert_eq!(s.union(&p).unwrap(), s);

        assert_eq!(p.union(&z).unwrap(), s);
        assert_eq!(p.union(&q).unwrap(), s);
        assert_eq!(p.union(&o).unwrap(), p);
        assert_eq!(p.union(&s).unwrap(), s);
        assert_eq!(p.union(&p).unwrap(), p);

        assert!(!z.can_union(&minf(2)));
        assert!(!o.can_union(&m(3, 4)));
        assert!(!minf(8).can_union(&m(3, 4)));
        assert!(z.union(&m(3, 4)).is_err());
    }
}
