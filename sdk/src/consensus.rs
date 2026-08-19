//! Consensus stake helpers.
//!
//! Mirrors `sdk/src/main/scala/coop/rchain/sdk/consensus/Stake.scala`.
//! Law 14: finality requires strictly more than 2/3 of bonded stake.

/// Check if `stake` prevails the 2/3 supermajority threshold of `total_stake`.
///
/// Law 14: strictly more than 2/3 of bonded stake. This is the *exact* integer comparison
/// `3 * stake > 2 * total_stake` (computed in `i128`, so it cannot overflow); the Scala
/// `stake.toDouble / totalStake > 2d / 3` loses precision for stakes ≥ 2⁵³ and is
/// rounding-dependent at the exact 2/3 boundary.
pub fn is_super_majority(stake: i64, total_stake: i64) -> bool {
    (stake as i128) * 3 > (total_stake as i128) * 2
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

    #[test]
    fn large_stake_just_above_two_thirds_is_exact() {
        // stake = 2·2^53 + 1, total = 3·2^53: the ratio is just above 2/3. The old f64 form
        // cannot represent 2·2^53+1 (it rounds to 2·2^53) and misclassifies this as "not a
        // supermajority"; the exact integer form is correct.
        let stake = 2 * (1i64 << 53) + 1;
        let total = 3 * (1i64 << 53);
        assert!(is_super_majority(stake, total));
    }
}
