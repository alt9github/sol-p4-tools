use sha1::{Sha1, Digest};

/// 16-way partition postfix — SHA1(pk) 첫 byte 의 상위 4-bit 를 hex char 1자로.
/// 결과는 `"0"` ~ `"F"` 중 하나. caller 는 `{Schema}_{postfix}.json` 형태로 조합한다.
pub fn compute_partition_postfix(pk: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(pk.as_bytes());
    let hash = hasher.finalize();
    format!("{:X}", hash[0] >> 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_postfix_deterministic() {
        let a = compute_partition_postfix("test_pk");
        let b = compute_partition_postfix("test_pk");
        assert_eq!(a, b);
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn partition_postfix_hex_range() {
        for pk in ["a", "b", "c", "1234", "abc_test"] {
            let p = compute_partition_postfix(pk);
            assert_eq!(p.len(), 1);
            let last = p.chars().next().unwrap();
            assert!("0123456789ABCDEF".contains(last), "got {last}");
        }
    }

    #[test]
    fn partition_postfix_distributes_across_buckets() {
        use std::collections::HashSet;
        let mut buckets: HashSet<String> = HashSet::new();
        // 다양한 pk 로 16개 bucket 에 걸쳐 분산되는지 약하게 검증.
        for i in 0..200 {
            buckets.insert(compute_partition_postfix(&format!("pk_{i}")));
        }
        assert!(buckets.len() > 8, "expected wide distribution, got {} buckets", buckets.len());
    }
}
