//! Admin web API interface (port of `api/AdminWebApi.scala`).
//!
//! The `AdminWebApiImpl` (which delegates to the casper `BlockApi`) is deferred.

/// The admin web API contract (port of `AdminWebApi[F]`; the `F[_]` effect is simplified to
/// synchronous calls, matching the `WebApi` trait).
pub trait AdminWebApi {
    fn propose(&self) -> String;

    fn propose_result(&self) -> String;
}
