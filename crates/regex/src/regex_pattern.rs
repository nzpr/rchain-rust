//! Regex AST, parser and FSM compilation (port of `RegexPattern.scala`).
//!
//! The `RegexPattern` ADT (`CharClass`, `Conc`, `Alt`, `Mult`) mirrors the Scala sealed class
//! hierarchy. Structural equality (`PartialEq`/`Eq`/`Ord`) coincides with the Scala `equivalent`
//! method (which is itself structural for every subtype), so it is used directly for `==`/`!=`.

use std::collections::BTreeSet;
use std::fmt;

use crate::fsm::{Fsm, ANYTHING_ELSE};
use crate::multiplier::Multiplier;

// ---------------------------------------------------------------------------
// Character sets and escapes (port of the Scala `CharClassPattern` object)
// ---------------------------------------------------------------------------

fn is_all_special(c: char) -> bool {
    matches!(
        c,
        '\\' | '[' | ']' | '|' | '(' | ')' | '.' | '?' | '*' | '+' | '{' | '}'
    )
}

fn escape_char(c: char) -> Option<char> {
    match c {
        't' => Some('\t'),
        'r' => Some('\r'),
        'n' => Some('\n'),
        'f' => Some('\u{000c}'),
        'v' => Some('\u{000b}'),
        _ => None,
    }
}

fn rev_escape_char(c: char) -> Option<char> {
    match c {
        '\t' => Some('t'),
        '\r' => Some('r'),
        '\n' => Some('n'),
        '\u{000c}' => Some('f'),
        '\u{000b}' => Some('v'),
        _ => None,
    }
}

fn word_chars_set() -> BTreeSet<char> {
    "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz"
        .chars()
        .collect()
}

fn digits_char_set() -> BTreeSet<char> {
    "0123456789".chars().collect()
}

fn spaces_char_set() -> BTreeSet<char> {
    "\t\n\u{000b}\u{000c}\r \u{00a0}".chars().collect()
}

fn known_class(c: char) -> Option<CharClassPattern> {
    match c {
        'w' => Some(CharClassPattern::new(word_chars_set(), false)),
        'W' => Some(CharClassPattern::new(word_chars_set(), true)),
        'd' => Some(CharClassPattern::new(digits_char_set(), false)),
        'D' => Some(CharClassPattern::new(digits_char_set(), true)),
        's' => Some(CharClassPattern::new(spaces_char_set(), false)),
        'S' => Some(CharClassPattern::new(spaces_char_set(), true)),
        _ => None,
    }
}

fn known_class_letter(cc: &CharClassPattern) -> Option<char> {
    let known = [
        ('w', word_chars_set(), false),
        ('W', word_chars_set(), true),
        ('d', digits_char_set(), false),
        ('D', digits_char_set(), true),
        ('s', spaces_char_set(), false),
        ('S', spaces_char_set(), true),
    ];
    for (letter, set, negate) in known {
        if cc.char_set() == &set && cc.negate() == negate {
            return Some(letter);
        }
    }
    None
}

fn is_iso_control(c: char) -> bool {
    let cu = c as u32;
    cu <= 0x1F || (0x7F..=0x9F).contains(&cu)
}

fn is_specials_block(c: char) -> bool {
    let cu = c as u32;
    (0xFFF0..=0xFFFF).contains(&cu)
}

fn is_printable(c: char) -> bool {
    let cu = c as u32;
    (32..=127).contains(&cu) || (!is_iso_control(c) && !is_specials_block(c))
}

fn single_char_to_string(c: char) -> String {
    if is_all_special(c) {
        format!("\\{c}")
    } else if let Some(r) = rev_escape_char(c) {
        format!("\\{r}")
    } else if (128..=255).contains(&(c as u32)) {
        format!("\\x{:02X}", c as u32)
    } else if is_printable(c) {
        c.to_string()
    } else if (c as u32) <= 65535 {
        format!("\\u{:04X}", c as u32)
    } else {
        let mut buf = String::new();
        let mut tmp = [0u16; 2];
        for cu in c.encode_utf16(&mut tmp) {
            buf.push_str(&format!("\\u{cu:04X}"));
        }
        buf
    }
}

fn unknown_set_to_string(char_set: &BTreeSet<char>) -> String {
    fn list_to_string(lst: &[char]) -> String {
        match lst.len() {
            0 => String::new(),
            1 => single_char_to_string(lst[0]),
            2 | 3 => lst.iter().map(|&c| single_char_to_string(c)).collect(),
            _ => match (lst.first(), lst.last()) {
                (Some(&first), Some(&last)) => {
                    let start = single_char_to_string(last);
                    let end = single_char_to_string(first);
                    format!("{start}-{end}")
                }
                // Unreachable: this arm is only taken when `lst.len() >= 4`.
                _ => String::new(),
            },
        }
    }

    let sorted: Vec<char> = char_set.iter().copied().collect();
    let mut strings: Vec<String> = Vec::new();
    let mut pending: Vec<char> = Vec::new();
    for next_char in sorted {
        if pending.is_empty() {
            pending.push(next_char);
        } else if (next_char as u32) == (pending[0] as u32) + 1 {
            pending.insert(0, next_char);
        } else {
            strings.push(list_to_string(&pending));
            pending = vec![next_char];
        }
    }
    if !pending.is_empty() {
        strings.push(list_to_string(&pending));
    }
    strings.concat()
}

// ---------------------------------------------------------------------------
// CharClassPattern
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CharClassPattern {
    char_set: BTreeSet<char>,
    negate: bool,
}

impl CharClassPattern {
    pub fn new(char_set: BTreeSet<char>, negate: bool) -> Self {
        CharClassPattern { char_set, negate }
    }

    pub fn from_string(s: &str) -> Self {
        CharClassPattern::new(s.chars().collect(), false)
    }

    pub fn from_chars(cs: &[char]) -> Self {
        CharClassPattern::new(cs.iter().copied().collect(), false)
    }

    pub fn char_set(&self) -> &BTreeSet<char> {
        &self.char_set
    }

    pub fn negate(&self) -> bool {
        self.negate
    }

    pub fn negated(&self) -> CharClassPattern {
        CharClassPattern::new(self.char_set.clone(), !self.negate)
    }

    pub fn alphabet(&self) -> BTreeSet<char> {
        let mut s = self.char_set.clone();
        s.insert(ANYTHING_ELSE);
        s
    }

    pub fn is_empty(&self) -> bool {
        self.char_set.is_empty() && !self.negate
    }

    pub fn to_fsm(&self, alphabet: Option<&BTreeSet<char>>) -> Fsm {
        let actual = alphabet.cloned().unwrap_or_else(|| self.alphabet());
        let inner: BTreeSet<(char, i32)> = if self.negate {
            actual
                .iter()
                .filter(|c| !self.char_set.contains(c))
                .map(|&c| (c, 1))
                .collect()
        } else {
            self.char_set.iter().map(|&c| (c, 1)).collect()
        };
        let map = [(0, inner.into_iter().collect())].into_iter().collect();
        Fsm::new_unchecked(actual, BTreeSet::from([0, 1]), 0, BTreeSet::from([1]), map)
    }

    /// Union of two char classes (the 4 negation-combination cases).
    pub fn union_char_class(&self, that: &CharClassPattern) -> CharClassPattern {
        match (self.negate, that.negate) {
            (true, true) => CharClassPattern::new(
                self.char_set.intersection(&that.char_set).copied().collect(),
                true,
            ),
            (true, false) => CharClassPattern::new(
                self.char_set.difference(&that.char_set).copied().collect(),
                true,
            ),
            (false, true) => CharClassPattern::new(
                that.char_set.difference(&self.char_set).copied().collect(),
                true,
            ),
            (false, false) => CharClassPattern::new(
                self.char_set.union(&that.char_set).copied().collect(),
                false,
            ),
        }
    }

    /// Intersection of two char classes.
    pub fn intersect_char_class(&self, that: &CharClassPattern) -> CharClassPattern {
        match (self.negate, that.negate) {
            (true, true) => CharClassPattern::new(
                self.char_set.union(&that.char_set).copied().collect(),
                true,
            ),
            (true, false) => CharClassPattern::new(
                that.char_set.difference(&self.char_set).copied().collect(),
                false,
            ),
            (false, true) => CharClassPattern::new(
                self.char_set.difference(&that.char_set).copied().collect(),
                false,
            ),
            (false, false) => CharClassPattern::new(
                self.char_set.intersection(&that.char_set).copied().collect(),
                false,
            ),
        }
    }

    pub fn union(&self, that: &RegexPattern) -> RegexPattern {
        match that {
            RegexPattern::CharClass(that_c) => {
                RegexPattern::CharClass(self.union_char_class(that_c))
            }
            _ => RegexPattern::Mult(MultPattern::new(
                RegexPattern::CharClass(self.clone()),
                Multiplier::PRESET_ONE,
            ))
            .union(that),
        }
    }

    pub fn intersection(&self, that: &RegexPattern) -> RegexPattern {
        match that {
            RegexPattern::CharClass(that_c) => {
                RegexPattern::CharClass(self.intersect_char_class(that_c))
            }
            _ => RegexPattern::Mult(MultPattern::new(
                RegexPattern::CharClass(self.clone()),
                Multiplier::PRESET_ONE,
            ))
            .intersection(that),
        }
    }

    // --- parsing ---

    pub fn try_parse(s: &str) -> Option<(RegexPattern, usize)> {
        if s.is_empty() {
            return None;
        }
        match s.as_bytes()[0] as char {
            '.' => Some((
                RegexPattern::CharClass(CharClassPattern::new(BTreeSet::new(), true)),
                1,
            )),
            '\\' => parse_escaped_sequence(s, 1).map(|(cc, n)| (RegexPattern::CharClass(cc), n)),
            '[' => {
                if s.len() > 1 {
                    if s.as_bytes()[1] == b'^' {
                        parse_char_set_sequence(s, 2, true)
                    } else {
                        parse_char_set_sequence(s, 1, false)
                    }
                } else {
                    None
                }
            }
            c if is_all_special(c) => None,
            c => Some((
                RegexPattern::CharClass(CharClassPattern::from_chars(&[c])),
                1,
            )),
        }
    }

    pub fn parse(s: &str) -> Option<RegexPattern> {
        Self::try_parse(s).and_then(|(p, n)| if n == s.len() { Some(p) } else { None })
    }
}

impl fmt::Display for CharClassPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = if self.char_set.is_empty() {
            if self.negate {
                ".".to_string()
            } else {
                String::new()
            }
        } else if self.char_set.len() == 1 {
            match self.char_set.iter().next() {
                Some(&c) => {
                    if self.negate {
                        format!("[^{}]", single_char_to_string(c))
                    } else {
                        single_char_to_string(c)
                    }
                }
                // Unreachable: this arm is only taken when `char_set.len() == 1`.
                None => String::new(),
            }
        } else if let Some(letter) = known_class_letter(self) {
            format!("\\{letter}")
        } else {
            let inv = if self.negate { "^" } else { "" };
            format!("[{inv}{}]", unknown_set_to_string(&self.char_set))
        };
        write!(f, "{s}")
    }
}

// ---------------------------------------------------------------------------
// MultPattern
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MultPattern {
    multiplicand: Box<RegexPattern>,
    multiplier: Multiplier,
}

impl MultPattern {
    pub fn new(multiplicand: RegexPattern, multiplier: Multiplier) -> Self {
        MultPattern {
            multiplicand: Box::new(multiplicand),
            multiplier,
        }
    }

    pub fn multiplicand(&self) -> &RegexPattern {
        &self.multiplicand
    }

    pub fn multiplier(&self) -> Multiplier {
        self.multiplier
    }

    pub fn alphabet(&self) -> BTreeSet<char> {
        self.multiplicand.alphabet()
    }

    pub fn is_empty(&self) -> bool {
        self.multiplicand.is_empty() || self.multiplier.max_bound().unwrap_or(1) < 1
    }

    pub fn reversed(&self) -> MultPattern {
        MultPattern::new(self.multiplicand.reversed(), self.multiplier)
    }

    pub fn multiply(&self, next_multiplier: Multiplier) -> MultPattern {
        if next_multiplier.is_one() {
            self.clone()
        } else if self.multiplier.can_multiply_by(&next_multiplier) {
            MultPattern::new(
                (*self.multiplicand).clone(),
                self.multiplier.mul_unchecked(&next_multiplier),
            )
        } else {
            MultPattern::new(
                RegexPattern::Alt(AltPattern::from_conc(ConcPattern::from_mult(self.clone()))),
                self.multiplier,
            )
        }
    }

    pub fn intersection(&self, that: &RegexPattern) -> RegexPattern {
        let that_mult = match that {
            RegexPattern::Mult(m) => m.clone(),
            other => MultPattern::new(other.clone(), Multiplier::PRESET_ONE),
        };
        if that_mult.multiplicand == self.multiplicand
            && self.multiplier.can_intersect(&that_mult.multiplier)
        {
            RegexPattern::Mult(MultPattern::new(
                (*self.multiplicand).clone(),
                self.multiplier.intersect_unchecked(&that_mult.multiplier),
            ))
        } else {
            ConcPattern::from_mult(self.clone()).intersection(that)
        }
    }

    pub fn common(&self, that: &MultPattern) -> MultPattern {
        if self.multiplicand == that.multiplicand {
            MultPattern::new(
                (*self.multiplicand).clone(),
                self.multiplier.common(&that.multiplier),
            )
        } else {
            MultPattern::new(RegexPattern::nothing(), Multiplier::PRESET_ZERO)
        }
    }

    pub fn to_fsm(&self, alphabet: Option<&BTreeSet<char>>) -> Fsm {
        let actual = alphabet.cloned().unwrap_or_else(|| self.alphabet());
        let start_fsm = self.multiplicand.to_fsm(Some(&actual));
        let mandatory = self.multiplier.mandatory().unwrap_or(0);
        let mandatory_fsm = start_fsm.times_unchecked(mandatory);
        let optional_fsm = match self.multiplier.optional() {
            None => start_fsm.star(),
            Some(k) => Fsm::epsilon_fsm(actual.clone())
                .union(&start_fsm)
                .times_unchecked(k),
        };
        mandatory_fsm.concatenate(&optional_fsm)
    }

    // --- parsing ---

    pub fn try_parse(s: &str) -> Option<(MultPattern, usize)> {
        fn match_multiplicand(s: &str, start: usize) -> Option<(RegexPattern, usize)> {
            if start >= s.len() {
                return None;
            }
            if s.as_bytes()[start] == b'(' {
                AltPattern::try_parse(&s[start + 1..]).and_then(|(alt_pattern, alt_inner_end)| {
                    let alt_end = start + 1 + alt_inner_end;
                    if alt_end < s.len() && s.as_bytes()[alt_end] == b')' {
                        Some((RegexPattern::Alt(alt_pattern), alt_end + 1))
                    } else {
                        None
                    }
                })
            } else {
                CharClassPattern::try_parse(&s[start..])
            }
        }

        match_multiplicand(s, 0).map(|(multiplicand, multiplicand_end)| {
            let (multiplier, multiplier_end) = Multiplier::try_parse(&s[multiplicand_end..]);
            (
                MultPattern::new(multiplicand, multiplier),
                multiplicand_end + multiplier_end,
            )
        })
    }

    pub fn parse(s: &str) -> Option<MultPattern> {
        Self::try_parse(s).and_then(|(p, n)| if n == s.len() { Some(p) } else { None })
    }
}

// ---------------------------------------------------------------------------
// ConcPattern
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConcPattern {
    mults: Vec<MultPattern>,
}

impl ConcPattern {
    pub fn new(mults: Vec<MultPattern>) -> Self {
        ConcPattern { mults }
    }

    pub fn from_mult(m: MultPattern) -> Self {
        ConcPattern::new(vec![m])
    }

    pub fn empty() -> Self {
        ConcPattern::new(Vec::new())
    }

    pub fn mults(&self) -> &[MultPattern] {
        &self.mults
    }

    pub fn alphabet(&self) -> BTreeSet<char> {
        self.mults.iter().flat_map(|m| m.alphabet()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.mults.iter().all(|m| m.is_empty())
    }

    pub fn reversed(&self) -> ConcPattern {
        ConcPattern::new(self.mults.iter().rev().map(|m| m.reversed()).collect())
    }

    pub fn concatenate_conc(&self, that: &ConcPattern) -> ConcPattern {
        ConcPattern::new(
            self.mults
                .iter()
                .chain(that.mults.iter())
                .cloned()
                .collect(),
        )
    }

    pub fn to_fsm(&self, alphabet: Option<&BTreeSet<char>>) -> Fsm {
        let actual = alphabet.cloned().unwrap_or_else(|| self.alphabet());
        self.mults.iter().fold(Fsm::epsilon_fsm(actual.clone()), |acc, m| {
            acc.concatenate(&m.to_fsm(Some(&actual)))
        })
    }

    pub fn intersection(&self, that: &RegexPattern) -> RegexPattern {
        AltPattern::from_conc(self.clone()).intersection(that)
    }

    pub fn union(&self, that: &RegexPattern) -> RegexPattern {
        AltPattern::from_conc(self.clone()).union(that)
    }

    /// TODO in the Scala oracle.
    pub fn common(&self, _that: &ConcPattern, _suffix: bool) -> ConcPattern {
        todo!("TODO")
    }

    // --- parsing ---

    pub fn try_parse(s: &str) -> Option<(ConcPattern, usize)> {
        fn parse_recursive(s: &str, start: usize, mut parsed: Vec<MultPattern>) -> (Vec<MultPattern>, usize) {
            match MultPattern::try_parse(&s[start..]) {
                Some((mult, pos)) => {
                    parsed.push(mult);
                    let next_pos = start + pos;
                    if next_pos < s.len() {
                        parse_recursive(s, next_pos, parsed)
                    } else {
                        (parsed, next_pos)
                    }
                }
                None => (parsed, start),
            }
        }

        let (seq, seq_end) = parse_recursive(s, 0, Vec::new());
        if seq.is_empty() {
            None
        } else {
            Some((ConcPattern::new(seq), seq_end))
        }
    }

    pub fn parse(s: &str) -> Option<ConcPattern> {
        Self::try_parse(s).and_then(|(p, n)| if n == s.len() { Some(p) } else { None })
    }
}

// ---------------------------------------------------------------------------
// AltPattern
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AltPattern {
    concs: BTreeSet<ConcPattern>,
}

impl AltPattern {
    pub fn new(concs: Vec<ConcPattern>) -> Self {
        AltPattern {
            concs: concs.into_iter().collect(),
        }
    }

    pub fn from_conc(c: ConcPattern) -> Self {
        AltPattern::new(vec![c])
    }

    pub fn from_char_classes(cps: &[CharClassPattern]) -> Self {
        AltPattern::new(
            cps.iter()
                .map(|c| ConcPattern::from_mult(MultPattern::new(RegexPattern::CharClass(c.clone()), Multiplier::PRESET_ONE)))
                .collect(),
        )
    }

    pub fn concs(&self) -> &BTreeSet<ConcPattern> {
        &self.concs
    }

    pub fn alphabet(&self) -> BTreeSet<char> {
        self.concs.iter().flat_map(|c| c.alphabet()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.concs.iter().all(|c| c.is_empty())
    }

    pub fn reversed(&self) -> AltPattern {
        AltPattern::new(self.concs.iter().map(|c| c.reversed()).collect())
    }

    pub fn to_fsm(&self, alphabet: Option<&BTreeSet<char>>) -> Fsm {
        let actual = alphabet.cloned().unwrap_or_else(|| self.alphabet());
        self.concs.iter().fold(Fsm::null_fsm(actual.clone()), |acc, c| {
            acc.union(&c.to_fsm(Some(&actual)))
        })
    }

    pub fn union(&self, that: &RegexPattern) -> RegexPattern {
        let that_alt = match that {
            RegexPattern::Alt(a) => a.clone(),
            RegexPattern::CharClass(c) => AltPattern::from_conc(ConcPattern::from_mult(
                MultPattern::new(RegexPattern::CharClass(c.clone()), Multiplier::PRESET_ONE),
            )),
            RegexPattern::Conc(c) => AltPattern::from_conc(c.clone()),
            RegexPattern::Mult(m) => AltPattern::from_conc(ConcPattern::from_mult(m.clone())),
        };
        RegexPattern::Alt(AltPattern::new(
            self.concs
                .iter()
                .chain(that_alt.concs.iter())
                .cloned()
                .collect(),
        ))
    }

    pub fn intersection(&self, _that: &RegexPattern) -> RegexPattern {
        // The Scala oracle converts both patterns to FSMs and then back via `fromFsm`, which is
        // `NotImplementedError` ("TODO"). Faithfully reproduce that as a todo!().
        todo!("TODO")
    }

    // --- parsing ---

    pub fn try_parse(s: &str) -> Option<(AltPattern, usize)> {
        fn parse_recursive(s: &str, start: usize, mut parsed: Vec<ConcPattern>) -> (Vec<ConcPattern>, usize) {
            match ConcPattern::try_parse(&s[start..]) {
                Some((conc, pos)) => {
                    parsed.push(conc);
                    let next_pos = start + pos;
                    if next_pos < s.len() && s.as_bytes()[next_pos] == b'|' {
                        parse_recursive(s, next_pos + 1, parsed)
                    } else {
                        (parsed, next_pos)
                    }
                }
                None => (parsed, start),
            }
        }

        let (seq, seq_end) = parse_recursive(s, 0, Vec::new());
        if seq.is_empty() {
            None
        } else {
            Some((AltPattern::new(seq), seq_end))
        }
    }

    pub fn parse(s: &str) -> Option<AltPattern> {
        Self::try_parse(s).and_then(|(p, n)| if n == s.len() { Some(p) } else { None })
    }
}

// ---------------------------------------------------------------------------
// RegexPattern (enum)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegexPattern {
    CharClass(CharClassPattern),
    Conc(ConcPattern),
    Alt(AltPattern),
    Mult(MultPattern),
}

impl RegexPattern {
    /// Scala `RegexPattern.fromFsm` — `NotImplementedError("TODO")`.
    pub fn from_fsm(_fsm: Fsm) -> RegexPattern {
        todo!("TODO")
    }

    /// The pattern expressing "no possibilities at all" (`RegexPattern.nothing`).
    pub fn nothing() -> RegexPattern {
        RegexPattern::CharClass(CharClassPattern::new(BTreeSet::new(), false))
    }

    pub fn alphabet(&self) -> BTreeSet<char> {
        let mut s = match self {
            RegexPattern::CharClass(c) => c.char_set.clone(),
            RegexPattern::Conc(c) => c.alphabet(),
            RegexPattern::Alt(a) => a.alphabet(),
            RegexPattern::Mult(m) => m.alphabet(),
        };
        s.insert(ANYTHING_ELSE);
        s
    }

    pub fn to_fsm(&self, alphabet: Option<&BTreeSet<char>>) -> Fsm {
        match self {
            RegexPattern::CharClass(c) => c.to_fsm(alphabet),
            RegexPattern::Conc(c) => c.to_fsm(alphabet),
            RegexPattern::Alt(a) => a.to_fsm(alphabet),
            RegexPattern::Mult(m) => m.to_fsm(alphabet),
        }
    }

    pub fn accepts(&self, s: &str) -> bool {
        self.to_fsm(None).accepts(s)
    }

    fn to_conc(&self) -> ConcPattern {
        match self {
            RegexPattern::Conc(c) => c.clone(),
            RegexPattern::CharClass(c) => ConcPattern::from_mult(MultPattern::new(
                RegexPattern::CharClass(c.clone()),
                Multiplier::PRESET_ONE,
            )),
            RegexPattern::Alt(a) => ConcPattern::from_mult(MultPattern::new(
                RegexPattern::Alt(a.clone()),
                Multiplier::PRESET_ONE,
            )),
            RegexPattern::Mult(m) => ConcPattern::from_mult(m.clone()),
        }
    }

    pub fn concatenate(&self, that: &RegexPattern) -> ConcPattern {
        self.to_conc().concatenate_conc(&that.to_conc())
    }

    pub fn union(&self, that: &RegexPattern) -> RegexPattern {
        match self {
            RegexPattern::CharClass(c) => c.union(that),
            RegexPattern::Conc(c) => AltPattern::from_conc(c.clone()).union(that),
            RegexPattern::Alt(a) => a.union(that),
            RegexPattern::Mult(m) => AltPattern::from_conc(ConcPattern::from_mult(m.clone())).union(that),
        }
    }

    pub fn intersection(&self, that: &RegexPattern) -> RegexPattern {
        match self {
            RegexPattern::CharClass(c) => c.intersection(that),
            RegexPattern::Conc(c) => AltPattern::from_conc(c.clone()).intersection(that),
            RegexPattern::Alt(a) => a.intersection(that),
            RegexPattern::Mult(m) => m.intersection(that),
        }
    }

    pub fn multiply(&self, multiplier: Multiplier) -> MultPattern {
        match self {
            RegexPattern::CharClass(c) => {
                MultPattern::new(RegexPattern::CharClass(c.clone()), multiplier)
            }
            RegexPattern::Conc(c) => {
                MultPattern::new(RegexPattern::Alt(AltPattern::from_conc(c.clone())), multiplier)
            }
            RegexPattern::Alt(a) => MultPattern::new(RegexPattern::Alt(a.clone()), multiplier),
            RegexPattern::Mult(m) => m.multiply(multiplier),
        }
    }

    /// `multiply(Multiplier(n))`; assumes `n >= 0` (the Scala `require`).
    pub fn multiply_int(&self, n: i32) -> MultPattern {
        self.multiply(Multiplier::new_unchecked(Some(n), Some(n)))
    }

    pub fn reversed(&self) -> RegexPattern {
        match self {
            RegexPattern::CharClass(c) => RegexPattern::CharClass(c.clone()),
            RegexPattern::Conc(c) => RegexPattern::Conc(c.reversed()),
            RegexPattern::Alt(a) => RegexPattern::Alt(a.reversed()),
            RegexPattern::Mult(m) => RegexPattern::Mult(m.reversed()),
        }
    }

    pub fn negated(&self) -> RegexPattern {
        match self {
            RegexPattern::CharClass(c) => RegexPattern::CharClass(c.negated()),
            RegexPattern::Conc(_) => todo!("TODO"),
            RegexPattern::Alt(_) => todo!("TODO"),
            RegexPattern::Mult(_) => todo!("TODO"),
        }
    }

    pub fn reduced(&self) -> RegexPattern {
        match self {
            RegexPattern::CharClass(c) => RegexPattern::CharClass(c.clone()),
            RegexPattern::Conc(_) => todo!("TODO"),
            RegexPattern::Alt(_) => todo!("TODO"),
            RegexPattern::Mult(_) => todo!("TODO"),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            RegexPattern::CharClass(c) => c.is_empty(),
            RegexPattern::Conc(c) => c.is_empty(),
            RegexPattern::Alt(a) => a.is_empty(),
            RegexPattern::Mult(m) => m.is_empty(),
        }
    }

    pub fn try_parse(s: &str) -> Option<(RegexPattern, usize)> {
        AltPattern::try_parse(s).map(|(a, n)| (RegexPattern::Alt(a), n))
    }

    pub fn parse(s: &str) -> Option<RegexPattern> {
        Self::try_parse(s).and_then(|(p, n)| if n == s.len() { Some(p) } else { None })
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers (byte-indexed over ASCII regex syntax)
// ---------------------------------------------------------------------------

fn parse_hex_char(s: &str, start_index: usize, count: usize) -> Option<(char, usize)> {
    if start_index + count <= s.len() {
        let substr = &s[start_index..start_index + count];
        let cp = u32::from_str_radix(substr, 16).ok()?;
        char::from_u32(cp).map(|ch| (ch, start_index + count))
    } else {
        None
    }
}

fn parse_escaped_sequence(s: &str, start_index: usize) -> Option<(CharClassPattern, usize)> {
    if start_index >= s.len() {
        return None;
    }
    match s.as_bytes()[start_index] as char {
        'x' => parse_hex_char(s, start_index + 1, 2)
            .map(|(ch, n)| (CharClassPattern::from_chars(&[ch]), n)),
        'u' => parse_hex_char(s, start_index + 1, 4)
            .map(|(ch, n)| (CharClassPattern::from_chars(&[ch]), n)),
        c => {
            if let Some(known) = known_class(c) {
                Some((known, start_index + 1))
            } else if let Some(esc) = escape_char(c) {
                Some((CharClassPattern::from_chars(&[esc]), start_index + 1))
            } else {
                Some((CharClassPattern::from_chars(&[c]), start_index + 1))
            }
        }
    }
}

enum CharOrClass {
    Char(char),
    Class(CharClassPattern),
}

fn parse_internal_escaped_sequence(s: &str, start_index: usize) -> Option<(CharOrClass, usize)> {
    if start_index >= s.len() {
        return None;
    }
    match s.as_bytes()[start_index] as char {
        'x' => parse_hex_char(s, start_index + 1, 2).map(|(ch, n)| (CharOrClass::Char(ch), n)),
        'u' => parse_hex_char(s, start_index + 1, 4).map(|(ch, n)| (CharOrClass::Char(ch), n)),
        c => {
            if let Some(esc) = escape_char(c) {
                Some((CharOrClass::Char(esc), start_index + 1))
            } else if let Some(known) = known_class(c) {
                Some((CharOrClass::Class(known), start_index + 1))
            } else {
                Some((CharOrClass::Char(c), start_index + 1))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RangeState {
    FirstSymbol,
    NotStarted,
    Inside,
    JustFinished,
}

#[derive(Clone)]
struct CharSetParseState {
    collected_chars: Vec<char>,
    collected_union_classes: Vec<CharClassPattern>,
    range_state: RangeState,
}

impl CharSetParseState {
    fn change_state(&self, next: RangeState) -> Self {
        CharSetParseState {
            collected_chars: self.collected_chars.clone(),
            collected_union_classes: self.collected_union_classes.clone(),
            range_state: next,
        }
    }

    fn add(&self, value: CharOrClass, override_state: Option<RangeState>) -> Self {
        let actual = override_state.unwrap_or(self.range_state);
        match value {
            CharOrClass::Char(add_char) => {
                if actual == RangeState::Inside {
                    let head = self.collected_chars[0];
                    let mut new_chars: Vec<char> = (head..=add_char).collect();
                    new_chars.extend(self.collected_chars.iter().copied());
                    CharSetParseState {
                        collected_chars: new_chars,
                        collected_union_classes: self.collected_union_classes.clone(),
                        range_state: RangeState::JustFinished,
                    }
                } else {
                    let mut new_chars = vec![add_char];
                    new_chars.extend(self.collected_chars.iter().copied());
                    CharSetParseState {
                        collected_chars: new_chars,
                        collected_union_classes: self.collected_union_classes.clone(),
                        range_state: RangeState::NotStarted,
                    }
                }
            }
            CharOrClass::Class(add_class) => {
                if actual == RangeState::Inside {
                    let mut new_chars = vec!['-'];
                    new_chars.extend(self.collected_chars.iter().copied());
                    let mut new_classes = vec![add_class];
                    new_classes.extend(self.collected_union_classes.iter().cloned());
                    CharSetParseState {
                        collected_chars: new_chars,
                        collected_union_classes: new_classes,
                        range_state: RangeState::JustFinished,
                    }
                } else {
                    let mut new_classes = vec![add_class];
                    new_classes.extend(self.collected_union_classes.iter().cloned());
                    CharSetParseState {
                        collected_chars: self.collected_chars.clone(),
                        collected_union_classes: new_classes,
                        range_state: RangeState::JustFinished,
                    }
                }
            }
        }
    }
}

fn parse_char_set_sequence(s: &str, start_index: usize, negate: bool) -> Option<(RegexPattern, usize)> {
    fn process_next_char(
        s: &str,
        current_index: usize,
        state: CharSetParseState,
    ) -> Option<(CharSetParseState, usize)> {
        if current_index >= s.len() {
            return None;
        }
        match s.as_bytes()[current_index] as char {
            '\\' => match parse_internal_escaped_sequence(s, current_index + 1) {
                Some((char_or_class, next_pos)) => {
                    process_next_char(s, next_pos, state.add(char_or_class, None))
                }
                None => None,
            },
            ']' if state.range_state != RangeState::FirstSymbol => {
                if state.range_state == RangeState::Inside {
                    Some((
                        state.add(CharOrClass::Char('-'), Some(RangeState::NotStarted)),
                        current_index + 1,
                    ))
                } else {
                    Some((state, current_index + 1))
                }
            }
            '-' => match state.range_state {
                RangeState::NotStarted => {
                    process_next_char(s, current_index + 1, state.change_state(RangeState::Inside))
                }
                RangeState::Inside | RangeState::JustFinished | RangeState::FirstSymbol => {
                    process_next_char(s, current_index + 1, state.add(CharOrClass::Char('-'), None))
                }
            },
            other => {
                process_next_char(s, current_index + 1, state.add(CharOrClass::Char(other), None))
            }
        }
    }

    let initial = CharSetParseState {
        collected_chars: Vec::new(),
        collected_union_classes: Vec::new(),
        range_state: RangeState::FirstSymbol,
    };

    process_next_char(s, start_index, initial).map(|(state, end_index)| {
        if state.collected_union_classes.is_empty() {
            (
                RegexPattern::CharClass(CharClassPattern::new(
                    state.collected_chars.into_iter().collect(),
                    negate,
                )),
                end_index,
            )
        } else {
            let start = CharClassPattern::new(state.collected_chars.into_iter().collect(), false);
            let union = state
                .collected_union_classes
                .iter()
                .fold(start, |acc, c| acc.union_char_class(c));
            let result = if negate { union.negated() } else { union };
            (RegexPattern::CharClass(result), end_index)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(s: &str) -> RegexPattern {
        RegexPattern::CharClass(CharClassPattern::from_string(s))
    }

    fn cc_neg(s: &str) -> RegexPattern {
        RegexPattern::CharClass(CharClassPattern::from_string(s).negated())
    }

    fn mult(multiplicand: RegexPattern, multiplier: Multiplier) -> MultPattern {
        MultPattern::new(multiplicand, multiplier)
    }

    fn conc(mults: Vec<MultPattern>) -> ConcPattern {
        ConcPattern::new(mults)
    }

    fn alt(concs: Vec<ConcPattern>) -> AltPattern {
        AltPattern::new(concs)
    }

    fn parsed_cc_str(s: &str) -> String {
        match CharClassPattern::parse(s).unwrap() {
            RegexPattern::CharClass(c) => c.to_string(),
            _ => panic!("expected char class"),
        }
    }

    #[test]
    fn char_class_equality() {
        assert_eq!(cc("a"), cc("a"));
        assert_eq!(cc("a").negated().negated(), cc("a"));
        assert_eq!(cc("a"), cc("a").negated().negated());
        assert_eq!(cc("a").negated(), cc("a").negated());
        assert_ne!(cc("a").negated(), cc("a"));
        assert_eq!(cc("ab"), cc("ba"));
    }

    #[test]
    fn char_class_union() {
        assert_eq!(cc("ab").union(&cc("bc")), cc("abc"));
        assert_eq!(cc("ab").union(&cc_neg("bc")), cc_neg("c"));
        assert_eq!(cc_neg("ab").union(&cc("bc")), cc_neg("a"));
        assert_eq!(cc_neg("ab").union(&cc_neg("bc")), cc_neg("b"));
    }

    #[test]
    fn char_class_intersection() {
        assert_eq!(cc("ab").intersection(&cc("bc")), cc("b"));
        assert_eq!(cc("ab").intersection(&cc_neg("bc")), cc("a"));
        assert_eq!(cc_neg("ab").intersection(&cc("bc")), cc("c"));
        assert_eq!(cc_neg("ab").intersection(&cc_neg("bc")), cc_neg("abc"));
    }

    #[test]
    fn char_class_is_empty() {
        assert!(cc("").is_empty());
        assert!(!cc_neg("").is_empty());
    }

    #[test]
    fn char_class_multiplication() {
        assert_eq!(cc("a").multiply_int(1), mult(cc("a"), Multiplier::PRESET_ONE));
        assert_ne!(
            RegexPattern::Mult(cc("a").multiply(Multiplier::new(Some(1), Some(2)).unwrap())),
            cc("a")
        );
    }

    #[test]
    fn char_class_fsm() {
        let not_a = cc_neg("a").to_fsm(None);
        assert_eq!(not_a.alphabet(), &{
            let mut s = BTreeSet::new();
            s.insert('a');
            s.insert(ANYTHING_ELSE);
            s
        });
        assert!(not_a.accepts("b"));
        assert!(not_a.accepts(&ANYTHING_ELSE.to_string()));
    }

    #[test]
    fn char_class_try_parse_all_classes() {
        let not_d0 = CharClassPattern::parse("[\\D0]").unwrap().to_fsm(None);
        assert!(not_d0.accepts("a") && not_d0.accepts("0") && !not_d0.accepts("1"));
        assert_eq!(CharClassPattern::parse("[\\D]"), CharClassPattern::parse("\\D"));
        assert_eq!(CharClassPattern::parse("[^\\D]"), CharClassPattern::parse("\\d"));

        assert_eq!(CharClassPattern::parse("[a-]"), Some(cc("a-")));
        assert_eq!(CharClassPattern::parse("[a-b-z]"), Some(cc("abz-")));
        assert_eq!(CharClassPattern::parse("[-a]"), Some(cc("a-")));
        assert_eq!(CharClassPattern::parse("[-]"), Some(cc("-")));
        assert_eq!(CharClassPattern::parse("[---]"), Some(cc("-")));
        assert_eq!(CharClassPattern::parse("[--0]"), Some(cc("-./0")));
        assert_eq!(CharClassPattern::parse("[--0-z]"), Some(cc("-./0z")));
        assert_eq!(CharClassPattern::parse("[+--.]"), Some(cc("+,-.")));
        assert_eq!(CharClassPattern::parse("[]]"), Some(cc("]")));
        assert_eq!(CharClassPattern::parse("[^]]"), Some(cc_neg("]")));
        assert_eq!(CharClassPattern::parse("[\\d-]"), Some(cc("0123456789-")));
        assert_eq!(CharClassPattern::parse("[-\\d]"), Some(cc("0123456789-")));
        assert_eq!(CharClassPattern::parse("[\\d-\\d]"), Some(cc("0123456789-")));

        assert_eq!(CharClassPattern::parse("\\x41"), Some(cc("A")));
        assert_eq!(CharClassPattern::parse("\\u0041"), Some(cc("A")));
        assert_eq!(CharClassPattern::parse("\\["), Some(cc("[")));
        assert_eq!(CharClassPattern::parse("\\t"), Some(cc("\t")));
        assert_eq!(CharClassPattern::parse("[\\t]"), Some(cc("\t")));
        assert_eq!(CharClassPattern::parse("\\Z"), Some(cc("Z")));
        assert_eq!(CharClassPattern::parse("[\\Z]"), Some(cc("Z")));
        assert_eq!(CharClassPattern::parse("[^\\t\\[]"), Some(cc_neg("\t[")));

        assert_eq!(CharClassPattern::parse("a"), Some(cc("a")));
        assert_eq!(CharClassPattern::parse("\\s"), Some(RegexPattern::CharClass(CharClassPattern::new(spaces_char_set(), false))));
        assert_eq!(CharClassPattern::parse("\\S"), Some(RegexPattern::CharClass(CharClassPattern::new(spaces_char_set(), true))));
        assert_eq!(CharClassPattern::parse("\\d"), Some(cc("0123456789")));
        assert_eq!(CharClassPattern::parse("\\D"), Some(cc_neg("0123456789")));
        assert_eq!(CharClassPattern::parse("\\w"), Some(RegexPattern::CharClass(CharClassPattern::new(word_chars_set(), false))));
        assert_eq!(CharClassPattern::parse("\\W"), Some(RegexPattern::CharClass(CharClassPattern::new(word_chars_set(), true))));
        assert_eq!(CharClassPattern::parse("."), Some(cc_neg("")));
        assert_eq!(CharClassPattern::parse("[abc]"), Some(cc("abc")));
        assert_eq!(CharClassPattern::parse("[^abc]"), Some(cc_neg("abc")));

        assert_eq!(CharClassPattern::parse("[\\x41]"), Some(cc("A")));
        assert_eq!(CharClassPattern::parse("[\\x41-\\x44]"), Some(cc("ABCD")));
        assert_eq!(CharClassPattern::parse("[^\\x41]"), Some(cc_neg("A")));
        assert_eq!(CharClassPattern::parse("[^\\x41-\\x44]"), Some(cc_neg("ABCD")));
        assert_eq!(CharClassPattern::parse("[\\u0041]"), Some(cc("A")));
        assert_eq!(CharClassPattern::parse("[\\u0041-\\u0044]"), Some(cc("ABCD")));
        assert_eq!(CharClassPattern::parse("[^\\u0041]"), Some(cc_neg("A")));
        assert_eq!(CharClassPattern::parse("[^\\u0041-\\u0044]"), Some(cc_neg("ABCD")));
    }

    #[test]
    fn char_class_parse_returns_none_on_invalid() {
        assert!(CharClassPattern::parse("\\x4").is_none());
        assert!(CharClassPattern::parse("\\u004").is_none());
        assert!(CharClassPattern::parse("\\").is_none());
        assert!(CharClassPattern::parse("[").is_none());
        assert!(CharClassPattern::parse("[a-").is_none());
        assert!(CharClassPattern::parse("[^").is_none());
        assert!(CharClassPattern::parse("[^\\").is_none());
        assert!(CharClassPattern::parse("[^a").is_none());
        assert!(CharClassPattern::parse("[^a-").is_none());
        assert!(CharClassPattern::parse("[^\\x3]").is_none());
        assert!(CharClassPattern::parse("[^\\u003]").is_none());
        assert!(CharClassPattern::parse("[]").is_none());
        assert!(CharClassPattern::parse("[^]").is_none());
    }

    #[test]
    fn mult_pattern_parse_accepts_simple() {
        assert_eq!(MultPattern::parse("a"), Some(mult(cc("a"), Multiplier::PRESET_ONE)));
        assert_eq!(MultPattern::parse("a*"), Some(mult(cc("a"), Multiplier::PRESET_STAR)));
        assert_eq!(MultPattern::parse("a?"), Some(mult(cc("a"), Multiplier::PRESET_QUESTION)));
        assert_eq!(MultPattern::parse("a+"), Some(mult(cc("a"), Multiplier::PRESET_PLUS)));
        assert_eq!(MultPattern::parse("a{3,5}"), Some(mult(cc("a"), Multiplier::new(Some(3), Some(5)).unwrap())));
        assert_eq!(MultPattern::parse("a{3,}"), Some(mult(cc("a"), Multiplier::new(Some(3), None).unwrap())));
    }

    #[test]
    fn mult_pattern_parse_returns_none_on_invalid() {
        assert!(MultPattern::parse("(a").is_none());
        assert!(MultPattern::parse("a{}").is_none());
        assert!(MultPattern::parse("a{3").is_none());
        assert!(MultPattern::parse("a{3,").is_none());
        assert!(MultPattern::parse("a{,4}").is_none());
    }

    #[test]
    fn mult_pattern_multiply_works() {
        let a = mult(cc("a"), Multiplier::PRESET_ONE);
        assert_eq!(a, a.multiply(Multiplier::PRESET_ONE));
        assert_eq!(
            mult(cc("a"), Multiplier::new(Some(2), Some(2)).unwrap()),
            a.multiply(Multiplier::new(Some(2), Some(2)).unwrap())
        );
        assert_eq!(
            mult(cc("a"), Multiplier::PRESET_PLUS).multiply(Multiplier::new(Some(3), Some(4)).unwrap()),
            a.multiply(Multiplier::new(Some(3), None).unwrap())
        );
    }

    #[test]
    fn regex_pattern_parse_groups() {
        assert_eq!(
            MultPattern::try_parse("(a)"),
            Some((mult(RegexPattern::Alt(alt(vec![conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)])])), Multiplier::PRESET_ONE), 3))
        );

        let ain = MultPattern::parse("((a))").unwrap().to_fsm(None);
        assert!(ain.accepts("a"));

        let ab = RegexPattern::parse("(a)b").unwrap();
        assert_eq!(
            ab,
            RegexPattern::Alt(alt(vec![conc(vec![
                mult(RegexPattern::Alt(alt(vec![conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)])])), Multiplier::PRESET_ONE),
                mult(cc("b"), Multiplier::PRESET_ONE),
            ])]))
        );
        assert!(ab.to_fsm(None).accepts("ab"));

        assert!(RegexPattern::parse("((a))b").unwrap().to_fsm(None).accepts("ab"));
        assert!(RegexPattern::parse("((a)(b*))c").unwrap().to_fsm(None).accepts("abbbc"));
    }

    #[test]
    fn mult_pattern_common_works() {
        let a_star = MultPattern::parse("a*").unwrap();
        let a_plus = MultPattern::parse("a+").unwrap();
        assert_eq!(a_star.common(&a_plus), a_star);
    }

    #[test]
    fn conc_pattern_parse_sequences() {
        assert_eq!(
            ConcPattern::parse("a"),
            Some(conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]))
        );
        assert_eq!(
            ConcPattern::parse("ab"),
            Some(conc(vec![
                mult(cc("a"), Multiplier::PRESET_ONE),
                mult(cc("b"), Multiplier::PRESET_ONE),
            ]))
        );
        assert_eq!(
            ConcPattern::parse("abc"),
            Some(conc(vec![
                mult(cc("a"), Multiplier::PRESET_ONE),
                mult(cc("b"), Multiplier::PRESET_ONE),
                mult(cc("c"), Multiplier::PRESET_ONE),
            ]))
        );
    }

    #[test]
    fn conc_pattern_parse_none_on_invalid() {
        assert!(ConcPattern::parse("").is_none());
        assert!(ConcPattern::parse("\\").is_none());
    }

    #[test]
    fn conc_pattern_equality_rare_cases() {
        assert_eq!(ConcPattern::empty(), ConcPattern::empty());
        assert_ne!(
            RegexPattern::Conc(ConcPattern::empty()),
            RegexPattern::Mult(mult(cc("a"), Multiplier::PRESET_ONE))
        );
        assert_ne!(
            RegexPattern::Conc(conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)])),
            RegexPattern::Mult(mult(cc("a"), Multiplier::PRESET_ONE))
        );
    }

    #[test]
    fn alt_pattern_parse_alt_sequences() {
        assert_eq!(
            RegexPattern::parse("a|b"),
            Some(RegexPattern::Alt(alt(vec![
                conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]),
                conc(vec![mult(cc("b"), Multiplier::PRESET_ONE)]),
            ])))
        );
        assert_eq!(
            RegexPattern::parse("a?b"),
            Some(RegexPattern::Alt(alt(vec![conc(vec![
                mult(cc("a"), Multiplier::PRESET_QUESTION),
                mult(cc("b"), Multiplier::PRESET_ONE),
            ])])))
        );
        assert_eq!(
            RegexPattern::parse("a?b{3,}"),
            Some(RegexPattern::Alt(alt(vec![conc(vec![
                mult(cc("a"), Multiplier::PRESET_QUESTION),
                mult(cc("b"), Multiplier::new(Some(3), None).unwrap()),
            ])])))
        );
        assert!(RegexPattern::parse("ac*").is_some());
        assert!(RegexPattern::parse("b{3,4}c").is_some());
        assert!(RegexPattern::parse("b{3,}c").is_some());
        assert_eq!(
            RegexPattern::parse("a?b{3,}c*"),
            Some(RegexPattern::Alt(alt(vec![conc(vec![
                mult(cc("a"), Multiplier::PRESET_QUESTION),
                mult(cc("b"), Multiplier::new(Some(3), None).unwrap()),
                mult(cc("c"), Multiplier::new(Some(0), None).unwrap()),
            ])])))
        );
    }

    #[test]
    fn alt_pattern_union() {
        assert_eq!(
            conc(vec![mult(cc("a"), Multiplier::PRESET_QUESTION)]).union(&RegexPattern::Conc(conc(vec![mult(cc("b"), Multiplier::PRESET_QUESTION)]))),
            RegexPattern::Alt(alt(vec![
                conc(vec![mult(cc("a"), Multiplier::PRESET_QUESTION)]),
                conc(vec![mult(cc("b"), Multiplier::PRESET_QUESTION)]),
            ]))
        );
    }

    #[test]
    fn empty_works_for_all_char_classes() {
        assert!(cc("").is_empty());
        assert!(RegexPattern::Conc(ConcPattern::empty()).is_empty());
        assert!(RegexPattern::Alt(AltPattern::new(vec![])).is_empty());
        assert!(mult(cc(""), Multiplier::PRESET_ONE).is_empty());
        assert!(mult(cc("a"), Multiplier::PRESET_ZERO).is_empty());
    }

    #[test]
    fn mult_pattern_produces_good_fsm() {
        let a1 = cc("a").multiply_int(1);
        assert!(a1.to_fsm(None).accepts("a"));
        assert!(!a1.to_fsm(None).accepts("b"));
        assert!(!a1.to_fsm(None).accepts("aa"));

        let a2 = cc("a").multiply_int(2);
        assert!(!a2.to_fsm(None).accepts("a"));
        assert!(!a2.to_fsm(None).accepts("b"));
        assert!(a2.to_fsm(None).accepts("aa"));
        assert!(!a2.to_fsm(None).accepts("aaa"));

        let a_quest = cc("a").multiply(Multiplier::PRESET_QUESTION);
        assert!(a_quest.to_fsm(None).accepts(""));
        assert!(a_quest.to_fsm(None).accepts("a"));
        assert!(!a_quest.to_fsm(None).accepts("b"));
        assert!(!a_quest.to_fsm(None).accepts("aa"));

        let a_star = cc("a").multiply(Multiplier::PRESET_STAR);
        assert!(a_star.to_fsm(None).accepts(""));
        assert!(a_star.to_fsm(None).accepts("a"));
        assert!(!a_star.to_fsm(None).accepts("b"));
        assert!(a_star.to_fsm(None).accepts("aa"));
        assert!(a_star.to_fsm(None).accepts("aaaaaaaaaaaaaaaaaaa"));

        let a_plus = cc("a").multiply(Multiplier::PRESET_PLUS);
        assert!(!a_plus.to_fsm(None).accepts(""));
        assert!(a_plus.to_fsm(None).accepts("a"));
        assert!(!a_plus.to_fsm(None).accepts("b"));

        let a_zero = cc("a").multiply(Multiplier::PRESET_ZERO);
        assert!(a_zero.to_fsm(None).accepts(""));
        assert!(!a_zero.to_fsm(None).accepts("a"));
        assert!(!a_zero.to_fsm(None).accepts("b"));
    }

    #[test]
    fn reverse_works_for_all_pattern_types() {
        assert_eq!(cc("a").reversed(), cc("a"));

        let a_or_b = RegexPattern::Alt(AltPattern::from_char_classes(&[
            CharClassPattern::from_string("a"),
            CharClassPattern::from_string("b"),
        ]));
        let b_or_a = RegexPattern::Alt(AltPattern::from_char_classes(&[
            CharClassPattern::from_string("b"),
            CharClassPattern::from_string("a"),
        ]));
        assert_eq!(a_or_b.reversed(), a_or_b);
        assert_eq!(a_or_b.reversed(), b_or_a);
        assert_eq!(a_or_b.reversed(), b_or_a.reversed());

        let ab = RegexPattern::Conc(conc(vec![
            mult(cc("a"), Multiplier::PRESET_ONE),
            mult(cc("b"), Multiplier::PRESET_ONE),
        ]));
        let ba = RegexPattern::Conc(conc(vec![
            mult(cc("b"), Multiplier::PRESET_ONE),
            mult(cc("a"), Multiplier::PRESET_ONE),
        ]));
        assert_eq!(ab.reversed(), ba);
        assert_eq!(ba.reversed(), ab);

        let aa = cc("a").multiply_int(2);
        assert_eq!(aa.reversed(), aa);
    }

    #[test]
    fn alt_pattern_produces_good_fsm() {
        let fsm = RegexPattern::Alt(AltPattern::from_char_classes(&[
            CharClassPattern::from_string("a"),
            CharClassPattern::from_string("b"),
        ]))
        .to_fsm(None);
        assert!(fsm.accepts("a"));
        assert!(fsm.accepts("b"));
        assert!(!fsm.accepts("c"));
        assert!(!fsm.accepts("aa"));
        assert!(!fsm.accepts("ab"));
        assert!(!fsm.accepts("ba"));
        assert!(!fsm.accepts("bb"));
        assert!(!fsm.accepts(&format!("a{ANYTHING_ELSE}")));
        assert!(!fsm.accepts(&format!("b{ANYTHING_ELSE}")));
        assert!(!fsm.accepts(&format!("{ANYTHING_ELSE}b")));
        assert!(!fsm.accepts(&format!("{ANYTHING_ELSE}a")));
        assert!(!fsm.accepts(&format!("{ANYTHING_ELSE}{ANYTHING_ELSE}")));
    }

    #[test]
    fn conc_pattern_produces_good_fsm() {
        let fsm = RegexPattern::Conc(conc(vec![
            mult(cc("a"), Multiplier::PRESET_ONE),
            mult(cc_neg("a"), Multiplier::PRESET_ONE),
        ]))
        .to_fsm(None);
        assert_eq!(fsm.states().len(), 3);
        assert!(!fsm.accepts("a"));
        assert!(!fsm.accepts("b"));
        assert!(!fsm.accepts("aa"));
        assert!(fsm.accepts("ab"));
        assert!(fsm.accepts(&format!("a{ANYTHING_ELSE}")));
        assert!(!fsm.accepts("ba"));
        assert!(!fsm.accepts("bb"));
    }

    #[test]
    fn mult_pattern_equality() {
        let a = mult(cc("a"), Multiplier::PRESET_ONE);
        assert_eq!(a, mult(cc("a"), Multiplier::PRESET_ONE));
        assert_ne!(a, mult(cc("b"), Multiplier::PRESET_ONE));
        assert_ne!(a, mult(cc("a"), Multiplier::PRESET_QUESTION));
        assert_ne!(a, mult(cc("a"), Multiplier::new(Some(1), Some(2)).unwrap()));
        assert_ne!(RegexPattern::Mult(a.clone()), cc("a"));
    }

    #[test]
    fn conc_pattern_equality() {
        let a = conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]);
        assert_eq!(a, conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]));
        assert_ne!(a, conc(vec![mult(cc("b"), Multiplier::PRESET_ONE)]));
        assert_ne!(a, conc(vec![mult(cc("a"), Multiplier::PRESET_QUESTION)]));
        assert_ne!(a, conc(vec![mult(cc("a"), Multiplier::new(Some(1), Some(2)).unwrap())]));
        assert_ne!(a, ConcPattern::empty());
    }

    #[test]
    fn nested_patterns_equality() {
        assert_eq!(
            RegexPattern::Alt(alt(vec![
                conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]),
                conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]),
            ])),
            RegexPattern::Alt(alt(vec![conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)])]))
        );
        assert_eq!(
            RegexPattern::Alt(alt(vec![
                conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]),
                conc(vec![mult(cc("b"), Multiplier::PRESET_ONE)]),
            ])),
            RegexPattern::Alt(alt(vec![
                conc(vec![mult(cc("b"), Multiplier::PRESET_ONE)]),
                conc(vec![mult(cc("a"), Multiplier::PRESET_ONE)]),
            ]))
        );
    }

    #[test]
    fn conc_pattern_is_result_of_plus() {
        assert_eq!(
            RegexPattern::parse("ba"),
            Some(RegexPattern::Alt(alt(vec![
                RegexPattern::Mult(mult(cc("b"), Multiplier::PRESET_ONE))
                    .concatenate(&RegexPattern::Mult(mult(cc("a"), Multiplier::PRESET_ONE)))
            ])))
        );
    }

    #[test]
    fn char_class_to_string_works() {
        assert_eq!(parsed_cc_str("[\\t\\r\\n]"), "[\\n\\t\\r]");
        assert_eq!(parsed_cc_str("[^\\t\\r\\n]"), "[^\\n\\t\\r]");
        assert_eq!(parsed_cc_str("[\\t]"), "\\t");
        assert_eq!(parsed_cc_str("[^\\t]"), "[^\\t]");
        assert_eq!(parsed_cc_str("\\w"), "\\w");
        assert_eq!(parsed_cc_str("\\s"), "\\s");
        assert_eq!(parsed_cc_str("\\d"), "\\d");
        assert_eq!(parsed_cc_str("\\W"), "\\W");
        assert_eq!(parsed_cc_str("\\S"), "\\S");
        assert_eq!(parsed_cc_str("\\D"), "\\D");
        assert_eq!(parsed_cc_str("[^\\D]"), "\\d");
        assert_eq!(parsed_cc_str("[^\\d]"), "\\D");
        assert_eq!(parsed_cc_str("[a-z]"), "[a-z]");
        assert_eq!(parsed_cc_str("[^a-z]"), "[^a-z]");
        assert_eq!(parsed_cc_str("[0-9a-zQ]"), "[0-9Qa-z]");
        assert_eq!(parsed_cc_str("[^0-9a-zQ]"), "[^0-9Qa-z]");
        assert_eq!(parsed_cc_str("[\\dABCD]"), "[0-9A-D]");
        assert_eq!(parsed_cc_str("[^\\dABCD]"), "[^0-9A-D]");
        assert_eq!(parsed_cc_str("[\\uFFF1-\\uFFF80-9]"), "[0-9\\uFFF1-\\uFFF8]");
        assert_eq!(parsed_cc_str("[^\\uFFF1-\\uFFF80-9]"), "[^0-9\\uFFF1-\\uFFF8]");
        assert_eq!(parsed_cc_str("."), ".");
        assert_eq!(parsed_cc_str("\\["), "\\[");
        assert_eq!(parsed_cc_str("\\xFF"), "\\xFF");
        assert_eq!(CharClassPattern::from_string("").to_string(), "");
    }
}
