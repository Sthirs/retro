//! Fractional position keys for ordering tasks within a column.
//!
//! Keys are strings over the alphabet `0-9A-Za-z` (62 symbols in ASCII order),
//! so byte-wise string comparison equals key order. A valid key is non-empty
//! and never ends in `'0'`, which guarantees there is always room between two
//! distinct keys.

/// Returns a key strictly between `after` and `before`. `None` for either
/// bound means "open": no lower / no upper bound. Returns `None` when the
/// bounds are not strictly ordered or when a given key is invalid (empty, ends
/// in `'0'`, or contains a character outside the alphabet).
pub fn key_between(after: Option<&str>, before: Option<&str>) -> Option<String> {
    let a = after.map_or(Some(&[][..]), |k| valid(k).then_some(k.as_bytes()))?;
    let b = match before {
        Some(k) => Some(valid(k).then_some(k.as_bytes())?),
        None => None,
    };
    if let Some(b) = b
        && a >= b
    {
        return None;
    }
    // Keys only contain ASCII alphabet characters, so this is valid UTF-8.
    String::from_utf8(midpoint(a, b)).ok()
}

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Digit value of an alphabet byte (0..=61).
fn digit(c: u8) -> usize {
    ALPHABET
        .iter()
        .position(|&x| x == c)
        .expect("validated alphabet byte")
}

fn valid(key: &str) -> bool {
    !key.is_empty() && !key.ends_with('0') && key.bytes().all(|c| ALPHABET.contains(&c))
}

/// Midpoint of `a` (digits read as zero-padded; empty means "all zeros") and
/// `b` (`None` means digit 62, i.e. above every key). Requires `a < b`, `a`
/// not ending in `'0'` and `b` not ending in `'0'`.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    if let Some(b) = b {
        // Length of the common prefix of `a` (padded with '0') and `b`.
        let n = b
            .iter()
            .enumerate()
            .take_while(|&(i, &c)| a.get(i).copied().unwrap_or(b'0') == c)
            .count();
        if n > 0 {
            let mut out = b[..n].to_vec();
            out.extend(midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..])));
            return out;
        }
    }
    let da = a.first().map_or(0, |&c| digit(c));
    let db = b.map_or(62, |b| digit(b[0]));
    if db - da > 1 {
        return vec![ALPHABET[(da + db).div_ceil(2)]];
    }
    if let Some(b) = b
        && b.len() > 1
    {
        return b[..1].to_vec();
    }
    let mut out = vec![a.first().copied().unwrap_or(b'0')];
    out.extend(midpoint(a.get(1..).unwrap_or(&[]), None));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_bounds_give_the_middle_key() {
        assert_eq!(key_between(None, None).as_deref(), Some("V"));
    }

    #[test]
    fn append_and_prepend_around_a_key() {
        assert!(key_between(Some("V"), None).unwrap().as_str() > "V");
        assert!(key_between(None, Some("V")).unwrap().as_str() < "V");
    }

    #[test]
    fn adjacent_keys_gain_a_digit() {
        assert_eq!(key_between(Some("a"), Some("b")).as_deref(), Some("aV"));
    }

    #[test]
    fn equal_or_reversed_bounds_are_rejected() {
        assert_eq!(key_between(Some("a"), Some("a")), None);
        assert_eq!(key_between(Some("b"), Some("a")), None);
    }

    #[test]
    fn invalid_keys_are_rejected() {
        assert_eq!(key_between(Some(""), None), None);
        assert_eq!(key_between(None, Some("")), None);
        assert_eq!(key_between(Some("a0"), None), None);
        assert_eq!(key_between(None, Some("a0")), None);
        assert_eq!(key_between(Some("a-"), None), None);
        assert_eq!(key_between(None, Some("é")), None);
        assert_eq!(key_between(Some("a"), Some("b0")), None);
    }

    #[test]
    fn result_is_strictly_between_for_assorted_pairs() {
        let keys = [
            "1", "V", "V1", "Va", "a", "aV", "b", "z", "zz", "zzV", "01", "0V",
        ];
        for a in keys {
            for b in keys {
                if a < b {
                    let k = key_between(Some(a), Some(b)).unwrap();
                    assert!(a < k.as_str() && k.as_str() < b, "{a} < {k} < {b}");
                    assert!(!k.ends_with('0'));
                }
            }
            let after = key_between(Some(a), None).unwrap();
            assert!(after.as_str() > a && !after.ends_with('0'));
            let before = key_between(None, Some(a)).unwrap();
            assert!(before.as_str() < a && !before.ends_with('0'));
        }
    }

    #[test]
    fn repeated_appends_stay_ordered_and_short() {
        let mut prev = key_between(None, None).unwrap();
        let mut max = prev.len();
        for _ in 0..1000 {
            let next = key_between(Some(&prev), None).unwrap();
            assert!(next > prev);
            assert!(!next.ends_with('0'));
            max = max.max(next.len());
            prev = next;
        }
        println!("max append key length: {max}");
        assert!(max < 250, "append key length {max}");
    }

    #[test]
    fn repeated_prepends_stay_ordered_and_short() {
        let first = key_between(None, None).unwrap();
        let mut head = first.clone();
        let mut max = head.len();
        for _ in 0..1000 {
            let next = key_between(None, Some(&head)).unwrap();
            assert!(next < head);
            assert!(!next.ends_with('0'));
            max = max.max(next.len());
            head = next;
        }
        println!("max prepend key length: {max}");
        assert!(max < 250, "prepend key length {max}");
    }
}
