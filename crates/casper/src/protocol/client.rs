//! Listen-at-name client types (port of the `Name` ADT in `ListenAtName.scala`).

/// A name to listen at (port of `ListenAtName.Name`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Name {
    PrivName(String),
    PubName(String),
}
