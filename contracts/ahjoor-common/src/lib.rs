#![no_std]

use soroban_sdk::{Env, IntoVal, TryFromVal, Val, Vec};

/// Shared pagination helper used by all contracts.
///
/// Clamps `limit` to `max_limit` and returns an empty vector when `start` is
/// out of range (i.e. `start >= vec.len()`). This centralises the repeated
/// `start`/`limit` clamping and slicing logic so every paginated view behaves
/// consistently.
///
/// # Arguments
/// * `vec`       - the full collection to paginate
/// * `start`     - zero-based index of the first element to return
/// * `limit`     - maximum number of elements to return
/// * `max_limit` - hard cap applied to `limit`
///
/// # Returns
/// A new `Vec<T>` containing at most `min(limit, max_limit)` elements starting
/// at `start`, or an empty vector when `start` is out of range.
pub fn paginate<T>(vec: &Vec<T>, start: u32, limit: u32, max_limit: u32) -> Vec<T>
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    let len = vec.len();
    let mut out = Vec::new(vec.env());

    // Out-of-range start returns an empty vector.
    if start >= len {
        return out;
    }

    // Clamp the requested limit to the configured maximum.
    let effective_limit = if limit > max_limit { max_limit } else { limit };

    let end = start.saturating_add(effective_limit).min(len);
    let mut i = start;
    while i < end {
        if let Some(item) = vec.get(i) {
            out.push_back(item);
        }
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(env: &Env, n: u32) -> Vec<u32> {
        let mut v = Vec::new(env);
        let mut i = 0;
        while i < n {
            v.push_back(i);
            i += 1;
        }
        v
    }

    #[test]
    fn start_zero_returns_first_page() {
        let env = Env::default();
        let v = sample(&env, 10);
        let page = paginate(&v, 0, 3, 5);
        assert_eq!(page.len(), 3);
        assert_eq!(page.get(0), Some(0));
        assert_eq!(page.get(2), Some(2));
    }

    #[test]
    fn start_equal_len_returns_empty() {
        let env = Env::default();
        let v = sample(&env, 5);
        let page = paginate(&v, 5, 3, 5);
        assert_eq!(page.len(), 0);
    }

    #[test]
    fn start_greater_than_len_returns_empty() {
        let env = Env::default();
        let v = sample(&env, 5);
        let page = paginate(&v, 100, 3, 5);
        assert_eq!(page.len(), 0);
    }

    #[test]
    fn limit_zero_returns_empty() {
        let env = Env::default();
        let v = sample(&env, 5);
        let page = paginate(&v, 0, 0, 5);
        assert_eq!(page.len(), 0);
    }

    #[test]
    fn limit_clamped_to_max_limit() {
        let env = Env::default();
        let v = sample(&env, 10);
        let page = paginate(&v, 0, 100, 4);
        assert_eq!(page.len(), 4);
    }

    #[test]
    fn empty_vec_returns_empty() {
        let env = Env::default();
        let v: Vec<u32> = Vec::new(&env);
        let page = paginate(&v, 0, 10, 10);
        assert_eq!(page.len(), 0);
    }

    #[test]
    fn start_near_end_truncates_to_len() {
        let env = Env::default();
        let v = sample(&env, 5);
        let page = paginate(&v, 3, 10, 10);
        assert_eq!(page.len(), 2);
        assert_eq!(page.get(0), Some(3));
        assert_eq!(page.get(1), Some(4));
    }
}
