//! Refinement newtypes — the "no silent partiality" strong types.
//!
//! Mirrors the `TotalOn`/`Closed` refinements in [`Rchain/Ty.lean`](../../spec/Rchain/Ty.lean). A
//! refinement type is a value together with an invariant `P`: the invariant is **part of the type**
//! and travels with the value through the domain. It is obtained only via a *validated* constructor
//! (`TryFrom`) or a *total* constructor on an already-valid input, and it is discharged (the raw
//! inner value observed) only via the explicit, one-way `From<Newtype> for Raw` conversion — used at
//! a declared boundary (wire encode, FFI, external API).
//!
//! **No type escape.** Refinement newtypes must not implement `Deref` or expose a public `.get()`:
//! those silently drop the invariant mid-domain and re-introduce the exact silent-cast bug the
//! refinement exists to prevent. If you find yourself reaching for the raw value outside a boundary,
//! the correct fix is to carry the newtype through the domain (make the field/parameter the newtype),
//! not to unwrap it.

/// A refinement violation (a value outside a newtype's invariant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefineError(pub String);

impl RefineError {
    pub fn new(msg: impl Into<String>) -> Self {
        RefineError(msg.into())
    }
}

impl std::fmt::Display for RefineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RefineError {}

/// Define a non-negative signed-integer newtype. Construction is `TryFrom` (validates `v >= 0`);
/// discharge is `From<Newtype> for Inner`.
macro_rules! non_neg_signed {
    ($name:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($inner);

        impl TryFrom<$inner> for $name {
            type Error = RefineError;
            fn try_from(v: $inner) -> Result<Self, Self::Error> {
                if v >= 0 {
                    Ok(Self(v))
                } else {
                    Err(RefineError::new(format!(
                        "{} must be non-negative, got {v}",
                        stringify!($name)
                    )))
                }
            }
        }

        /// Boundary discharge (wire/FFI only): the raw signed value.
        impl From<$name> for $inner {
            fn from(v: $name) -> $inner {
                v.0
            }
        }
    };
}

non_neg_signed!(NonNegI64, i64);
non_neg_signed!(NonNegI32, i32);

impl NonNegI64 {
    /// The value one (total: `1` is non-negative).
    pub const fn one() -> Self {
        NonNegI64(1)
    }
}

/// A block height (non-negative). Used for `block_number`/`block_num`/`height` across the DAG and
/// consensus layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockHeight(i64);

impl BlockHeight {
    /// The genesis/empty height (total: `0` is non-negative).
    pub const fn zero() -> Self {
        BlockHeight(0)
    }
}

impl TryFrom<i64> for BlockHeight {
    type Error = RefineError;
    fn try_from(v: i64) -> Result<Self, Self::Error> {
        if v >= 0 {
            Ok(BlockHeight(v))
        } else {
            Err(RefineError::new(format!("block height must be non-negative, got {v}")))
        }
    }
}

impl From<BlockHeight> for i64 {
    fn from(v: BlockHeight) -> i64 {
        v.0
    }
}

impl std::fmt::Display for BlockHeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add<NonNegI64> for BlockHeight {
    type Output = BlockHeight;
    fn add(self, rhs: NonNegI64) -> BlockHeight {
        // Sum of two non-negative values stays non-negative (the invariant is preserved).
        BlockHeight(self.0 + i64::from(rhs))
    }
}

impl std::ops::Sub<i64> for BlockHeight {
    type Output = i64;
    fn sub(self, rhs: i64) -> i64 {
        self.0 - rhs
    }
}

impl std::ops::Sub<BlockHeight> for BlockHeight {
    type Output = i64;
    fn sub(self, rhs: BlockHeight) -> i64 {
        self.0 - rhs.0
    }
}

/// A sequence number (non-negative). Used for `seq_num`/`sender_seq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeqNum(i64);

impl SeqNum {
    /// The zero sequence number (total: `0` is non-negative).
    pub const fn zero() -> Self {
        SeqNum(0)
    }
}

impl TryFrom<i64> for SeqNum {
    type Error = RefineError;
    fn try_from(v: i64) -> Result<Self, Self::Error> {
        if v >= 0 {
            Ok(SeqNum(v))
        } else {
            Err(RefineError::new(format!("sequence number must be non-negative, got {v}")))
        }
    }
}

impl From<SeqNum> for i64 {
    fn from(v: SeqNum) -> i64 {
        v.0
    }
}

impl std::fmt::Display for SeqNum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add<NonNegI64> for SeqNum {
    type Output = SeqNum;
    fn add(self, rhs: NonNegI64) -> SeqNum {
        // Sum of two non-negative values stays non-negative (the invariant is preserved).
        SeqNum(self.0 + i64::from(rhs))
    }
}

impl std::ops::Sub<i64> for SeqNum {
    type Output = i64;
    fn sub(self, rhs: i64) -> i64 {
        self.0 - rhs
    }
}

impl std::ops::Sub<SeqNum> for SeqNum {
    type Output = i64;
    fn sub(self, rhs: SeqNum) -> i64 {
        self.0 - rhs.0
    }
}

/// A TCP/UDP port (`0..=65535`). Replaces `i32 → u16` port casts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Port(u16);

impl Port {
    /// Total constructor: a `u16` is already within the valid port range.
    pub const fn new(port: u16) -> Self {
        Port(port)
    }
}

impl TryFrom<i32> for Port {
    type Error = RefineError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        u16::try_from(v)
            .map(Port)
            .map_err(|_| RefineError::new(format!("port out of range 0..=65535: {v}")))
    }
}

impl TryFrom<u32> for Port {
    type Error = RefineError;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        u16::try_from(v)
            .map(Port)
            .map_err(|_| RefineError::new(format!("port out of range 0..=65535: {v}")))
    }
}

impl From<Port> for u16 {
    fn from(v: Port) -> u16 {
        v.0
    }
}

impl From<Port> for u32 {
    fn from(v: Port) -> u32 {
        u32::from(v.0)
    }
}

/// A phantom/execution cost (non-negative by construction; the wire type is `uint64`). Replaces
/// `u64 ↔ i64` cost casts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cost(u64);

impl Cost {
    /// Total constructor: a `u64` is already non-negative.
    pub const fn new(cost: u64) -> Self {
        Cost(cost)
    }
}

impl From<Cost> for u64 {
    fn from(v: Cost) -> u64 {
        v.0
    }
}

/// Define a fixed-width length newtype. Construction is `TryFrom<usize>` (validates the value fits
/// the wire width); discharge is `From<Newtype> for Inner`.
macro_rules! len_newtype {
    ($name:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($inner);

        impl TryFrom<usize> for $name {
            type Error = RefineError;
            fn try_from(v: usize) -> Result<Self, Self::Error> {
                <$inner>::try_from(v).map($name).map_err(|_| {
                    RefineError::new(format!(
                        "{}: length {v} does not fit in {}",
                        stringify!($name),
                        stringify!($inner)
                    ))
                })
            }
        }

        /// Boundary discharge (wire/FFI only): the raw fixed-width length.
        impl From<$name> for $inner {
            fn from(v: $name) -> $inner {
                v.0
            }
        }
    };
}

len_newtype!(ByteLen, u8);
len_newtype!(ShortLen, u16);
len_newtype!(WireLen, u32);

/// Decode a `WireLen` from a protobuf `int32` (the `contentLength` wire field), rejecting negative
/// values.
impl TryFrom<i32> for WireLen {
    type Error = RefineError;
    fn try_from(v: i32) -> Result<Self, RefineError> {
        u32::try_from(v)
            .map(WireLen)
            .map_err(|_| RefineError::new(format!("content length must be non-negative: {v}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_neg_i64_accepts_non_negative_rejects_negative() {
        let v: NonNegI64 = 5.try_into().unwrap();
        assert_eq!(i64::from(v), 5);
        assert!(NonNegI64::try_from(0).is_ok());
        assert!(NonNegI64::try_from(-1).is_err());
    }

    #[test]
    fn non_neg_i32() {
        let v: NonNegI32 = 3.try_into().unwrap();
        assert_eq!(i32::from(v), 3);
        assert!(NonNegI32::try_from(-3).is_err());
    }

    #[test]
    fn block_height_and_seq_num() {
        let h: BlockHeight = 7.try_into().unwrap();
        assert_eq!(i64::from(h), 7);
        assert!(BlockHeight::try_from(-1).is_err());
        let s: SeqNum = 9.try_into().unwrap();
        assert_eq!(i64::from(s), 9);
        assert!(SeqNum::try_from(-1).is_err());
    }

    #[test]
    fn arithmetic_preserves_non_negativity() {
        let h = BlockHeight::zero();
        assert_eq!(i64::from(h + NonNegI64::one()), 1);
        let s = SeqNum::zero();
        assert_eq!(i64::from(s + NonNegI64::one()), 1);
        // A negative delta cannot be constructed, so the invariant cannot be broken by `Add`.
        assert!(NonNegI64::try_from(-1).is_err());
    }

    #[test]
    fn port_bounds() {
        assert_eq!(u16::from(Port::try_from(40400).unwrap()), 40400);
        assert_eq!(u16::from(Port::try_from(0).unwrap()), 0);
        assert_eq!(u16::from(Port::try_from(65535).unwrap()), 65535);
        assert!(Port::try_from(-1).is_err());
        assert!(Port::try_from(70000).is_err());
    }

    #[test]
    fn cost_is_non_negative() {
        let c = Cost::new(42);
        assert_eq!(u64::from(c), 42);
    }

    #[test]
    fn length_widths() {
        assert_eq!(u8::from(ByteLen::try_from(255).unwrap()), 255);
        assert!(ByteLen::try_from(256).is_err());
        assert_eq!(u16::from(ShortLen::try_from(65535).unwrap()), 65535);
        assert!(ShortLen::try_from(65536).is_err());
        assert_eq!(u32::from(WireLen::try_from(4_000_000_000usize).unwrap()), 4_000_000_000);
        assert!(WireLen::try_from(usize::MAX).is_err());
    }
}
