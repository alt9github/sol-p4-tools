use sha1::{Sha1, Digest};

pub fn compute_partition_postfix(stream: &str, pk: &str) -> String {
    let stream_lower = stream.to_lowercase();
    let mut hasher = Sha1::new();
    hasher.update(pk.as_bytes());
    let hash = hasher.finalize();
    let hex_char = format!("{:X}", hash[0] >> 4);
    format!("{}{}", stream_lower, hex_char)
}

pub fn get_stream_prefix(stream: &str) -> String {
    stream.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_postfix_deterministic() {
        let a = compute_partition_postfix("Dev1Next", "test_pk");
        let b = compute_partition_postfix("Dev1Next", "test_pk");
        assert_eq!(a, b);
        assert!(a.starts_with("dev1next"));
        assert_eq!(a.len(), "dev1next".len() + 1);
    }

    #[test]
    fn partition_postfix_hex_range() {
        for pk in ["a", "b", "c", "1234", "NpcDropRG_Test"] {
            let p = compute_partition_postfix("Dev1", pk);
            let last = p.chars().last().unwrap();
            assert!("0123456789ABCDEF".contains(last), "got {last}");
        }
    }
}
