//! Finite-state machine engine (port of `Fsm.scala`).
//!
//! A deterministic finite automaton over `char` symbols. The `crawl` primitive builds new DFAs
//! from a hashable meta-state type; all of `reversed`, `everything_but`, `times`, `star`,
//! `parallel` and `concatenate` are constructed on top of it. `reduced` is Brzozowski double
//! reversal (minimization).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::errors::RegexError;

/// A surrogate symbol representing "any symbol not in the official alphabet" (Scala
/// `Fsm.anythingElse`). Unicode private-use char `U+E000`.
pub const ANYTHING_ELSE: char = '\u{E000}';

/// Sort an alphabet ascending, forcing `ANYTHING_ELSE` last (Scala `Fsm.sortAlphabet`).
fn sort_alphabet(alphabet: &BTreeSet<char>) -> Vec<char> {
    let mut v: Vec<char> = alphabet
        .iter()
        .copied()
        .filter(|&c| c != ANYTHING_ELSE)
        .collect();
    if alphabet.contains(&ANYTHING_ELSE) {
        v.push(ANYTHING_ELSE);
    }
    v
}

/// A finite state machine (port of the Scala `Fsm` case class).
#[derive(Clone, Debug)]
pub struct Fsm {
    alphabet: BTreeSet<char>,
    states: BTreeSet<i32>,
    initial_state: i32,
    final_states: BTreeSet<i32>,
    transitions: BTreeMap<i32, BTreeMap<char, i32>>,
}

impl Fsm {
    /// Validate and construct an FSM (port of the Scala constructor `require` checks).
    pub fn new(
        alphabet: BTreeSet<char>,
        states: BTreeSet<i32>,
        initial_state: i32,
        final_states: BTreeSet<i32>,
        transitions: BTreeMap<i32, BTreeMap<char, i32>>,
    ) -> Result<Fsm, RegexError> {
        if !states.contains(&initial_state) {
            return Err(RegexError::InvalidArgument(format!(
                "Initial state {initial_state} must be one of states"
            )));
        }
        if !final_states.iter().all(|f| states.contains(f)) {
            return Err(RegexError::InvalidArgument(
                "Final states must be a subset of states".to_string(),
            ));
        }
        for (&map_entry, transition_entry) in &transitions {
            if !states.contains(&map_entry) {
                return Err(RegexError::InvalidArgument(format!(
                    "Map state {map_entry} must be one of states"
                )));
            }
            for (&symbol, &next_state) in transition_entry {
                if !alphabet.contains(&symbol) {
                    return Err(RegexError::InvalidArgument(format!(
                        "Transition symbol {symbol} -> {next_state} must be one of alphabet"
                    )));
                }
                if !states.contains(&next_state) {
                    return Err(RegexError::InvalidArgument(format!(
                        "Transition state {symbol} -> {next_state} must be one of states"
                    )));
                }
            }
        }
        Ok(Fsm {
            alphabet,
            states,
            initial_state,
            final_states,
            transitions,
        })
    }

    /// Construct without re-validating (internal; invariants are known to hold).
    pub(crate) fn new_unchecked(
        alphabet: BTreeSet<char>,
        states: BTreeSet<i32>,
        initial_state: i32,
        final_states: BTreeSet<i32>,
        transitions: BTreeMap<i32, BTreeMap<char, i32>>,
    ) -> Fsm {
        Fsm {
            alphabet,
            states,
            initial_state,
            final_states,
            transitions,
        }
    }

    pub fn alphabet(&self) -> &BTreeSet<char> {
        &self.alphabet
    }

    pub fn states(&self) -> &BTreeSet<i32> {
        &self.states
    }

    pub fn initial_state(&self) -> i32 {
        self.initial_state
    }

    pub fn final_states(&self) -> &BTreeSet<i32> {
        &self.final_states
    }

    pub fn transitions(&self) -> &BTreeMap<i32, BTreeMap<char, i32>> {
        &self.transitions
    }

    /// An FSM accepting nothing (not even the empty string).
    pub fn null_fsm(alphabet: BTreeSet<char>) -> Fsm {
        let transitions: BTreeMap<i32, BTreeMap<char, i32>> = [(0, {
            let mut m = BTreeMap::new();
            for c in &alphabet {
                m.insert(*c, 0);
            }
            m
        })]
        .into_iter()
        .collect();
        Fsm::new_unchecked(alphabet, BTreeSet::from([0]), 0, BTreeSet::new(), transitions)
    }

    /// An FSM matching the empty string only.
    pub fn epsilon_fsm(alphabet: BTreeSet<char>) -> Fsm {
        Fsm::new_unchecked(
            alphabet,
            BTreeSet::from([0]),
            0,
            BTreeSet::from([0]),
            BTreeMap::new(),
        )
    }

    /// Crawl an unknown FSM from a meta-state, assigning dense state indices (Scala `Fsm.crawl`).
    fn crawl<T: Clone + PartialEq>(
        alphabet: &BTreeSet<char>,
        initial: T,
        is_final: impl Fn(&T) -> bool,
        follow: impl Fn(&T, char) -> Option<T>,
    ) -> Fsm {
        let mut states: Vec<T> = vec![initial];
        let mut transitions: BTreeMap<i32, BTreeMap<char, i32>> = BTreeMap::new();
        let sorted_alphabet = sort_alphabet(alphabet);

        let mut finals: BTreeSet<i32> = BTreeSet::new();
        let mut idx = 0usize;
        while idx < states.len() {
            let current = states[idx].clone();
            if is_final(&current) {
                finals.insert(idx as i32);
            }
            let mut current_map: BTreeMap<char, i32> = BTreeMap::new();
            for &symbol in &sorted_alphabet {
                if let Some(next) = follow(&current, symbol) {
                    let next_idx = match states.iter().position(|s| *s == next) {
                        Some(i) => i,
                        None => {
                            states.push(next);
                            states.len() - 1
                        }
                    };
                    current_map.insert(symbol, next_idx as i32);
                }
            }
            transitions.insert(idx as i32, current_map);
            idx += 1;
        }

        Fsm::new_unchecked(
            alphabet.clone(),
            (0..states.len() as i32).collect(),
            0,
            finals,
            transitions,
        )
    }

    /// Crawl several FSMs in parallel (product construction) with a finality combiner.
    pub fn parallel(finality_test: impl Fn(&[bool]) -> bool, fsms: &[Fsm]) -> Fsm {
        let alphabet: BTreeSet<char> = fsms.iter().flat_map(|f| f.alphabet.iter().copied()).collect();
        let initial: BTreeMap<i32, i32> = fsms
            .iter()
            .enumerate()
            .map(|(i, f)| (i as i32, f.initial_state))
            .collect();

        let follow = |current: &BTreeMap<i32, i32>, symbol: char| -> Option<BTreeMap<i32, i32>> {
            let mut next = BTreeMap::new();
            for (idx, fsm) in fsms.iter().enumerate() {
                if let Some(&fsm_state) = current.get(&(idx as i32)) {
                    if let Some(next_state) = fsm.next_state(fsm_state, symbol) {
                        next.insert(idx as i32, next_state);
                    }
                }
            }
            if next.is_empty() {
                None
            } else {
                Some(next)
            }
        };

        let is_final = |fsm_states: &BTreeMap<i32, i32>| -> bool {
            let finality: Vec<bool> = fsms
                .iter()
                .enumerate()
                .map(|(i, fsm)| {
                    fsm_states
                        .get(&(i as i32))
                        .map_or(false, |&s| fsm.final_states.contains(&s))
                })
                .collect();
            finality_test(&finality)
        };

        Fsm::crawl(&alphabet, initial, is_final, follow).reduced()
    }

    /// Union of several FSMs.
    pub fn union_many(fsms: &[Fsm]) -> Fsm {
        Fsm::parallel(|finality| finality.iter().any(|x| *x), fsms)
    }

    /// Intersection of several FSMs.
    pub fn intersection_many(fsms: &[Fsm]) -> Fsm {
        Fsm::parallel(|finality| finality.iter().all(|x| *x), fsms)
    }

    /// Difference: strings recognised by the first FSM but none of the others.
    pub fn difference_many(fsms: &[Fsm]) -> Fsm {
        Fsm::parallel(
            |finality| {
                finality
                    .first()
                    .copied()
                    .unwrap_or(false)
                    && !finality.iter().skip(1).all(|x| *x)
            },
            fsms,
        )
    }

    /// Symmetric difference of several FSMs.
    pub fn symmetric_difference_many(fsms: &[Fsm]) -> Fsm {
        Fsm::parallel(|finality| finality.iter().filter(|x| **x).count() % 2 == 1, fsms)
    }

    /// Concatenate arbitrarily many FSMs.
    pub fn concatenate_many(fsms: &[Fsm]) -> Fsm {
        let alphabet: BTreeSet<char> = fsms.iter().flat_map(|f| f.alphabet.iter().copied()).collect();

        let is_final = |states_set: &BTreeSet<(i32, i32)>| -> bool {
            let last = fsms.len() as i32 - 1;
            states_set.iter().any(|&(fsm_index, fsm_state)| {
                fsm_index == last && fsms[fsm_index as usize].final_states.contains(&fsm_state)
            })
        };

        let initial_states = if fsms.is_empty() {
            BTreeSet::new()
        } else {
            connect_all(fsms, 0, fsms[0].initial_state)
        };

        let follow =
            |current_states: &BTreeSet<(i32, i32)>, symbol: char| -> Option<BTreeSet<(i32, i32)>> {
                let mut next_states = BTreeSet::new();
                for &(fsm_index, fsm_state) in current_states {
                    if let Some(next_state) = fsms[fsm_index as usize].next_state(fsm_state, symbol)
                    {
                        next_states.extend(connect_all(fsms, fsm_index, next_state));
                    }
                }
                if next_states.is_empty() {
                    None
                } else {
                    Some(next_states)
                }
            };

        Fsm::crawl(&alphabet, initial_states, is_final, follow).reduced()
    }

    /// True if the alphabet contains `ANYTHING_ELSE` (accepts unknown chars).
    pub fn has_anything_else(&self) -> bool {
        self.alphabet.contains(&ANYTHING_ELSE)
    }

    /// A state is "live" if a final state is reachable from it.
    pub fn is_live(&self, state: i32) -> bool {
        let mut reachable = VecDeque::from([state]);
        let mut checked = BTreeSet::from([state]);
        while let Some(s) = reachable.pop_front() {
            if self.final_states.contains(&s) {
                return true;
            }
            if let Some(trans) = self.transitions.get(&s) {
                for &next in trans.values() {
                    if !checked.contains(&next) {
                        checked.insert(next);
                        reachable.push_back(next);
                    }
                }
            }
        }
        false
    }

    /// Test whether this FSM accepts `input`.
    pub fn accepts(&self, input: &str) -> bool {
        let mut current_state = self.initial_state;
        for c in input.chars() {
            let sym = if self.has_anything_else() && !self.alphabet.contains(&c) {
                ANYTHING_ELSE
            } else {
                c
            };
            match self.next_state(current_state, sym) {
                None => return false,
                Some(next) => current_state = next,
            }
        }
        self.final_states.contains(&current_state)
    }

    /// Brzozowski derivative with respect to `input`.
    pub fn derive(&self, input: &str) -> Result<Fsm, RegexError> {
        let mut current_state = self.initial_state;
        for c in input.chars() {
            let sym = if self.alphabet.contains(&c) {
                c
            } else if self.has_anything_else() {
                ANYTHING_ELSE
            } else {
                return Err(RegexError::SymbolOutOfAlphabet(c));
            };
            match self.next_state(current_state, sym) {
                Some(next) => current_state = next,
                None => return Ok(Fsm::null_fsm(self.alphabet.clone())),
            }
        }
        Ok(Fsm::new_unchecked(
            self.alphabet.clone(),
            self.states.clone(),
            current_state,
            self.final_states.clone(),
            self.transitions.clone(),
        ))
    }

    /// The reversed FSM (accepts reversed strings).
    pub fn reversed(&self) -> Fsm {
        let res_alphabet = self.alphabet.clone();
        let res_initials = self.final_states.clone();

        let follow = |current_states: &BTreeSet<i32>, symbol: char| -> Option<BTreeSet<i32>> {
            let mut next = BTreeSet::new();
            for (&transition_state, transition_map) in &self.transitions {
                for &current_state in current_states {
                    if transition_map.get(&symbol) == Some(&current_state) {
                        next.insert(transition_state);
                    }
                }
            }
            if next.is_empty() {
                None
            } else {
                Some(next)
            }
        };

        let is_final = |states_set: &BTreeSet<i32>| states_set.contains(&self.initial_state);

        Fsm::crawl(&res_alphabet, res_initials, is_final, follow)
    }

    /// The complement FSM (accepts any string `self` does not).
    pub fn everything_but(&self) -> Fsm {
        let res_initial = vec![self.initial_state];

        let follow = |current_state: &Vec<i32>, symbol: char| -> Option<Vec<i32>> {
            let next = current_state
                .first()
                .and_then(|&head| self.next_state(head, symbol))
                .map(|s| vec![s])
                .unwrap_or_default();
            Some(next)
        };

        let is_final = |states_set: &Vec<i32>| {
            !states_set
                .first()
                .map_or(false, |&x| self.final_states.contains(&x))
        };

        Fsm::crawl(&self.alphabet, res_initial, is_final, follow).reduced()
    }

    /// Lazily enumerate accepted strings, sorted by length then lexicographically.
    pub fn strings(&self) -> StringsIter {
        StringsIter::new(self.clone())
    }

    /// Multiply this FSM by a non-negative integer (repeat `multiplier` times).
    pub fn times(&self, multiplier: i32) -> Result<Fsm, RegexError> {
        if multiplier < 0 {
            return Err(RegexError::InvalidArgument(format!(
                "Can't multiply an FSM by {multiplier}"
            )));
        }
        Ok(self.times_unchecked(multiplier))
    }

    /// Multiply, assuming `multiplier >= 0`.
    pub(crate) fn times_unchecked(&self, multiplier: i32) -> Fsm {
        let initial: BTreeSet<(i32, i32)> = BTreeSet::from([(self.initial_state, 0)]);

        let follow = |crawl_state: &BTreeSet<(i32, i32)>,
                      symbol: char|
         -> Option<BTreeSet<(i32, i32)>> {
            let mut next = BTreeSet::new();
            for &(fsm_state, iteration) in crawl_state {
                if iteration < multiplier {
                    if let Some(sub_state) = self.next_state(fsm_state, symbol) {
                        if self.final_states.contains(&sub_state) {
                            next.insert((sub_state, iteration));
                            next.insert((self.initial_state, iteration + 1));
                        } else {
                            next.insert((sub_state, iteration));
                        }
                    }
                }
            }
            if next.is_empty() {
                None
            } else {
                Some(next)
            }
        };

        let is_final = |crawl_state: &BTreeSet<(i32, i32)>| {
            crawl_state.iter().any(|&(fsm_state, iteration)| {
                fsm_state == self.initial_state
                    && (self.final_states.contains(&self.initial_state) || iteration == multiplier)
            })
        };

        Fsm::crawl(&self.alphabet, initial, is_final, follow).reduced()
    }

    /// Next state for `(state, symbol)`, if defined.
    pub fn next_state(&self, state: i32, symbol: char) -> Option<i32> {
        self.transitions
            .get(&state)
            .and_then(|m| m.get(&symbol))
            .copied()
    }

    /// Kleene star closure (0 or more repetitions).
    pub fn star(&self) -> Fsm {
        let follow = |sub_states: &BTreeSet<i32>, symbol: char| -> Option<BTreeSet<i32>> {
            let mut next = BTreeSet::new();
            for &sub_state in sub_states {
                if self.final_states.contains(&sub_state) {
                    if let Some(s) = self.next_state(sub_state, symbol) {
                        next.insert(s);
                    }
                    if let Some(s) = self.next_state(self.initial_state, symbol) {
                        next.insert(s);
                    }
                } else if let Some(s) = self.next_state(sub_state, symbol) {
                    next.insert(s);
                }
            }
            if next.is_empty() {
                None
            } else {
                Some(next)
            }
        };

        let is_final = |sub_states: &BTreeSet<i32>| {
            sub_states
                .iter()
                .any(|s| self.final_states.contains(s))
        };

        Fsm::crawl(
            &self.alphabet,
            BTreeSet::from([self.initial_state]),
            is_final,
            follow,
        )
        .union(&Fsm::epsilon_fsm(self.alphabet.clone()))
    }

    /// Number of accepted strings, or `None` if infinite.
    pub fn cardinality(&self) -> Option<i32> {
        let mut memo: BTreeMap<i32, Option<i32>> = BTreeMap::new();
        count_strings(self, &mut memo, self.initial_state)
    }

    /// Whether this FSM's strings are a subset of `that`'s.
    pub fn is_subset(&self, that: &Fsm) -> bool {
        self.difference(that).is_empty()
    }

    pub fn is_strict_subset(&self, that: &Fsm) -> bool {
        self.difference(that).is_empty() && !self.equivalent(that)
    }

    pub fn is_superset(&self, that: &Fsm) -> bool {
        that.difference(self).is_empty()
    }

    pub fn is_strict_superset(&self, that: &Fsm) -> bool {
        that.difference(self).is_empty() && !self.equivalent(that)
    }

    /// Alias for `cardinality`.
    pub fn length(&self) -> Option<i32> {
        self.cardinality()
    }

    pub fn symmetric_difference(&self, that: &Fsm) -> Fsm {
        Fsm::symmetric_difference_many(&[self.clone(), that.clone()])
    }

    pub fn difference(&self, that: &Fsm) -> Fsm {
        Fsm::difference_many(&[self.clone(), that.clone()])
    }

    pub fn intersection(&self, that: &Fsm) -> Fsm {
        Fsm::intersection_many(&[self.clone(), that.clone()])
    }

    pub fn union(&self, that: &Fsm) -> Fsm {
        Fsm::union_many(&[self.clone(), that.clone()])
    }

    pub fn concatenate(&self, that: &Fsm) -> Fsm {
        Fsm::concatenate_many(&[self.clone(), that.clone()])
    }

    pub fn is_disjoint(&self, that: &Fsm) -> bool {
        self.intersection(that).is_empty()
    }

    pub fn is_empty(&self) -> bool {
        !self.is_live(self.initial_state)
    }

    pub fn equivalent(&self, that: &Fsm) -> bool {
        self.symmetric_difference(that).is_empty()
    }

    pub fn different(&self, that: &Fsm) -> bool {
        !self.equivalent(that)
    }

    /// Minimized FSM (Brzozowski double reversal).
    pub fn reduced(&self) -> Fsm {
        self.reversed().reversed()
    }

    /// Alias for `accepts`.
    pub fn contains(&self, s: &str) -> bool {
        self.accepts(s)
    }
}

impl PartialEq for Fsm {
    fn eq(&self, other: &Fsm) -> bool {
        self.equivalent(other)
    }
}

impl Eq for Fsm {}

/// A state set plus the next FSM's initial state (and so on) while final (Scala `connectAll`).
fn connect_all(fsms: &[Fsm], fsm_idx: i32, sub_state: i32) -> BTreeSet<(i32, i32)> {
    let mut result = BTreeSet::new();
    let mut current_fsm_idx = fsm_idx;
    let mut current_state = sub_state;
    result.insert((current_fsm_idx, current_state));
    while current_fsm_idx < (fsms.len() as i32 - 1)
        && fsms[current_fsm_idx as usize]
            .final_states
            .contains(&current_state)
    {
        current_fsm_idx += 1;
        current_state = fsms[current_fsm_idx as usize].initial_state;
        result.insert((current_fsm_idx, current_state));
    }
    result
}

/// Recursive string counter with cycle detection (Scala `Fsm.cardinality`'s `getNumStrings`).
fn count_strings(fsm: &Fsm, memo: &mut BTreeMap<i32, Option<i32>>, state: i32) -> Option<i32> {
    if !fsm.is_live(state) {
        memo.insert(state, Some(0));
        return Some(0);
    }
    if let Some(&count) = memo.get(&state) {
        // Some(n) => already computed; None => currently computing => cycle.
        return count;
    }
    memo.insert(state, None);
    let mut n = if fsm.final_states.contains(&state) { 1 } else { 0 };
    if let Some(trans) = fsm.transitions.get(&state) {
        for &next_state in trans.values() {
            n += count_strings(fsm, memo, next_state)?;
        }
    }
    memo.insert(state, Some(n));
    Some(n)
}

/// A lazy breadth-first enumerator of accepted strings (port of `Fsm.strings`).
pub struct StringsIter {
    fsm: Fsm,
    livestates: BTreeSet<i32>,
    pending: VecDeque<(String, i32)>,
    pending_finals: VecDeque<String>,
    need_initial: bool,
}

impl StringsIter {
    fn new(fsm: Fsm) -> Self {
        let livestates: BTreeSet<i32> = fsm.states.iter().copied().filter(|&s| fsm.is_live(s)).collect();
        StringsIter {
            fsm,
            livestates,
            pending: VecDeque::new(),
            pending_finals: VecDeque::new(),
            need_initial: true,
        }
    }
}

impl Iterator for StringsIter {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if let Some(s) = self.pending_finals.pop_front() {
            return Some(s);
        }

        if self.need_initial {
            self.need_initial = false;
            let cstate = self.fsm.initial_state;
            if self.livestates.contains(&cstate) {
                self.pending.push_back((String::new(), cstate));
                if self.fsm.final_states.contains(&cstate) {
                    self.pending_finals.push_back(String::new());
                }
            }
        }

        while self.pending_finals.is_empty() {
            let Some((pending_string, pending_state)) = self.pending.pop_front() else {
                break;
            };
            if let Some(trans) = self.fsm.transitions.get(&pending_state) {
                for (&next_symbol, &next_state) in trans {
                    let mut next_string = pending_string.clone();
                    next_string.push(next_symbol);
                    if self.livestates.contains(&next_state) {
                        self.pending.push_back((next_string.clone(), next_state));
                        if self.fsm.final_states.contains(&next_state) {
                            self.pending_finals.push_back(next_string);
                        }
                    }
                }
            }
        }

        self.pending_finals.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn cs(chars: &[char]) -> BTreeSet<char> {
        chars.iter().copied().collect()
    }

    fn trans(entries: &[(i32, &[(char, i32)])]) -> BTreeMap<i32, BTreeMap<char, i32>> {
        entries
            .iter()
            .map(|(k, v)| (*k, v.iter().copied().collect()))
            .collect()
    }

    fn fsm(
        alphabet: BTreeSet<char>,
        states: BTreeSet<i32>,
        initial: i32,
        finals: BTreeSet<i32>,
        transitions: BTreeMap<i32, BTreeMap<char, i32>>,
    ) -> Fsm {
        Fsm::new(alphabet, states, initial, finals, transitions).unwrap()
    }

    const OB: i32 = -5;

    fn create_fsm_a() -> Fsm {
        fsm(
            cs(&['a', 'b']),
            BTreeSet::from([0, 1, OB]),
            0,
            BTreeSet::from([1]),
            trans(&[
                (0, &[('a', 1), ('b', OB)]),
                (1, &[('a', OB), ('b', OB)]),
                (OB, &[('a', OB), ('b', OB)]),
            ]),
        )
    }

    fn create_fsm_b() -> Fsm {
        fsm(
            cs(&['a', 'b']),
            BTreeSet::from([0, 1, OB]),
            0,
            BTreeSet::from([1]),
            trans(&[
                (0, &[('a', OB), ('b', 1)]),
                (1, &[('a', OB), ('b', OB)]),
                (OB, &[('a', OB), ('b', OB)]),
            ]),
        )
    }

    fn create_fsm_abc() -> Fsm {
        fsm(
            cs(&['a', 'b', 'c']),
            BTreeSet::from([0, 1, 2, 3, OB]),
            0,
            BTreeSet::from([3]),
            trans(&[
                (0, &[('a', 1), ('b', OB), ('c', OB)]),
                (1, &[('a', OB), ('b', 2), ('c', OB)]),
                (2, &[('a', OB), ('b', OB), ('c', 3)]),
                (3, &[('a', OB), ('b', OB), ('c', OB)]),
            ]),
        )
    }

    fn create_brzozowski() -> Fsm {
        fsm(
            cs(&['a', 'b']),
            BTreeSet::from([0, 1, 2, 3, 4]),
            0,
            BTreeSet::from([2, 4]),
            trans(&[
                (0, &[('a', 1), ('b', 3)]),
                (1, &[('a', 2), ('b', 4)]),
                (2, &[('a', 2), ('b', 4)]),
                (3, &[('a', 1), ('b', 3)]),
                (4, &[('a', 1), ('b', 3)]),
            ]),
        )
    }

    fn strs(f: &Fsm) -> Vec<String> {
        f.strings().collect()
    }

    #[test]
    fn single_char_fsms_pass_simple_check() {
        let fsm_a = create_fsm_a();
        assert!(!fsm_a.is_empty());
        assert!(!fsm_a.accepts(""));
        assert!(!fsm_a.accepts("b"));
        assert!(fsm_a.accepts("a"));

        let fsm_b = create_fsm_b();
        assert!(!fsm_b.is_empty());
        assert!(!fsm_b.accepts(""));
        assert!(fsm_b.accepts("b"));
        assert!(!fsm_b.accepts("a"));
    }

    #[test]
    fn epsilon_fsm_is_empty() {
        let epsilon_a = Fsm::epsilon_fsm(cs(&['a']));
        assert!(epsilon_a.accepts(""));
        assert!(!epsilon_a.accepts("a"));

        let epsilon_ab = Fsm::epsilon_fsm(cs(&['a', 'b']));
        assert!(!epsilon_ab.is_empty());

        assert!(fsm(BTreeSet::new(), BTreeSet::from([0, 1]), 0, BTreeSet::from([1]), trans(&[(0, &[]), (1, &[])])).is_empty());
        assert!(!fsm(BTreeSet::new(), BTreeSet::from([0]), 0, BTreeSet::from([0]), trans(&[(0, &[])])).is_empty());
        assert!(fsm(BTreeSet::new(), BTreeSet::from([0, 1]), 1, BTreeSet::from([0]), trans(&[(0, &[])])).is_empty());
    }

    #[test]
    fn null_fsm_behaves_as_expected() {
        let null_fsm = Fsm::null_fsm(cs(&['a']));
        assert!(!null_fsm.accepts("a"));
        assert!(!null_fsm.accepts(""));
    }

    #[test]
    fn is_live_succeeds_for_fsm_a() {
        let fsm_a = create_fsm_a();
        assert!(fsm_a.is_live(0));
        assert!(fsm_a.is_live(1));
        assert!(!fsm_a.is_live(OB));
    }

    #[test]
    fn is_empty_passes_basic_check() {
        assert!(!create_fsm_a().is_empty());
        assert!(!create_fsm_b().is_empty());
        assert!(fsm(BTreeSet::new(), BTreeSet::from([0, 1]), 0, BTreeSet::from([1]), trans(&[(0, &[]), (1, &[])])).is_empty());
        assert!(!fsm(BTreeSet::new(), BTreeSet::from([0]), 0, BTreeSet::from([0]), trans(&[(0, &[])])).is_empty());
    }

    #[test]
    fn fsm_abc_accepts_abc() {
        assert!(create_fsm_abc().accepts("abc"));
    }

    #[test]
    fn fsm_is_reversible() {
        let fsm_abc = create_fsm_abc();
        assert!(fsm_abc.accepts("abc"));
        assert!(fsm_abc.reversed().accepts("cba"));
    }

    #[test]
    fn brzozowski_is_reversible() {
        let fsm_br = create_brzozowski();
        assert!(fsm_br.accepts("aa"));
        assert!(fsm_br.accepts("ab"));
        assert!(fsm_br.accepts("aab"));
        assert!(fsm_br.accepts("bab"));
        assert!(fsm_br.accepts("abbbbbbbab"));
        assert!(!fsm_br.accepts(""));
        assert!(!fsm_br.accepts("a"));
        assert!(!fsm_br.accepts("b"));
        assert!(!fsm_br.accepts("ba"));
        assert!(!fsm_br.accepts("bb"));

        let rev = fsm_br.reversed();
        assert!(rev.accepts("aa"));
        assert!(rev.accepts("ba"));
        assert!(rev.accepts("baa"));
        assert!(rev.accepts("bab"));
        assert!(rev.accepts("babbbbbbba"));
        assert!(!rev.accepts(""));
        assert!(!rev.accepts("a"));
        assert!(!rev.accepts("b"));
        assert!(!rev.accepts("ab"));
        assert!(!rev.accepts("bb"));
    }

    #[test]
    fn reversed_epsilon_stays_epsilon() {
        assert!(Fsm::epsilon_fsm(cs(&['a'])).reversed().accepts(""));
    }

    #[test]
    fn fsm_is_inversible() {
        let not_a = create_fsm_a().everything_but();
        assert!(not_a.accepts(""));
        assert!(!not_a.accepts("a"));
        assert!(not_a.accepts("b"));
        assert!(not_a.accepts("aa"));
        assert!(not_a.accepts("ab"));
    }

    #[test]
    fn anything_else_is_accepted() {
        let fsm = fsm(
            cs(&['a', 'b', 'c', ANYTHING_ELSE]),
            BTreeSet::from([1]),
            1,
            BTreeSet::from([1]),
            trans(&[(1, &[('a', 1), ('b', 1), ('c', 1), (ANYTHING_ELSE, 1)])]),
        );
        assert!(fsm.accepts("a"));
        assert!(fsm.accepts("b"));
        assert!(fsm.accepts("c"));
        assert!(fsm.accepts("d"));
    }

    #[test]
    fn crawl_reduction_resolves_duplication() {
        let merged = fsm(
            cs(&['0', '1']),
            BTreeSet::from([1, 2, 3, 4, OB]),
            1,
            BTreeSet::from([4]),
            trans(&[
                (1, &[('0', 2), ('1', 4)]),
                (2, &[('0', 3), ('1', 4)]),
                (3, &[('0', 3), ('1', 4)]),
                (4, &[('0', OB), ('1', OB)]),
                (OB, &[('0', OB), ('1', OB)]),
            ]),
        );
        assert_eq!(merged.reversed().states().len(), 2);
    }

    #[test]
    fn cardinality_calculates_length() {
        assert_eq!(create_fsm_a().cardinality(), Some(1));
        assert_eq!(create_fsm_abc().cardinality(), Some(1));
        assert_eq!(Fsm::null_fsm(cs(&['a'])).cardinality(), Some(0));
        assert_eq!(Fsm::epsilon_fsm(cs(&['a'])).cardinality(), Some(1));
        assert_eq!(create_brzozowski().cardinality(), None);
    }

    #[test]
    fn reduce_removes_unreachable_states() {
        let fsm = fsm(
            cs(&['a']),
            BTreeSet::from([0, 1, 2]),
            0,
            BTreeSet::from([1]),
            trans(&[(0, &[('a', 2)]), (1, &[('a', 2)]), (2, &[('a', 2)])]),
        );
        assert!(fsm.is_empty());
        assert!(!fsm.accepts("a"));
        let reduced = fsm.reduced();
        assert_eq!(reduced.states().len(), 1);
        assert!(reduced.is_empty());
    }

    #[test]
    fn concatenate_passes_basic_check() {
        assert_eq!(Fsm::concatenate_many(&[]).strings().collect::<Vec<_>>(), Vec::<String>::new());

        let fsm_a = create_fsm_a();
        assert_eq!(Fsm::concatenate_many(&[fsm_a.clone(), fsm_a.clone(), fsm_a.clone()]).strings().collect::<Vec<_>>(), vec!["aaa".to_string()]);
        assert_eq!(Fsm::concatenate_many(&[fsm_a.clone()]).strings().collect::<Vec<_>>(), vec!["a".to_string()]);

        let fsm_bab = Fsm::concatenate_many(&[create_fsm_b(), create_fsm_a(), create_fsm_b()]);
        assert_eq!(fsm_bab.strings().collect::<Vec<_>>(), vec!["bab".to_string()]);

        let fsm_aa = fsm_a.concatenate(&create_fsm_a());
        assert!(!fsm_aa.accepts(""));
        assert!(!fsm_aa.accepts("a"));
        assert!(fsm_aa.accepts("aa"));
        assert!(!fsm_aa.accepts("aaa"));

        let fsm_ab = create_fsm_a().concatenate(&create_fsm_b());
        assert!(!fsm_ab.accepts(""));
        assert!(!fsm_ab.accepts("a"));
        assert!(!fsm_ab.accepts("b"));
        assert!(!fsm_ab.accepts("aa"));
        assert!(!fsm_ab.accepts("bb"));
        assert!(fsm_ab.accepts("ab"));
        assert!(!fsm_ab.accepts("ba"));
    }

    #[test]
    fn concatenate_with_epsilon_has_no_defect() {
        let fsm_a = create_fsm_a();
        let eps = Fsm::epsilon_fsm(cs(&['a', 'b']));

        let aea = Fsm::concatenate_many(&[fsm_a.clone(), eps.clone(), fsm_a.clone()]);
        assert!(!aea.accepts(""));
        assert!(!aea.accepts("a"));
        assert!(aea.accepts("aa"));
        assert!(!aea.accepts("aaa"));

        let aeea = Fsm::concatenate_many(&[fsm_a.clone(), eps.clone(), eps.clone(), fsm_a.clone()]);
        assert!(!aeea.accepts(""));
        assert!(!aeea.accepts("a"));
        assert!(aeea.accepts("aa"));
        assert!(!aeea.accepts("aaa"));

        let eeaa = Fsm::concatenate_many(&[eps.clone(), eps.clone(), fsm_a.clone(), fsm_a.clone()]);
        assert!(!eeaa.accepts(""));
        assert!(!eeaa.accepts("a"));
        assert!(eeaa.accepts("aa"));
        assert!(!eeaa.accepts("aaa"));
    }

    #[test]
    fn concatenate_bc_star_c_works() {
        let fsm1 = fsm(
            cs(&['a', 'b', 'c', ANYTHING_ELSE]),
            BTreeSet::from([0, 1]),
            1,
            BTreeSet::from([1]),
            trans(&[
                (0, &[(ANYTHING_ELSE, 0), ('a', 0), ('b', 0), ('c', 0)]),
                (1, &[(ANYTHING_ELSE, 0), ('a', 0), ('b', 1), ('c', 1)]),
            ]),
        );
        assert!(fsm1.accepts(""));

        let fsm2 = fsm(
            cs(&['a', 'b', 'c', ANYTHING_ELSE]),
            BTreeSet::from([0, 1, 2]),
            1,
            BTreeSet::from([0]),
            trans(&[
                (0, &[(ANYTHING_ELSE, 2), ('a', 2), ('b', 2), ('c', 2)]),
                (1, &[(ANYTHING_ELSE, 2), ('a', 2), ('b', 2), ('c', 0)]),
                (2, &[(ANYTHING_ELSE, 2), ('a', 2), ('b', 2), ('c', 2)]),
            ]),
        );
        assert!(fsm2.accepts("c"));
        assert!(fsm1.concatenate(&fsm2).accepts("c"));
    }

    #[test]
    fn disagreeing_alphabets_have_valid_unions() {
        let fsm_a = fsm(cs(&['a']), BTreeSet::from([0, 1]), 0, BTreeSet::from([1]), trans(&[(0, &[('a', 1)])]));
        let fsm_b = fsm(cs(&['b']), BTreeSet::from([0, 1]), 0, BTreeSet::from([1]), trans(&[(0, &[('b', 1)])]));

        assert!(fsm_a.union(&fsm_b).accepts("a"));
        assert!(fsm_a.union(&fsm_b).accepts("b"));
        assert!(fsm_a.intersection(&fsm_b).is_empty());
        assert!(fsm_a.concatenate(&fsm_b).accepts("ab"));
        assert!(fsm_a.symmetric_difference(&fsm_b).accepts("a"));
        assert!(fsm_a.symmetric_difference(&fsm_b).accepts("b"));
    }

    #[test]
    fn star_passes_basic_check() {
        let star_a = create_fsm_a().star();
        assert!(star_a.accepts(""));
        assert!(star_a.accepts("a"));
        assert!(!star_a.accepts("b"));
        assert!(star_a.accepts("aaaaaaaaa"));
    }

    #[test]
    fn star_passes_advanced_check() {
        let fsm_s = fsm(
            cs(&['a', 'b']),
            BTreeSet::from([0, 1, 2, OB]),
            0,
            BTreeSet::from([2]),
            trans(&[
                (0, &[('a', 0), ('b', 1)]),
                (1, &[('a', 2), ('b', OB)]),
                (2, &[('a', OB), ('b', OB)]),
                (OB, &[('a', OB), ('b', OB)]),
            ]),
        )
        .star();

        assert_eq!(fsm_s.alphabet(), &cs(&['a', 'b']));
        assert!(fsm_s.accepts(""));
        assert!(!fsm_s.accepts("a"));
        assert!(!fsm_s.accepts("b"));
        assert!(!fsm_s.accepts("aa"));
        assert!(!fsm_s.accepts("aabb"));
        assert!(fsm_s.accepts("ba"));
        assert!(fsm_s.accepts("aba"));
        assert!(fsm_s.accepts("aaba"));
        assert!(fsm_s.accepts("abababa"));
    }

    #[test]
    fn fsm_ab_star_star_works() {
        let ab_star = fsm(cs(&['a', 'b']), BTreeSet::from([0, 1]), 0, BTreeSet::from([1]), trans(&[(0, &[('a', 1)]), (1, &[('b', 1)])]));
        assert!(ab_star.accepts("a"));
        assert!(!ab_star.accepts("b"));
        assert!(ab_star.accepts("ab"));
        assert!(ab_star.accepts("abb"));

        let abstarstar = ab_star.star();
        assert!(abstarstar.accepts("a"));
        assert!(!abstarstar.accepts("b"));
        assert!(abstarstar.accepts("ab"));
        assert!(!abstarstar.accepts("bb"));
    }

    #[test]
    fn derive_passes_basic_check() {
        let fsm_a = create_fsm_a();
        assert_eq!(fsm_a.derive("a").unwrap(), Fsm::epsilon_fsm(cs(&['a', 'b'])));
        assert_eq!(fsm_a.derive("b").unwrap(), Fsm::null_fsm(cs(&['a', 'b'])));
        assert!(fsm_a.derive("c").is_err());
        assert_eq!(fsm_a.derive("a").unwrap(), Fsm::epsilon_fsm(cs(&['a', 'b'])));
        assert_eq!(fsm_a.derive("b").unwrap(), Fsm::null_fsm(cs(&['a', 'b'])));
        assert_eq!(
            fsm_a.star().difference(&Fsm::epsilon_fsm(cs(&['a', 'b']))).derive("a").unwrap(),
            fsm_a.star()
        );
        assert_eq!(fsm_a.times(3).unwrap().derive("a").unwrap(), fsm_a.times(2).unwrap());
    }

    #[test]
    fn multiply_fails_on_negative() {
        assert!(create_fsm_a().times(-1).is_err());
    }

    #[test]
    fn multiply_by_zero_one_two_seven() {
        assert!(create_fsm_a().times(0).unwrap().accepts(""));
        assert!(!create_fsm_a().times(0).unwrap().accepts("a"));

        let one = create_fsm_a().times(1).unwrap();
        assert!(!one.accepts(""));
        assert!(one.accepts("a"));
        assert!(!one.accepts("aa"));

        let two = create_fsm_a().times(2).unwrap();
        assert!(!two.accepts(""));
        assert!(!two.accepts("a"));
        assert!(two.accepts("aa"));
        assert!(!two.accepts("aaa"));

        let seven = create_fsm_a().times(7).unwrap();
        assert!(!seven.accepts("aaaaaa"));
        assert!(seven.accepts("aaaaaaa"));
        assert!(!seven.accepts("aaaaaaaa"));
    }

    #[test]
    fn multiply_applied_to_unions() {
        let fsm_ab = create_fsm_a().concatenate(&create_fsm_b());
        let fsm_opt = Fsm::epsilon_fsm(create_fsm_a().alphabet().clone()).union(&fsm_ab);
        assert!(fsm_opt.accepts(""));
        assert!(!fsm_opt.accepts("a"));
        assert!(!fsm_opt.accepts("b"));
        assert!(fsm_opt.accepts("ab"));
        assert!(!fsm_opt.accepts("aa"));

        let fsm_opt2 = fsm_opt.times(2).unwrap();
        assert!(fsm_opt2.accepts(""));
        assert!(!fsm_opt2.accepts("a"));
        assert!(!fsm_opt2.accepts("b"));
        assert!(!fsm_opt2.accepts("aa"));
        assert!(!fsm_opt2.accepts("bb"));
        assert!(!fsm_opt2.accepts("ba"));
        assert!(!fsm_opt2.accepts("aba"));
        assert!(fsm_opt2.accepts("abab"));
        assert!(fsm_opt2.accepts("ab"));
    }

    #[test]
    fn intersection_produces_correct_fsm() {
        let fsm_ab = create_fsm_a().intersection(&create_fsm_b());
        assert!(!fsm_ab.accepts(""));
        assert!(!fsm_ab.accepts("a"));
        assert!(!fsm_ab.accepts("b"));
    }

    #[test]
    fn union_with_null_passes_basic_check() {
        let fsm_a = create_fsm_a().union(&Fsm::null_fsm(cs(&['a', 'b'])));
        assert!(!fsm_a.accepts(""));
        assert!(fsm_a.accepts("a"));
        assert!(!fsm_a.accepts("aa"));
        assert!(!fsm_a.accepts("b"));
    }

    #[test]
    fn union_produces_correct_fsm() {
        let fsm_ab = create_fsm_a().union(&create_fsm_b());
        assert!(!fsm_ab.accepts(""));
        assert!(fsm_ab.accepts("a"));
        assert!(fsm_ab.accepts("b"));
        assert!(!fsm_ab.accepts("aa"));
        assert!(!fsm_ab.accepts("ab"));
        assert!(!fsm_ab.accepts("ba"));
        assert!(!fsm_ab.accepts("bb"));
    }

    #[test]
    fn difference_works() {
        let fsm_aor_b = fsm(
            cs(&['a', 'b']),
            BTreeSet::from([0, 1, OB]),
            0,
            BTreeSet::from([1]),
            trans(&[(0, &[('a', 1), ('b', 1)]), (1, &[('a', OB), ('b', OB)]), (OB, &[('a', OB), ('b', OB)])]),
        );
        let fsm_a = create_fsm_a();
        let fsm_b = create_fsm_b();

        assert_eq!(fsm_a.symmetric_difference(&fsm_a).strings().count(), 0);
        assert_eq!(fsm_b.symmetric_difference(&fsm_b).strings().count(), 0);
        assert_eq!(fsm_a.symmetric_difference(&fsm_b).strings().collect::<Vec<_>>(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(fsm_aor_b.symmetric_difference(&fsm_a).strings().collect::<Vec<_>>(), vec!["b".to_string()]);
        assert_eq!(fsm_aor_b.symmetric_difference(&fsm_b).strings().collect::<Vec<_>>(), vec!["a".to_string()]);
    }

    #[test]
    fn string_generator_generates_all_strings() {
        let check = vec!["aa", "ab", "aaa", "aab", "baa", "bab", "aaaa"];
        let got: Vec<String> = create_brzozowski().strings().take(7).collect();
        assert_eq!(got, check);
    }

    #[test]
    fn reduce_eliminates_unused_states() {
        let fsm3 = fsm(
            cs(&['a']),
            BTreeSet::from([0, 1, 2]),
            0,
            BTreeSet::from([1]),
            trans(&[(0, &[('a', 2)]), (1, &[('a', 2)]), (2, &[('a', 2)])]),
        );
        assert_eq!(fsm3.reduced().states().len(), 1);
    }

    #[test]
    fn equivalent_passes_basic_check() {
        let fsm_ab = create_fsm_a().union(&create_fsm_b());
        let fsm_ba = create_fsm_b().union(&create_fsm_a());
        assert!(fsm_ab.equivalent(&fsm_ba));
        assert_eq!(fsm_ab, fsm_ba);
    }

    #[test]
    fn binary_div_3_works() {
        let init = -2;
        let zero = -1;
        let fsm3 = fsm(
            cs(&['0', '1']),
            BTreeSet::from([init, zero, 0, 1, 2, OB]),
            init,
            BTreeSet::from([zero, 0]),
            trans(&[
                (init, &[('0', zero), ('1', 1)]),
                (zero, &[('0', OB), ('1', OB)]),
                (0, &[('0', 0), ('1', 1)]),
                (1, &[('0', 2), ('1', 0)]),
                (2, &[('0', 1), ('1', 2)]),
                (OB, &[('0', OB), ('1', OB)]),
            ]),
        );

        assert!(!fsm3.accepts(""));
        assert!(fsm3.accepts("0"));
        assert!(!fsm3.accepts("1"));
        assert!(!fsm3.accepts("00"));
        assert!(!fsm3.accepts("01"));
        assert!(!fsm3.accepts("10"));
        assert!(fsm3.accepts("11"));
        assert!(!fsm3.accepts("000"));
        assert!(!fsm3.accepts("001"));
        assert!(!fsm3.accepts("010"));
        assert!(!fsm3.accepts("011"));
        assert!(!fsm3.accepts("100"));
        assert!(!fsm3.accepts("101"));
        assert!(fsm3.accepts("110"));
        assert!(!fsm3.accepts("111"));
        assert!(!fsm3.accepts("0000"));
        assert!(!fsm3.accepts("0001"));
        assert!(!fsm3.accepts("0010"));
        assert!(!fsm3.accepts("0011"));
        assert!(!fsm3.accepts("0100"));
        assert!(!fsm3.accepts("0101"));
        assert!(!fsm3.accepts("0110"));
        assert!(!fsm3.accepts("0111"));
        assert!(!fsm3.accepts("1000"));
        assert!(fsm3.accepts("1001"));
    }

    #[test]
    fn oblivion_crawl_avoids_oblivion_state() {
        let abc = fsm(
            cs(&['a', 'b', 'c']),
            BTreeSet::from([0, 1, 2, 3]),
            0,
            BTreeSet::from([3]),
            trans(&[(0, &[('a', 1)]), (1, &[('b', 2)]), (2, &[('c', 3)])]),
        );
        assert_eq!(abc.difference(&abc).states().len(), 1);
        assert_eq!(abc.concatenate(&abc).states().len(), 7);
        assert_eq!(abc.star().states().len(), 3);
        assert_eq!(abc.times(3).unwrap().states().len(), 10);
        assert_eq!(abc.union(&abc).states().len(), 4);
        assert_eq!(abc.intersection(&abc).states().len(), 4);
        assert_eq!(abc.symmetric_difference(&abc).states().len(), 1);
    }

    #[test]
    fn dead_states_are_allowed() {
        let fsm = fsm(
            cs(&['/', '*', ANYTHING_ELSE]),
            BTreeSet::from([0, 1, 2, 3, 4, 5]),
            0,
            BTreeSet::from([4]),
            trans(&[
                (0, &[('/', 1)]),
                (1, &[('*', 2)]),
                (2, &[('/', 2), (ANYTHING_ELSE, 2), ('*', 3)]),
                (3, &[('/', 4), (ANYTHING_ELSE, 2), ('*', 3)]),
            ]),
        );

        assert_eq!(fsm.strings().take(1).collect::<Vec<_>>(), vec!["/**/".to_string()]);
        assert!(fsm.is_live(3));
        assert!(fsm.is_live(4));
        assert!(!fsm.is_live(5));
        assert!(fsm.accepts("/* whatever */"));
        assert!(!fsm.accepts("** whatever */"));

        let but = fsm.everything_but();
        assert!(!but.accepts("/* whatever */"));
        assert!(but.accepts("*"));
    }

    #[test]
    fn fsm_properties_like_list() {
        let fsm_a = create_fsm_a();
        let fsm_b = create_fsm_b();

        assert_eq!(fsm_a.length(), Some(1));
        assert_eq!(fsm_a.union(&fsm_b).times(4).unwrap().length(), Some(16));
        assert_eq!(fsm_a.star().length(), None);
        assert_eq!(Fsm::union_many(&[fsm_a.clone(), fsm_b.clone()]), fsm_a.union(&fsm_b));
        assert_eq!(Fsm::union_many(&[]).length(), Some(0));
        assert_eq!(Fsm::intersection_many(&[fsm_a.clone(), fsm_b.clone()]), fsm_a.intersection(&fsm_b));
    }

    #[test]
    fn logical_operations_behave_as_expected() {
        let fsm_none = Fsm::intersection_many(&[]);
        assert_eq!(fsm_none.length(), Some(1));
        assert_eq!(strs(&fsm_none), vec!["".to_string()]);

        let fsm_a = create_fsm_a();
        let fsm_b = create_fsm_b();

        assert_eq!(fsm_a.union(&fsm_b).difference(&fsm_a), fsm_b);
        assert_eq!(fsm_a.union(&fsm_b).difference(&fsm_a).difference(&fsm_b), Fsm::null_fsm(cs(&['a', 'b'])));
        assert!(fsm_a.is_disjoint(&fsm_b));
        assert!(fsm_a.is_subset(&fsm_a.union(&fsm_b)));
        assert!(fsm_a.is_strict_subset(&fsm_a.union(&fsm_b)));
        assert!(fsm_a.different(&fsm_a.union(&fsm_b)));
        assert!(fsm_a.union(&fsm_b).is_strict_superset(&fsm_a));
        assert!(fsm_a.union(&fsm_b).is_superset(&fsm_b));
    }

    #[test]
    fn invalid_fsm_fails() {
        assert!(Fsm::new(cs(&['a']), BTreeSet::from([0]), 1, BTreeSet::from([0]), trans(&[])).is_err());
        assert!(Fsm::new(cs(&['a']), BTreeSet::from([1]), 1, BTreeSet::from([2]), trans(&[])).is_err());
        assert!(Fsm::new(cs(&['a']), BTreeSet::from([1, 2]), 1, BTreeSet::from([2]), trans(&[(1, &[('a', 3)])])).is_err());
    }
}
