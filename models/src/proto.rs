//! Generated protobuf wire types.
//!
//! `routing` types are public (they are the transport wire format consumed directly by `comm`);
//! `casper` types are crate-private and wrapped by the hand-written domain types in
//! [`crate::casper::protocol`] (the scalapb `*Proto` / case-class split, per AGENTS.md D2).

pub mod routing {
    include!(concat!(env!("OUT_DIR"), "/routing.rs"));
}

pub mod discovery {
    include!(concat!(env!("OUT_DIR"), "/coop.rchain.comm.discovery.rs"));
}

pub(crate) mod casper {
    #![allow(dead_code)] // not all generated wire types are used until casper/comm land
    include!(concat!(env!("OUT_DIR"), "/casper.rs"));
}

pub(crate) mod rholang {
    #![allow(dead_code)] // wire types for the rholang AST, bridged by crate::wire
    include!(concat!(env!("OUT_DIR"), "/rholang.rs"));
}
