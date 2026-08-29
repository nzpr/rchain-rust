//! Consensus stake helpers.
//!
//! Mirrors `sdk/src/main/scala/coop/rchain/sdk/consensus/Stake.scala`.
//! Law 14: finality requires strictly more than 2/3 of bonded stake.

use std::collections::{BTreeMap, BTreeSet};

use rchain_shared::refined::NonNegI64;

/// Check if `stake` prevails the 2/3 supermajority threshold of `total_stake`.
///
/// Law 14: strictly more than 2/3 of bonded stake. This is the *exact* integer comparison
/// `3 * stake > 2 * total_stake` (computed in `i128`, so it cannot overflow); the Scala
/// `stake.toDouble / totalStake > 2d / 3` loses precision for stakes ≥ 2⁵³ and is
/// rounding-dependent at the exact 2/3 boundary.
///
/// Takes `i128` so callers accumulate bonded-stake sums in `i128` (a validator set with large
/// stakes can overflow `i64` before reaching this comparison).
pub fn is_super_majority(stake: i128, total_stake: i128) -> bool {
    stake * 3 > total_stake * 2
}

/// Why a proposed finality certificate is not a Law 14 capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CertificateError {
    AttestersLackSupermajority,
}

/// An opaque capability witnessing that a complete finality certificate passed Law 14.
///
/// Its fields are deliberately private. Callers cannot manufacture this value without validating
/// both levels of the certificate against one immutable committee snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalityCertificate<S> {
    attesters: BTreeSet<S>,
    candidate_senders: BTreeSet<S>,
}

impl<S: Ord + Clone> FinalityCertificate<S> {
    /// Validate a certificate whose outer keys are attesters and whose inner map records, for each
    /// candidate sender, the validators observing that candidate.
    pub fn validate(
        observations: &BTreeMap<S, BTreeMap<S, BTreeSet<S>>>,
        committee: &BTreeMap<S, NonNegI64>,
    ) -> Result<Self, CertificateError> {
        let members: BTreeSet<S> = committee.keys().cloned().collect();
        let total: i128 = committee.values().map(|s| i128::from(i64::from(*s))).sum();
        let mut attesters = BTreeSet::new();

        for (attester, candidates) in observations {
            if !committee.contains_key(attester) {
                continue;
            }
            if candidates.keys().cloned().collect::<BTreeSet<_>>() != members {
                continue;
            }

            let mut valid = true;
            for supporters in candidates.values() {
                // Non-committee identities carry no stake. Ignoring them prevents an observer or
                // stale validator identity from poisoning an otherwise valid certificate, while
                // the strict weighted threshold still prevents them from helping form a quorum.
                let support: i128 = supporters
                    .iter()
                    .filter_map(|supporter| committee.get(supporter))
                    .map(|stake| i128::from(i64::from(*stake)))
                    .sum();
                if !is_super_majority(support, total) {
                    valid = false;
                }
            }
            if valid {
                attesters.insert(attester.clone());
            }
        }

        let attesting_stake: i128 = attesters
            .iter()
            .map(|sender| i128::from(i64::from(committee[sender])))
            .sum();
        if !is_super_majority(attesting_stake, total) {
            return Err(CertificateError::AttestersLackSupermajority);
        }

        Ok(Self {
            attesters,
            candidate_senders: members,
        })
    }

    pub fn attesters(&self) -> &BTreeSet<S> {
        &self.attesters
    }

    pub fn candidate_senders(&self) -> &BTreeSet<S> {
        &self.candidate_senders
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;
    use rchain_shared::refined::NonNegI64;

    use super::{is_super_majority, FinalityCertificate};

    fn stake(value: i64) -> NonNegI64 {
        NonNegI64::try_from(value).unwrap()
    }

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
        let stake = 2 * (1i128 << 53) + 1;
        let total = 3 * (1i128 << 53);
        assert!(is_super_majority(stake, total));
    }

    #[test]
    fn i64_overflowing_stakes_do_not_wrap() {
        // A validator set whose total bonded stake exceeds i64::MAX must still compare exactly.
        // Two validators at i64::MAX each sum past i64::MAX in i64 arithmetic but fit in i128.
        let total = i128::from(i64::MAX) * 3;
        let two_thirds = i128::from(i64::MAX) * 2;
        assert!(!is_super_majority(two_thirds, total));
        assert!(is_super_majority(two_thirds + 1, total));
    }

    #[test]
    fn certificate_is_an_opaque_witness_of_both_quorums() {
        let committee = BTreeMap::from([
            (0, stake(10)),
            (1, stake(10)),
            (2, stake(10)),
            (3, stake(10)),
        ]);
        let supporters: BTreeSet<_> = [0, 1, 2].into_iter().collect();
        let candidates: BTreeMap<_, _> = committee
            .keys()
            .map(|candidate| (*candidate, supporters.clone()))
            .collect();
        let observations = BTreeMap::from([
            (0, candidates.clone()),
            (1, candidates.clone()),
            (2, candidates),
        ]);

        let proof = FinalityCertificate::validate(&observations, &committee).unwrap();
        assert_eq!(proof.attesters(), &supporters);
        assert_eq!(
            proof.candidate_senders(),
            &committee.keys().copied().collect()
        );
    }

    #[test]
    fn certificate_ignores_unknown_identity_instead_of_counting_or_failing_it() {
        let committee = BTreeMap::from([(0, stake(1)), (1, stake(1)), (2, stake(1))]);
        let supporters: BTreeSet<_> = [0, 1, 2, 99].into_iter().collect();
        let candidates: BTreeMap<_, _> =
            committee.keys().map(|s| (*s, supporters.clone())).collect();
        let observations = BTreeMap::from([
            (0, candidates.clone()),
            (1, candidates.clone()),
            (2, candidates),
        ]);
        let certificate = FinalityCertificate::validate(&observations, &committee).unwrap();
        assert_eq!(certificate.attesters(), &BTreeSet::from([0, 1, 2]));
    }

    #[test]
    fn byzantine_minority_cannot_forge_a_certificate_with_unknown_supporters() {
        let committee = BTreeMap::from([
            (0, stake(10)),
            (1, stake(10)),
            (2, stake(10)),
            (3, stake(10)),
        ]);
        // Two honest identities plus arbitrary forged identities are exactly half the stake.
        // Unknown identities must never turn this into a quorum.
        let supporters: BTreeSet<_> = [0, 1, 9001, 9002].into_iter().collect();
        let candidates: BTreeMap<_, _> = committee
            .keys()
            .map(|candidate| (*candidate, supporters.clone()))
            .collect();
        let observations = BTreeMap::from([(0, candidates.clone()), (1, candidates)]);
        assert!(FinalityCertificate::validate(&observations, &committee).is_err());
    }

    proptest! {
        #[test]
        fn strict_quorums_intersect_above_one_third(
            stakes in prop::collection::vec(0u16..10_000, 1..12),
            left_mask in any::<u16>(),
            right_mask in any::<u16>(),
        ) {
            let total: i128 = stakes.iter().map(|s| i128::from(*s)).sum();
            let left: i128 = stakes.iter().enumerate()
                .filter(|(i, _)| left_mask & (1 << i) != 0).map(|(_, s)| i128::from(*s)).sum();
            let right: i128 = stakes.iter().enumerate()
                .filter(|(i, _)| right_mask & (1 << i) != 0).map(|(_, s)| i128::from(*s)).sum();
            let intersection: i128 = stakes.iter().enumerate()
                .filter(|(i, _)| left_mask & right_mask & (1 << i) != 0)
                .map(|(_, s)| i128::from(*s)).sum();
            if is_super_majority(left, total) && is_super_majority(right, total) {
                prop_assert!(intersection * 3 > total);
            }
        }
    }
}
