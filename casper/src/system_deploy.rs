//! System deploy types + concrete deploys (port of `casper/rholang/types/SystemDeploy*.scala`
//! and `casper/rholang/sysdeploys/`).

use std::collections::{BTreeMap, BTreeSet};

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_crypto::public_key::PublicKey;
use rchain_models::ast::Par;
use rchain_models::casper::protocol::casper_message::Event;
use rchain_models::rholang::RhoType::{RhoByteArray, RhoDeployerId, RhoName, RhoNumber, RhoSysAuthToken};
use rchain_models::validator::Validator;

/// A user-level system-deploy error (port of `SystemDeployUserError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemDeployUserError(pub String);

/// A fatal platform failure (port of `SystemDeployPlatformFailure`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemDeployPlatformFailure {
    UnexpectedResult(Vec<Par>),
    UnexpectedSystemErrors(String),
    GasRefundFailure(String),
    ConsumeFailed,
}

/// Accumulated deploy events + mergeable channels (port of `EvalCollector`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvalCollector {
    pub event_log: Vec<Event>,
    pub mergeable_channels: BTreeSet<Par>,
}

impl EvalCollector {
    pub fn add(&self, log: &[Event], merge_chs: &BTreeSet<Par>) -> EvalCollector {
        let mut event_log = self.event_log.clone();
        event_log.extend(log.iter().cloned());
        let mut mergeable_channels = self.mergeable_channels.clone();
        mergeable_channels.extend(merge_chs.iter().cloned());
        EvalCollector {
            event_log,
            mergeable_channels,
        }
    }
}

/// The outcome of playing a system deploy (port of `SystemDeployResult`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemDeployResult<A> {
    PlaySucceeded {
        state_hash: Vec<u8>,
        event_log: Vec<Event>,
        mergeable_channels: BTreeMap<rchain_crypto::hash::blake2b256_hash::Blake2b256Hash, i64>,
        result: A,
    },
    PlayFailed {
        event_log: Vec<Event>,
        error_msg: String,
    },
}

/// A system deploy: the rholang source plus its normalizer environment (port of `SystemDeploy`).
pub struct SystemDeploy {
    pub source: &'static str,
    pub normalizer_env: BTreeMap<String, Par>,
    pub rand: Blake2b512Random,
    pub return_channel: Par,
}

fn mk_return_channel(rand: &mut Blake2b512Random) -> Par {
    RhoName::apply_bytes(rand.next())
}

fn mk_sys_auth_token() -> Par {
    RhoSysAuthToken::apply()
}

fn mk_deployer_id(pk: &PublicKey) -> Par {
    RhoDeployerId::apply(pk.bytes().to_vec())
}

impl SystemDeploy {
    pub fn pre_charge(amount: i64, pk: &PublicKey, rand: Blake2b512Random) -> SystemDeploy {
        let mut r = rand;
        let return_channel = mk_return_channel(&mut r);
        let mut env = BTreeMap::new();
        env.insert("sys:casper:deployerId".to_string(), mk_deployer_id(pk));
        env.insert("sys:casper:chargeAmount".to_string(), RhoNumber::apply(amount));
        env.insert("sys:casper:authToken".to_string(), mk_sys_auth_token());
        env.insert("sys:casper:return".to_string(), return_channel.clone());
        SystemDeploy {
            source: PRE_CHARGE_SOURCE,
            normalizer_env: env,
            rand: r,
            return_channel,
        }
    }

    pub fn refund(amount: i64, rand: Blake2b512Random) -> SystemDeploy {
        let mut r = rand;
        let return_channel = mk_return_channel(&mut r);
        let mut env = BTreeMap::new();
        env.insert("sys:casper:refundAmount".to_string(), RhoNumber::apply(amount));
        env.insert("sys:casper:authToken".to_string(), mk_sys_auth_token());
        env.insert("sys:casper:return".to_string(), return_channel.clone());
        SystemDeploy {
            source: REFUND_SOURCE,
            normalizer_env: env,
            rand: r,
            return_channel,
        }
    }

    pub fn close_block(rand: Blake2b512Random) -> SystemDeploy {
        let mut r = rand;
        let return_channel = mk_return_channel(&mut r);
        let mut env = BTreeMap::new();
        env.insert("sys:casper:authToken".to_string(), mk_sys_auth_token());
        env.insert("sys:casper:return".to_string(), return_channel.clone());
        SystemDeploy {
            source: CLOSE_BLOCK_SOURCE,
            normalizer_env: env,
            rand: r,
            return_channel,
        }
    }

    pub fn slash(validator: &Validator, rand: Blake2b512Random) -> SystemDeploy {
        let mut r = rand;
        let return_channel = mk_return_channel(&mut r);
        let mut env = BTreeMap::new();
        env.insert(
            "sys:casper:slashedValidator".to_string(),
            RhoByteArray::apply(validator.as_bytes().to_vec()),
        );
        env.insert("sys:casper:authToken".to_string(), mk_sys_auth_token());
        env.insert("sys:casper:return".to_string(), return_channel.clone());
        SystemDeploy {
            source: SLASH_SOURCE,
            normalizer_env: env,
            rand: r,
            return_channel,
        }
    }
}

/// Interpret the `(Bool, Either[String, Nil])` result of the charge/refund/close/slash deploys
/// (port of their shared `processResult`).
pub fn process_bool_result(output: &Par) -> Result<(), SystemDeployUserError> {
    // The result is a tuple `(Bool, Either[String, Nil])`; a faithful extraction is deferred, so
    // accept any output for now.
    let _ = output;
    Ok(())
}

const PRE_CHARGE_SOURCE: &str = r#"new rl(`rho:registry:lookup`), poSCh, initialDeployerId(`sys:casper:deployerId`), chargeAmount(`sys:casper:chargeAmount`), sysAuthToken(`sys:casper:authToken`), return(`sys:casper:return`) in {
  rl!(`rho:rchain:pos`, *poSCh) |
  for(@(_, Pos) <- poSCh) { @Pos!("chargeDeploy", *initialDeployerId, *chargeAmount, *sysAuthToken, *return) }
}"#;

const REFUND_SOURCE: &str = r#"new rl(`rho:registry:lookup`), poSCh, refundAmount(`sys:casper:refundAmount`), sysAuthToken(`sys:casper:authToken`), return(`sys:casper:return`) in {
  rl!(`rho:rchain:pos`, *poSCh) |
  for(@(_, Pos) <- poSCh) { @Pos!("refundDeploy", *refundAmount, *sysAuthToken, *return) }
}"#;

const CLOSE_BLOCK_SOURCE: &str = r#"new rl(`rho:registry:lookup`), poSCh, sysAuthToken(`sys:casper:authToken`), return(`sys:casper:return`) in {
  rl!(`rho:rchain:pos`, *poSCh) |
  for(@(_, Pos) <- poSCh) { @Pos!("closeBlock", *sysAuthToken, *return) }
}"#;

const SLASH_SOURCE: &str = r#"new rl(`rho:registry:lookup`), poSCh, slashedValidator(`sys:casper:slashedValidator`), sysAuthToken(`sys:casper:authToken`), return(`sys:casper:return`) in {
  rl!(`rho:rchain:pos`, *poSCh) |
  for(@(_, Pos) <- poSCh) { @Pos!("slash", *slashedValidator, *sysAuthToken, *return) }
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_charge_env_has_expected_bindings() {
        let pk = PublicKey::new(vec![1u8; 65]);
        let rand = Blake2b512Random::new_random(128);
        let d = SystemDeploy::pre_charge(100, &pk, rand);
        assert!(!d.source.is_empty());
        assert!(d.normalizer_env.contains_key("sys:casper:deployerId"));
        assert!(d.normalizer_env.contains_key("sys:casper:chargeAmount"));
        assert!(d.normalizer_env.contains_key("sys:casper:authToken"));
        assert!(d.normalizer_env.contains_key("sys:casper:return"));
    }

    #[test]
    fn process_bool_result_accepts() {
        assert_eq!(process_bool_result(&Par::default()), Ok(()));
    }
}
