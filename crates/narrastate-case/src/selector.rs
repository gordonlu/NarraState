use narrastate_core::{CaseId, Seed, VariantId, VariantSelection};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantCandidate {
    pub id: VariantId,
    pub weight: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SelectionError {
    #[error("no eligible variants")]
    NoEligibleVariants,
    #[error("variant {0} is not eligible")]
    VariantNotEligible(VariantId),
    #[error("total variant weight overflowed")]
    WeightOverflow,
    #[error("random selection has no positive-weight variants")]
    NoPositiveWeight,
}

pub fn select_variant(
    case_id: &CaseId,
    case_version: &str,
    default_variant_id: &VariantId,
    selection: &VariantSelection,
    seed: Seed,
    candidates: &[VariantCandidate],
) -> Result<VariantId, SelectionError> {
    if candidates.is_empty() {
        return Err(SelectionError::NoEligibleVariants);
    }
    match selection {
        VariantSelection::Default => eligible(default_variant_id, candidates),
        VariantSelection::Specific(id) => eligible(id, candidates),
        VariantSelection::Random => random(case_id, case_version, seed, candidates),
    }
}

fn eligible(id: &VariantId, candidates: &[VariantCandidate]) -> Result<VariantId, SelectionError> {
    candidates
        .iter()
        .find(|candidate| &candidate.id == id)
        .map(|candidate| candidate.id.clone())
        .ok_or_else(|| SelectionError::VariantNotEligible(id.clone()))
}

fn random(
    case_id: &CaseId,
    case_version: &str,
    seed: Seed,
    candidates: &[VariantCandidate],
) -> Result<VariantId, SelectionError> {
    let mut candidates: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.weight > 0)
        .cloned()
        .collect();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let total = candidates.iter().try_fold(0_u64, |total, candidate| {
        total.checked_add(u64::from(candidate.weight))
    });
    let total = total.ok_or(SelectionError::WeightOverflow)?;
    if total == 0 {
        return Err(SelectionError::NoPositiveWeight);
    }

    let mut hasher = Sha256::new();
    hasher.update(case_id.as_ref().as_bytes());
    hasher.update(case_version.as_bytes());
    hasher.update(seed.0.to_be_bytes());
    hasher.update(b"selector-v1");
    let digest = hasher.finalize();
    let value = u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 has at least 8 bytes"),
    );
    let selected = value % total;
    let mut cursor = 0_u64;
    for candidate in candidates {
        cursor += u64::from(candidate.weight);
        if selected < cursor {
            return Ok(candidate.id);
        }
    }
    unreachable!("selected modulo total must fall inside cumulative weights")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<VariantCandidate> {
        vec![
            VariantCandidate {
                id: "b".into(),
                weight: 2,
            },
            VariantCandidate {
                id: "a".into(),
                weight: 1,
            },
            VariantCandidate {
                id: "disabled-by-caller".into(),
                weight: 0,
            },
        ]
    }

    #[test]
    fn same_seed_is_reproducible_and_input_order_independent() {
        let mut reversed = candidates();
        reversed.reverse();
        let args = (CaseId::from("case"), VariantId::from("a"), Seed(928_341));
        let first = select_variant(
            &args.0,
            "1.0.0",
            &args.1,
            &VariantSelection::Random,
            args.2,
            &candidates(),
        )
        .unwrap();
        let second = select_variant(
            &args.0,
            "1.0.0",
            &args.1,
            &VariantSelection::Random,
            args.2,
            &reversed,
        )
        .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn random_rejects_all_zero_weights() {
        let result = select_variant(
            &CaseId::from("case"),
            "1.0.0",
            &VariantId::from("a"),
            &VariantSelection::Random,
            Seed(1),
            &[VariantCandidate {
                id: "a".into(),
                weight: 0,
            }],
        );
        assert_eq!(result, Err(SelectionError::NoPositiveWeight));
    }

    #[test]
    fn specific_rejects_ineligible_variant() {
        let result = select_variant(
            &CaseId::from("case"),
            "1.0.0",
            &VariantId::from("a"),
            &VariantSelection::Specific(VariantId::from("missing")),
            Seed(1),
            &candidates(),
        );
        assert_eq!(
            result,
            Err(SelectionError::VariantNotEligible(VariantId::from(
                "missing"
            )))
        );
    }
}
