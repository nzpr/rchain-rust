//! The REPL gRPC service (port of `ReplGrpcService.scala`).

use std::sync::Arc;

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_rholang::normalizer::source_to_adt;
use rchain_rholang::runtime::RhoRuntime;
use rchain_rholang::storage_printer::{pretty_print, pretty_print_unmatched_sends};

/// `CmdRequest` (run a single line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdRequest {
    pub line: String,
}

/// `EvalRequest` (evaluate a program, optionally reporting only unmatched sends).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalRequest {
    pub program: String,
    pub print_unmatched_sends_only: bool,
}

/// `ReplResponse` (the rendered output).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplResponse {
    pub output: String,
}

/// The REPL service (port of `ReplGrpcService`).
pub struct ReplGrpcService {
    runtime: Arc<RhoRuntime>,
}

impl ReplGrpcService {
    pub fn new(runtime: Arc<RhoRuntime>) -> Self {
        ReplGrpcService { runtime }
    }

    /// Run a single line (port of `run`).
    pub async fn run(&self, request: &CmdRequest) -> ReplResponse {
        self.exec(&request.line, false).await
    }

    /// Evaluate a program (port of `eval`).
    pub async fn eval(&self, request: &EvalRequest) -> ReplResponse {
        self.exec(&request.program, request.print_unmatched_sends_only).await
    }

    async fn exec(&self, source: &str, print_unmatched_sends_only: bool) -> ReplResponse {
        // Parse first so a syntax error surfaces as `Error: ...` before evaluation.
        match source_to_adt(source) {
            Err(e) => ReplResponse {
                output: format!("Error: {e}"),
            },
            Ok(_term) => {
                let rand = Blake2b512Random::default_random();
                let eval = self.runtime.evaluate(source, &rand).await;
                let pretty_storage = if print_unmatched_sends_only {
                    pretty_print_unmatched_sends(self.runtime.as_ref()).await
                } else {
                    pretty_print(self.runtime.as_ref()).await
                };
                match eval {
                    Ok(res) => {
                        let error_str = if res.errors.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "Errors received during evaluation:\n{}\n",
                                res.errors
                                    .iter()
                                    .map(|e| e.to_string())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        };
                        ReplResponse {
                            output: format!(
                                "Deployment cost: {}\n{}Storage Contents:\n{}",
                                res.cost.value, error_str, pretty_storage
                            ),
                        }
                    }
                    Err(e) => ReplResponse {
                        output: format!("Error: {e}"),
                    },
                }
            }
        }
    }
}
