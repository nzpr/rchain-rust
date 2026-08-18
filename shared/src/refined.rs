//! Refinement newtypes — the "no silent partiality" strong types.
//!
//! Mirrors the `TotalOn`/`Closed` refinements in [`Rchain/Ty.lean`](../../spec/Rchain/Ty.lean): each
//! newtype carries a numeric invariant (non-negativity, a range, or a fixed wire width). It is
//! constructed via `TryFrom` (fallible, at a declared boundary) or `From` (infallible, when the
//! invariant is free), and read back via `Deref`/`get`. This makes a lossy `as` cast or a
//! `.unwrap()` panic impossible on the happy path: the type forces the boundary to be explicit.

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

/// Define a non-negative signed-integer newtype (`TryFrom` validates `v >= 0`).
macro_rules! non_neg_signed {
    ($name:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($inner);

        impl $name {
            pub const fn get(self) -> $inner {
                self.0
            }
        }

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

        impl From<$name> for $inner {
            fn from(v: $name) -> $inner {
                v.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &$inner {
                &self.0
            }
        }
    };
}

non_neg_signed!(NonNegI64, i64);
non_neg_signed!(NonNegI32, i32);

/// A non-negative `usize`. Unsigned, so construction is total; the newtype keeps the refinement
/// explicit (and symmetric with `NonNegI64`/`NonNegI32`) at call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonNegUsize(usize);

impl NonNegUsize {
    pub const fn get(self) -> usize {
        self.0
    }
}

impl From<usize> for NonNegUsize {
    fn from(v: usize) -> Self {
        NonNegUsize(v)
    }
}

impl From<NonNegUsize> for usize {
    fn from(v: NonNegUsize) -> usize {
        v.0
    }
}

impl std::ops::Deref for NonNegUsize {
    type Target = usize;
    fn deref(&self) -> &usize {
        &self.0
    }
}

/// A TCP/UDP port (`0..=65535`). Replaces `i32 → u16` port casts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Port(u16);

impl Port {
    pub const fn get(self) -> u16 {
        self.0
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

impl TryFrom<u16> for Port {
    type Error = RefineError;
    fn try_from(v: u16) -> Result<Self, Self::Error> {
        Ok(Port(v))
    }
}

impl From<Port> for u16 {
    fn from(v: Port) -> u16 {
        v.0
    }
}

impl std::ops::Deref for Port {
    type Target = u16;
    fn deref(&self) -> &u16 {
        &self.0
    }
}

/// Define a fixed-width length newtype (`TryFrom<usize>` validates the value fits the wire width).
macro_rules! len_newtype {
    ($name:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($inner);

        impl $name {
            pub const fn get(self) -> $inner {
                self.0
            }
        }

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

        impl From<$name> for $inner {
            fn from(v: $name) -> $inner {
                v.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &$inner {
                &self.0
            }
        }
    };
}

len_newtype!(ByteLen, u8);
len_newtype!(ShortLen, u16);
len_newtype!(WireLen, u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_neg_i64_accepts_non_negative_rejects_negative() {
        assert_eq!(NonNegI64::try_from(5).unwrap().get(), 5);
        assert!(NonNegI64::try_from(0).is_ok());
        assert!(NonNegI64::try_from(-1).is_err());
        // Round-trips back to the inner type.
        assert_eq!(i64::from(NonNegI64::try_from(7).unwrap()), 7);
    }

    #[test]
    fn non_neg_i32_and_usize() {
        assert_eq!(NonNegI32::try_from(3).unwrap().get(), 3);
        assert!(NonNegI32::try_from(-3).is_err());
        assert_eq!(NonNegUsize::from(9usize).get(), 9);
    }

    #[test]
    fn port_bounds() {
        assert_eq!(Port::try_from(40400).unwrap().get(), 40400);
        assert_eq!(Port::try_from(0).unwrap().get(), 0);
        assert_eq!(Port::try_from(65535).unwrap().get(), 65535);
        assert!(Port::try_from(-1).is_err());
        assert!(Port::try_from(70000).is_err());
    }

    #[test]
    fn length_widths() {
        assert_eq!(ByteLen::try_from(255).unwrap().get(), 255);
        assert!(ByteLen::try_from(256).is_err());
        assert_eq!(ShortLen::try_from(65535).unwrap().get(), 65535);
        assert!(ShortLen::try_from(65536).is_err());
        assert_eq!(WireLen::try_from(4_000_000_000).unwrap().get(), 4_000_000_000);
        assert!(WireLen::try_from(usize::MAX).is_err());
    }
}
