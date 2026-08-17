//! REPL client interface (port of `effects/ReplClient.scala`).

/// A thin REPL client (port of `ReplClient[F]`; the `F[_]` effect is simplified to synchronous
/// calls and `Either[Throwable, String]` becomes `Result<String, String>`).
pub trait ReplClient {
    fn run(&self, line: &str) -> Result<String, String>;

    fn eval(
        &self,
        file_names: &[String],
        print_unmatched_sends_only: bool,
    ) -> Vec<Result<String, String>>;
}
