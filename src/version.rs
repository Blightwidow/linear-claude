pub const VERSION: &str = "v0.1.1";

/// Returns true if version `a` is less than version `b`.
/// Both versions may optionally start with 'v'.
pub fn version_lt(a: &str, b: &str) -> bool {
    let a = a.strip_prefix('v').unwrap_or(a);
    let b = b.strip_prefix('v').unwrap_or(b);

    let a_parts: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let b_parts: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();

    let max_len = a_parts.len().max(b_parts.len());
    for i in 0..max_len {
        let a_num = a_parts.get(i).copied().unwrap_or(0);
        let b_num = b_parts.get(i).copied().unwrap_or(0);
        if a_num < b_num {
            return true;
        }
        if a_num > b_num {
            return false;
        }
    }
    false // equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_lt_basic() {
        assert!(version_lt("0.1.0", "0.1.1"));
        assert!(version_lt("0.1.1", "0.2.0"));
        assert!(version_lt("0.9.9", "1.0.0"));
        assert!(!version_lt("1.0.0", "0.9.9"));
        assert!(!version_lt("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_lt_with_v_prefix() {
        assert!(version_lt("v0.1.0", "v0.1.1"));
        assert!(version_lt("v0.1.0", "0.1.1"));
        assert!(version_lt("0.1.0", "v0.1.1"));
    }

    #[test]
    fn test_version_lt_different_lengths() {
        assert!(version_lt("1.0", "1.0.1"));
        assert!(!version_lt("1.0.1", "1.0"));
    }
}
