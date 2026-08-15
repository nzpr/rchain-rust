//! The rholang term structure (hand-written AST mirroring `RhoTypes.proto`).
//!
//! Mirrors `models/src/main/protobuf/RhoTypes.proto`. `locallyFree` is wrapped in [`AlwaysEqual`]
//! so that it is excluded from equality, exactly as in the Scala `AlwaysEqual[BitSet]` mapper.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use num_bigint::BigInt;

/// A bitset of free-variable levels (the Scala `scala.collection.immutable.BitSet`).
pub type BitSet = Vec<i32>;

/// A wrapper whose equality and hash are constant, mirroring the Scala `AlwaysEqual`.
///
/// Used for `locallyFree`, which is excluded from `Par`/`Send`/… equality by design.
#[derive(Clone, Debug, Default)]
pub struct AlwaysEqual<T>(pub T);

impl<T> PartialEq for AlwaysEqual<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T> Eq for AlwaysEqual<T> {}

impl<T> Hash for AlwaysEqual<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        121410467i32.hash(state);
    }
}

/// A variable (de Bruijn levels).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Var {
    BoundVar(i32),
    FreeVar(i32),
    Wildcard,
    #[default]
    Empty,
}

/// A `Par` — the top-level process, a flat record of eight list fields.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Par {
    pub sends: Vec<Send>,
    pub receives: Vec<Receive>,
    pub news: Vec<New>,
    pub exprs: Vec<Expr>,
    pub matches: Vec<Match>,
    pub unforgeables: Vec<GUnforgeable>,
    pub bundles: Vec<Bundle>,
    pub connectives: Vec<Connective>,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
}

impl Par {
    /// Field-wise list append (the `|` operator).
    pub fn par_merge(&self, other: &Par) -> Par {
        let mut out = self.clone();
        out.sends.extend(other.sends.iter().cloned());
        out.receives.extend(other.receives.iter().cloned());
        out.news.extend(other.news.iter().cloned());
        out.exprs.extend(other.exprs.iter().cloned());
        out.matches.extend(other.matches.iter().cloned());
        out.unforgeables.extend(other.unforgeables.iter().cloned());
        out.bundles.extend(other.bundles.iter().cloned());
        out.connectives.extend(other.connectives.iter().cloned());
        out.connective_used = self.connective_used || other.connective_used;
        out
    }
}

/// A send: `chan!(data)` (or `chan!!(data)` when persistent).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Send {
    pub chan: Box<Par>,
    pub data: Vec<Par>,
    pub persistent: bool,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
}

/// A receive bind: `patterns <- source`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ReceiveBind {
    pub patterns: Vec<Par>,
    pub source: Box<Par>,
    pub remainder: Option<Box<Var>>,
    pub free_count: i32,
}

/// A receive: `for (binds) { body }`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Receive {
    pub binds: Vec<ReceiveBind>,
    pub body: Box<Par>,
    pub persistent: bool,
    pub peek: bool,
    pub bind_count: i32,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
}

/// A `new x1, ..., xn in { p }`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct New {
    pub bind_count: i32,
    pub p: Box<Par>,
    pub uri: Vec<String>,
    pub injections: BTreeMap<String, Par>,
    pub locally_free: AlwaysEqual<BitSet>,
}

/// A match case: `pattern => source`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MatchCase {
    pub pattern: Box<Par>,
    pub source: Box<Par>,
    pub free_count: i32,
}

/// A `match target { cases }`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Match {
    pub target: Box<Par>,
    pub cases: Vec<MatchCase>,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
}

/// A quoted/unquoted bundle.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Bundle {
    pub body: Box<Par>,
    pub write_flag: bool,
    pub read_flag: bool,
}

impl Bundle {
    /// Merge bundle flags (port of `BundleOps.merge`): keep `other`'s body, AND the read/write flags.
    pub fn merge(&self, other: &Bundle) -> Bundle {
        Bundle {
            body: other.body.clone(),
            write_flag: self.write_flag && other.write_flag,
            read_flag: self.read_flag && other.read_flag,
        }
    }
}

/// An expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    GBool(bool),
    GInt(i64),
    GBigInt(BigInt),
    GString(String),
    GUri(String),
    GByteArray(Vec<u8>),
    ENot(Box<Par>),
    ENeg(Box<Par>),
    EVar(Box<Var>),
    EMult(Box<Par>, Box<Par>),
    EDiv(Box<Par>, Box<Par>),
    EMod(Box<Par>, Box<Par>),
    EPlus(Box<Par>, Box<Par>),
    EMinus(Box<Par>, Box<Par>),
    ELt(Box<Par>, Box<Par>),
    ELte(Box<Par>, Box<Par>),
    EGt(Box<Par>, Box<Par>),
    EGte(Box<Par>, Box<Par>),
    EEq(Box<Par>, Box<Par>),
    ENeq(Box<Par>, Box<Par>),
    EAnd(Box<Par>, Box<Par>),
    EOr(Box<Par>, Box<Par>),
    EShortAnd(Box<Par>, Box<Par>),
    EShortOr(Box<Par>, Box<Par>),
    EMatches(Box<Par>, Box<Par>),
    EPercentPercent(Box<Par>, Box<Par>),
    EPlusPlus(Box<Par>, Box<Par>),
    EMinusMinus(Box<Par>, Box<Par>),
    EList(EList),
    ETuple(ETuple),
    ESet(ParSet),
    EMap(ParMap),
    EMethod(EMethod),
}

/// A list expression.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EList {
    pub ps: Vec<Par>,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
    pub remainder: Option<Box<Var>>,
}

/// A tuple expression.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ETuple {
    pub ps: Vec<Par>,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
}

/// A set expression (order-insensitive, deduplicated).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParSet {
    pub ps: Vec<Par>,
    pub connective_used: bool,
    pub locally_free: AlwaysEqual<BitSet>,
    pub remainder: Option<Box<Var>>,
}

/// A map expression (order-insensitive by key, last-write-wins).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParMap {
    pub kvs: Vec<(Par, Par)>,
    pub connective_used: bool,
    pub locally_free: AlwaysEqual<BitSet>,
    pub remainder: Option<Box<Var>>,
}

/// A method call: `target.methodName(arguments)`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EMethod {
    pub method_name: String,
    pub target: Box<Par>,
    pub arguments: Vec<Par>,
    pub locally_free: AlwaysEqual<BitSet>,
    pub connective_used: bool,
}

/// An unforgeable name.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum GUnforgeable {
    GPrivate(GPrivate),
    GDeployId(GDeployId),
    GDeployerId(GDeployerId),
    GSysAuthToken,
    #[default]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GPrivate {
    pub id: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GDeployId {
    pub sig: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GDeployerId {
    pub public_key: Vec<u8>,
}

/// A logical connective.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Connective {
    ConnAnd(ConnectiveBody),
    ConnOr(ConnectiveBody),
    ConnNot(Box<Par>),
    VarRef(VarRef),
    ConnBool(bool),
    ConnInt(bool),
    ConnBigInt(bool),
    ConnString(bool),
    ConnUri(bool),
    ConnByteArray(bool),
    #[default]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ConnectiveBody {
    pub ps: Vec<Par>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct VarRef {
    pub index: i32,
    pub depth: i32,
}
