//! The type-system layer over the ρ-calculus (the Calculus-of-Constructions hardening).
//!
//! Mirrors `spec/Rchain/Ty.lean` (and `Rho.lean`). This module gives the port the hard type
//! discipline of [`spec/TYPE-SYSTEM.md`]: the two language sorts (`PSort`), the structural
//! name-vs-process classification, the `Closed` well-formedness refinement (Law 6), and the
//! de Bruijn context judgment (`varSort`). It is a hardening of the port — no behavior change.

use crate::ast::{
    Bundle, Connective, Expr, GUnforgeable, Match, MatchCase, New, Par, Receive, ReceiveBind, Send,
    Var,
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

// --- Closedness (Law 6): no free variables ------------------------------------------------

fn closed_var(v: &Var) -> bool {
    match v {
        Var::FreeVar(_) => false,
        Var::BoundVar(_) | Var::Wildcard | Var::Empty => true,
    }
}

fn closed_par(p: &Par) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn g_int(i: i64) -> Par {
        Par {
            exprs: vec![Expr::GInt(i)],
            ..Par::default()
        }
    }

    fn free_var(l: i32) -> Par {
        Par {
            exprs: vec![Expr::EVar(Box::new(Var::FreeVar(l)))],
            ..Par::default()
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
            ..Par::default()
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
            ..Par::default()
        };
        assert!(is_closed(&bound));
    }

    #[test]
    fn par_merge_preserves_closedness() {
        assert!(is_closed(&g_int(1).par_merge(&g_int(2))));
        assert!(!is_closed(&free_var(0).par_merge(&g_int(2))));
    }
}
