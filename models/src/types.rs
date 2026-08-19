//! The type-system layer over the ρ-calculus (the Calculus-of-Constructions hardening).
//!
//! Mirrors `spec/Rchain/Ty.lean` (and `Rho.lean`). This module gives the port the hard type
//! discipline of [`spec/TYPE-SYSTEM.md`]: the two language sorts (`PSort`), the structural
//! name-vs-process classification, the `Closed` well-formedness refinement (Law 6), and the
//! de Bruijn context judgment (`varSort`). It is a hardening of the port — no behavior change.

use serde::{Deserialize, Serialize};

use crate::ast::{
    Bundle, Connective, Expr, GUnforgeable, Match, MatchCase, New, Par, Receive, ReceiveBind, Send,
    Sort, Var,
};

/// The two syntactic sorts (mirrors `Ty.lean`'s `inductive PSort | proc | name`): a term is used in
/// *process* position or *name* position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PSort {
    Proc,
    Name,
}

/// A de Bruijn context: the sort of each level in scope (mirrors `Ty.lean`'s `Ctx := List PSort`).
pub type Ctx = Vec<PSort>;

/// The sort of a variable occurrence (mirrors `varSort`): a bound level is looked up in the context;
/// a free variable or wildcard has no local sort.
pub fn var_sort(ctx: &Ctx, v: &Var) -> Option<PSort> {
    match v {
        Var::BoundVar(l) if *l >= 0 => ctx.get(*l as usize).copied(),
        Var::BoundVar(_) | Var::FreeVar(_) | Var::Wildcard | Var::Empty => None,
    }
}

/// A *pure name* (mirrors `isPureName`): a `Par` with no process constructors at the top (empty
/// `sends`/`receives`/`news`/`matches`). These occur in name position: `Nil`, ground/expressions,
/// bundles, unforgeables, connectives.
pub fn is_pure_name(p: &Par) -> bool {
    p.sends.is_empty() && p.receives.is_empty() && p.news.is_empty() && p.matches.is_empty()
}

/// The structural sort classification (mirrors `classify`): a pure name is a `Name`, otherwise `Proc`.
pub fn classify(p: &Par) -> PSort {
    if is_pure_name(p) {
        PSort::Name
    } else {
        PSort::Proc
    }
}

/// Validated sort construction: a `Par` is a `Name` iff it is a pure name (the structural sort).
/// The `quote` re-marking carries the sort thereafter; the invariant is checked exactly once, here.
impl TryFrom<Par> for crate::ast::Name {
    type Error = String;
    fn try_from(p: Par) -> Result<Self, Self::Error> {
        if is_pure_name(&p) {
            Ok(p.quote())
        } else {
            Err("term is not a pure name (has a top-level send/receive/new/match)".to_string())
        }
    }
}

/// One-way boundary discharge: a `Name` re-enters the general `Par` by `eval` (the reflective `*`;
/// the flat record is unchanged).
impl From<crate::ast::Name> for Par {
    fn from(n: crate::ast::Name) -> Par {
        n.eval()
    }
}

// --- Closedness (Law 6): no free variables ------------------------------------------------

fn closed_var(v: &Var) -> bool {
    match v {
        Var::FreeVar(_) => false,
        Var::BoundVar(_) | Var::Wildcard | Var::Empty => true,
    }
}

fn closed_par<S: Sort>(p: &Par<S>) -> bool {
    p.sends.iter().all(closed_send)
        && p.receives.iter().all(closed_receive)
        && p.news.iter().all(closed_new)
        && p.exprs.iter().all(closed_expr)
        && p.matches.iter().all(closed_match)
        && p.unforgeables.iter().all(closed_unforgeable)
        && p.bundles.iter().all(closed_bundle)
        && p.connectives.iter().all(closed_connective)
}

fn closed_send(s: &Send) -> bool {
    closed_par(&s.chan) && s.data.iter().all(closed_par)
}

fn closed_receive_bind(rb: &ReceiveBind) -> bool {
    rb.patterns.iter().all(closed_par) && closed_par(&rb.source)
}

fn closed_receive(r: &Receive) -> bool {
    r.binds.iter().all(closed_receive_bind) && closed_par(&r.body)
}

fn closed_new(n: &New) -> bool {
    closed_par(&n.p)
}

fn closed_match_case(mc: &MatchCase) -> bool {
    closed_par(&mc.pattern) && closed_par(&mc.source)
}

fn closed_match(m: &Match) -> bool {
    closed_par(&m.target) && m.cases.iter().all(closed_match_case)
}

fn closed_expr(e: &Expr) -> bool {
    match e {
        Expr::GBool(_)
        | Expr::GInt(_)
        | Expr::GBigInt(_)
        | Expr::GString(_)
        | Expr::GUri(_)
        | Expr::GByteArray(_) => true,
        Expr::EVar(v) => closed_var(v),
        Expr::ENot(p) | Expr::ENeg(p) => closed_par(p),
        Expr::EMult(p, q)
        | Expr::EDiv(p, q)
        | Expr::EMod(p, q)
        | Expr::EPlus(p, q)
        | Expr::EMinus(p, q)
        | Expr::ELt(p, q)
        | Expr::ELte(p, q)
        | Expr::EGt(p, q)
        | Expr::EGte(p, q)
        | Expr::EEq(p, q)
        | Expr::ENeq(p, q)
        | Expr::EAnd(p, q)
        | Expr::EOr(p, q)
        | Expr::EShortAnd(p, q)
        | Expr::EShortOr(p, q)
        | Expr::EMatches(p, q)
        | Expr::EPercentPercent(p, q)
        | Expr::EPlusPlus(p, q)
        | Expr::EMinusMinus(p, q) => closed_par(p) && closed_par(q),
        Expr::EList(el) => el.ps.iter().all(closed_par),
        Expr::ETuple(et) => et.ps.iter().all(closed_par),
        Expr::ESet(set) => set.ps.iter().all(closed_par),
        Expr::EMap(map) => map.kvs.iter().all(|(k, v)| closed_par(k) && closed_par(v)),
        Expr::EMethod(em) => closed_par(&em.target) && em.arguments.iter().all(closed_par),
    }
}

fn closed_bundle(b: &Bundle) -> bool {
    closed_par(&b.body)
}

fn closed_unforgeable(_: &GUnforgeable) -> bool {
    true
}

fn closed_connective(c: &Connective) -> bool {
    match c {
        Connective::ConnAnd(cb) | Connective::ConnOr(cb) => cb.ps.iter().all(closed_par),
        Connective::ConnNot(p) => closed_par(p),
        Connective::VarRef(_)
        | Connective::ConnBool(_)
        | Connective::ConnInt(_)
        | Connective::ConnBigInt(_)
        | Connective::ConnString(_)
        | Connective::ConnUri(_)
        | Connective::ConnByteArray(_)
        | Connective::Empty => true,
    }
}

/// `is_closed p` — the process has no free variables (Law 6). Decidable, and preserved by
/// composition, `≡`, and canonicalization (mirrors `Ty.lean`'s `closed` / `Closed`).
pub fn is_closed(p: &Par) -> bool {
    closed_par(p)
}

/// A closed process — the well-formedness refinement that makes the interpreter's partiality
/// impossible (the Rust spelling of `TotalOn`/the totality invariant in `TYPE-SYSTEM.md` §1.6).
///
/// Constructed only via [`Closed::new`], which is a declared partiality boundary: it returns `None`
/// for a term with free variables rather than panicking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closed(Par);

impl Closed {
    /// Validate that `par` is closed. `None` is the declared boundary for an open (free-variable)
    /// term.
    pub fn new(par: Par) -> Option<Closed> {
        if is_closed(&par) {
            Some(Closed(par))
        } else {
            None
        }
    }

    /// The underlying closed process.
    pub fn into_inner(self) -> Par {
        self.0
    }

    /// Borrow the underlying closed process.
    pub fn as_par(&self) -> &Par {
        &self.0
    }
}

// --- Well-scopedness (the variable half of the judgment) ---------------------------------

fn well_scoped_var(ctx: &Ctx, v: &Var) -> bool {
    match v {
        Var::BoundVar(l) => *l >= 0 && (*l as usize) < ctx.len(),
        Var::FreeVar(_) | Var::Wildcard | Var::Empty => true,
    }
}

fn well_scoped_par<S: Sort>(ctx: &Ctx, p: &Par<S>) -> bool {
    p.sends.iter().all(|s| well_scoped_send(ctx, s))
        && p.receives.iter().all(|r| well_scoped_receive(ctx, r))
        && p.news.iter().all(|n| well_scoped_new(ctx, n))
        && p.exprs.iter().all(|e| well_scoped_expr(ctx, e))
        && p.matches.iter().all(|m| well_scoped_match(ctx, m))
        && p.bundles.iter().all(|b| well_scoped_par(ctx, &b.body))
        && p.connectives.iter().all(|c| well_scoped_connective(ctx, c))
}

fn well_scoped_send(ctx: &Ctx, s: &Send) -> bool {
    well_scoped_par(ctx, &s.chan) && s.data.iter().all(|d| well_scoped_par(ctx, d))
}

fn well_scoped_receive_bind(ctx: &Ctx, rb: &ReceiveBind) -> bool {
    rb.patterns.iter().all(|p| well_scoped_par(ctx, p))
        && well_scoped_par(ctx, &rb.source)
        && rb.remainder
            .as_ref()
            .map(|v| well_scoped_var(ctx, v))
            .unwrap_or(true)
}

fn well_scoped_receive(ctx: &Ctx, r: &Receive) -> bool {
    r.binds.iter().all(|b| well_scoped_receive_bind(ctx, b)) && well_scoped_par(ctx, &r.body)
}

fn well_scoped_new(ctx: &Ctx, n: &New) -> bool {
    well_scoped_par(ctx, &n.p)
}

fn well_scoped_match_case(ctx: &Ctx, mc: &MatchCase) -> bool {
    well_scoped_par(ctx, &mc.pattern) && well_scoped_par(ctx, &mc.source)
}

fn well_scoped_match(ctx: &Ctx, m: &Match) -> bool {
    well_scoped_par(ctx, &m.target) && m.cases.iter().all(|c| well_scoped_match_case(ctx, c))
}

fn well_scoped_expr(ctx: &Ctx, e: &Expr) -> bool {
    match e {
        Expr::GBool(_)
        | Expr::GInt(_)
        | Expr::GBigInt(_)
        | Expr::GString(_)
        | Expr::GUri(_)
        | Expr::GByteArray(_) => true,
        Expr::EVar(v) => well_scoped_var(ctx, v),
        Expr::ENot(p) | Expr::ENeg(p) => well_scoped_par(ctx, p),
        Expr::EMult(p, q)
        | Expr::EDiv(p, q)
        | Expr::EMod(p, q)
        | Expr::EPlus(p, q)
        | Expr::EMinus(p, q)
        | Expr::ELt(p, q)
        | Expr::ELte(p, q)
        | Expr::EGt(p, q)
        | Expr::EGte(p, q)
        | Expr::EEq(p, q)
        | Expr::ENeq(p, q)
        | Expr::EAnd(p, q)
        | Expr::EOr(p, q)
        | Expr::EShortAnd(p, q)
        | Expr::EShortOr(p, q)
        | Expr::EMatches(p, q)
        | Expr::EPercentPercent(p, q)
        | Expr::EPlusPlus(p, q)
        | Expr::EMinusMinus(p, q) => well_scoped_par(ctx, p) && well_scoped_par(ctx, q),
        Expr::EList(el) => el.ps.iter().all(|p| well_scoped_par(ctx, p)),
        Expr::ETuple(et) => et.ps.iter().all(|p| well_scoped_par(ctx, p)),
        Expr::ESet(set) => set.ps.iter().all(|p| well_scoped_par(ctx, p)),
        Expr::EMap(map) => map
            .kvs
            .iter()
            .all(|(k, v)| well_scoped_par(ctx, k) && well_scoped_par(ctx, v)),
        Expr::EMethod(em) => {
            well_scoped_par(ctx, &em.target) && em.arguments.iter().all(|p| well_scoped_par(ctx, p))
        }
    }
}

fn well_scoped_connective(ctx: &Ctx, c: &Connective) -> bool {
    match c {
        Connective::ConnAnd(cb) | Connective::ConnOr(cb) => {
            cb.ps.iter().all(|p| well_scoped_par(ctx, p))
        }
        Connective::ConnNot(p) => well_scoped_par(ctx, p),
        Connective::VarRef(_)
        | Connective::ConnBool(_)
        | Connective::ConnInt(_)
        | Connective::ConnBigInt(_)
        | Connective::ConnString(_)
        | Connective::ConnUri(_)
        | Connective::ConnByteArray(_)
        | Connective::Empty => true,
    }
}

/// `well_scoped Γ t` — every bound level of `t` is within `Γ` (the variable half of the typing
/// judgment). A `BoundVar(l)` is in scope iff `0 ≤ l < Γ.len()`; free/wildcard/empty variables carry
/// no local scope requirement.
pub fn well_scoped(ctx: &Ctx, p: &Par) -> bool {
    well_scoped_par(ctx, p)
}

/// A well-scoped process under a de Bruijn context `Γ` — the variable half of the typing judgment
/// (`WellScoped Γ t`). Constructed only via [`WellScoped::new`], which is a declared partiality
/// boundary: it returns `None` for a term with an out-of-scope bound level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WellScoped {
    ctx: Ctx,
    par: Par,
}

impl WellScoped {
    /// Validate that `par` is well-scoped under `ctx`. `None` is the declared boundary for an
    /// out-of-scope bound variable.
    pub fn new(ctx: Ctx, par: Par) -> Option<WellScoped> {
        if well_scoped(&ctx, &par) {
            Some(WellScoped { ctx, par })
        } else {
            None
        }
    }

    /// The context the term is scoped under.
    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    /// Borrow the underlying (well-scoped) process.
    pub fn as_par(&self) -> &Par {
        &self.par
    }
}

/// One-way boundary discharge: a well-scoped process re-enters the general `Par` (the proof is
/// dropped at the boundary).
impl From<WellScoped> for Par {
    fn from(w: WellScoped) -> Par {
        w.par
    }
}

// --- BindsAtMostOnce (Law 5): the free-variable count of a pattern ----------------------

/// The number of free variables a pattern binds (Law 5, `BindsAtMostOnce`): a non-negative count,
/// carried by the `free_count` fields of `ReceiveBind`/`MatchCase`. The normalizer computes it as the
/// number of *distinct* free variables in the pattern, so each is bound at most once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct FreeCount(i32);

impl TryFrom<i32> for FreeCount {
    type Error = String;
    fn try_from(n: i32) -> Result<Self, Self::Error> {
        FreeCount::new(n).ok_or_else(|| format!("negative free-count: {n}"))
    }
}

impl FreeCount {
    /// The empty-pattern count.
    pub const ZERO: FreeCount = FreeCount(0);

    /// Validated construction — the declared partiality boundary for a negative count.
    pub fn new(n: i32) -> Option<FreeCount> {
        if n >= 0 {
            Some(FreeCount(n))
        } else {
            None
        }
    }

    /// Total construction from a count already known non-negative (e.g. `FreeMap::count_no_wildcards`).
    pub fn from_nonneg(n: i32) -> FreeCount {
        debug_assert!(n >= 0, "free-count must be non-negative");
        FreeCount(n)
    }
}

/// One-way boundary discharge: the raw count (`i32`) is used at the range/arithmetic boundaries
/// (e.g. `0..free_count`, the wire codec).
impl From<FreeCount> for i32 {
    fn from(f: FreeCount) -> i32 {
        f.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g_int(i: i64) -> Par {
        Par {
            exprs: vec![Expr::GInt(i)],
            ..Default::default()
        }
    }

    fn free_var(l: i32) -> Par {
        Par {
            exprs: vec![Expr::EVar(Box::new(Var::FreeVar(l)))],
            ..Default::default()
        }
    }

    #[test]
    fn classify_nil_is_name() {
        assert_eq!(classify(&Par::default()), PSort::Name);
    }

    #[test]
    fn classify_process_is_proc() {
        let send = Par {
            sends: vec![Send::default()],
            ..Default::default()
        };
        assert_eq!(classify(&send), PSort::Proc);
    }

    #[test]
    fn classify_ground_expr_is_name() {
        assert_eq!(classify(&g_int(7)), PSort::Name);
    }

    #[test]
    fn var_sort_looks_up_bound_level() {
        let ctx = vec![PSort::Name, PSort::Proc];
        assert_eq!(var_sort(&ctx, &Var::BoundVar(0)), Some(PSort::Name));
        assert_eq!(var_sort(&ctx, &Var::BoundVar(1)), Some(PSort::Proc));
        assert_eq!(var_sort(&ctx, &Var::BoundVar(2)), None);
    }

    #[test]
    fn var_sort_free_and_wildcard_are_none() {
        let ctx = vec![PSort::Name];
        assert_eq!(var_sort(&ctx, &Var::FreeVar(0)), None);
        assert_eq!(var_sort(&ctx, &Var::Wildcard), None);
        assert_eq!(var_sort(&ctx, &Var::Empty), None);
    }

    #[test]
    fn nil_is_closed() {
        assert!(Closed::new(Par::default()).is_some());
    }

    #[test]
    fn free_variable_is_not_closed() {
        assert!(Closed::new(free_var(0)).is_none());
    }

    #[test]
    fn bound_variable_is_closed() {
        let bound = Par {
            exprs: vec![Expr::EVar(Box::new(Var::BoundVar(0)))],
            ..Default::default()
        };
        assert!(is_closed(&bound));
    }

    #[test]
    fn par_merge_preserves_closedness() {
        assert!(is_closed(&g_int(1).par_merge(&g_int(2))));
        assert!(!is_closed(&free_var(0).par_merge(&g_int(2))));
    }

    fn bound_var(l: i32) -> Par {
        Par {
            exprs: vec![Expr::EVar(Box::new(Var::BoundVar(l)))],
            ..Default::default()
        }
    }

    #[test]
    fn well_scoped_bound_level_within_ctx() {
        let ctx = vec![PSort::Name, PSort::Proc];
        assert!(well_scoped(&ctx, &bound_var(0)));
        assert!(well_scoped(&ctx, &bound_var(1)));
        assert!(!well_scoped(&ctx, &bound_var(2)));
        assert!(!well_scoped(&ctx, &bound_var(-1)));
    }

    #[test]
    fn well_scoped_free_and_wildcard_are_always_in_scope() {
        let ctx: Ctx = vec![];
        assert!(well_scoped(&ctx, &free_var(0)));
        assert!(well_scoped(&ctx, &Par::default()));
    }

    #[test]
    fn well_scoped_newtype_validates() {
        let ctx = vec![PSort::Proc];
        assert!(WellScoped::new(ctx.clone(), bound_var(0)).is_some());
        assert!(WellScoped::new(ctx.clone(), bound_var(1)).is_none());
    }

    #[test]
    fn well_scoped_discharges_to_par() {
        let ctx = vec![PSort::Proc];
        let ws = WellScoped::new(ctx, bound_var(0)).unwrap();
        let p: Par = ws.into();
        assert_eq!(p, bound_var(0));
    }

    #[test]
    fn free_count_rejects_negative() {
        assert!(FreeCount::new(-1).is_none());
        assert!(FreeCount::new(0).is_some());
        assert_eq!(i32::from(FreeCount::new(3).unwrap()), 3);
    }

    #[test]
    fn free_count_from_nonneg() {
        assert_eq!(i32::from(FreeCount::from_nonneg(5)), 5);
    }
}
