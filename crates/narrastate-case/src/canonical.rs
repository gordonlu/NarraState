use narrastate_core::{
    CaseInstance, CaseInstanceId, CompiledCase, ContentHash, Seed, VariantSelectorVersion,
};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("failed to serialize canonical content: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub fn canonical_hash<T: Serialize>(value: &T) -> Result<ContentHash, HashError> {
    let value = serde_json::to_value(value)?;
    let bytes = serde_json::to_vec(&canonicalize(value))?;
    Ok(hash_bytes(&bytes))
}

pub fn raw_content_hash(bytes: &[u8]) -> ContentHash {
    hash_bytes(bytes)
}

pub fn instance_hash(
    compiled_hash: &ContentHash,
    selector_version: VariantSelectorVersion,
    seed: Seed,
) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(compiled_hash.as_ref().as_bytes());
    hasher.update(match selector_version {
        VariantSelectorVersion::V1 => b"selector-v1".as_slice(),
    });
    hasher.update(seed.0.to_be_bytes());
    digest(hasher.finalize())
}

pub fn freeze_case(compiled_case: CompiledCase, seed: Seed) -> CaseInstance {
    let selector_version = VariantSelectorVersion::V1;
    let hash = instance_hash(&compiled_case.compiled_content_hash, selector_version, seed);
    CaseInstance {
        instance_id: CaseInstanceId::new(),
        case_id: compiled_case.case_id.clone(),
        case_version: compiled_case.case_version.clone(),
        variant_id: compiled_case.variant_id.clone(),
        selector_version,
        seed,
        compiled_content_hash: compiled_case.compiled_content_hash.clone(),
        instance_hash: hash,
        compiled_case,
    }
}

pub fn verify_compiled_hash(compiled_case: &CompiledCase) -> Result<bool, HashError> {
    let input = (
        &compiled_case.definition,
        &compiled_case.variant_id,
        &compiled_case.case_version,
        &compiled_case.schema_version,
    );
    Ok(canonical_hash(&input)? == compiled_case.compiled_content_hash)
}

pub fn verify_instance_hash(instance: &CaseInstance) -> Result<bool, HashError> {
    Ok(instance_hash(
        &instance.compiled_content_hash,
        instance.selector_version,
        instance.seed,
    ) == instance.instance_hash
        && instance.compiled_content_hash == instance.compiled_case.compiled_content_hash
        && instance.case_id == instance.compiled_case.case_id
        && instance.case_version == instance.compiled_case.case_version
        && instance.variant_id == instance.compiled_case.variant_id)
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, canonicalize(value));
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        other => other,
    }
}

fn hash_bytes(bytes: &[u8]) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    digest(hasher.finalize())
}

fn digest(bytes: impl AsRef<[u8]>) -> ContentHash {
    let mut text = String::with_capacity(71);
    text.push_str("sha256:");
    for byte in bytes.as_ref() {
        use std::fmt::Write;
        write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
    }
    ContentHash::from(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_key_order_does_not_change_hash() {
        let left = json!({"b": 2, "a": {"d": 4, "c": 3}});
        let right = json!({"a": {"c": 3, "d": 4}, "b": 2});
        assert_eq!(
            canonical_hash(&left).unwrap(),
            canonical_hash(&right).unwrap()
        );
    }

    #[test]
    fn array_order_remains_semantic() {
        assert_ne!(
            canonical_hash(&json!([1, 2])).unwrap(),
            canonical_hash(&json!([2, 1])).unwrap()
        );
    }
}
