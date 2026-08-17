//! Consensus stake helpers.
//!
//! Mirrors `sdk/src/main/scala/coop/rchain/sdk/consensus/Stake.scala`.
//! Law 14: finality requires strictly more than 2/3 of bonded stake.

/// Check if `stake` prevails the 2/3 supermajority threshold of `total_stake`.
///
/// This is a faithful port of the Scala floating-point expression
/// `stake.toDouble / totalStake > 2d / 3` (kept as `f64` to preserve its exact behaviour).
pub fn is_super_majority(stake: i64, total_stake: i64) -> bool {
    (stake as f64) / (total_stake as f64) > 2.0 / 3.0
}

#[cfg(test)]
mod tests {
    use super::is_super_majority;

    #[test]
    fn two_thirds_is_not_supermajority() {
        // 2/3 exactly is NOT > 2/3.
        assert!(!is_super_majority(2, 3));
    }

    #[test]
    fn above_two_thirds_is_supermajority() {
        assert!(is_super_majority(3, 4));
    }

    #[test]
    fn below_two_thirds_is_not_supermajority() {
        assert!(!is_super_majority(1, 3));
    }
}
