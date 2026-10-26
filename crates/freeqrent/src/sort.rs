//! One stable sort for the whole crate. `slice::sort_by` compiles a full
//! sort for every call site, several KB of WASM each; this sorts indices
//! with one non-generic merge sort, so each call site adds only a small
//! permutation (SPEC §2's size budget, W20261026180849).

use alloc::vec::Vec;
use core::cmp::Ordering;

/// Sorts `v` stably by `cmp`, like `slice::sort_by`.
pub(crate) fn sort_by<T: Clone>(v: &mut [T], mut cmp: impl FnMut(&T, &T) -> Ordering) {
    if v.len() < 2 {
        return;
    }
    let order = merge_order(v.len(), &mut |a, b| cmp(&v[a], &v[b]));
    let sorted: Vec<T> = order.iter().map(|&i| v[i].clone()).collect();
    v.clone_from_slice(&sorted);
}

/// Sorts `v` stably by `key`, like `slice::sort_by_key`.
pub(crate) fn sort_by_key<T: Clone, K: Ord>(v: &mut [T], mut key: impl FnMut(&T) -> K) {
    sort_by(v, |a, b| key(a).cmp(&key(b)));
}

/// The stable sorted order of `0..n`: a bottom-up merge sort.
fn merge_order(n: usize, cmp: &mut dyn FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut a: Vec<usize> = (0..n).collect();
    let mut b = alloc::vec![0; n];
    let mut width = 1;
    while width < n {
        let mut start = 0;
        while start < n {
            let mid = (start + width).min(n);
            let end = (start + 2 * width).min(n);
            let (mut l, mut r) = (start, mid);
            for slot in &mut b[start..end] {
                // The left run wins ties, which keeps the sort stable.
                let take_right = l == mid || (r < end && cmp(a[r], a[l]) == Ordering::Less);
                if take_right {
                    *slot = a[r];
                    r += 1;
                } else {
                    *slot = a[l];
                    l += 1;
                }
            }
            start = end;
        }
        core::mem::swap(&mut a, &mut b);
        width *= 2;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    #[test]
    fn matches_the_std_stable_sort() {
        let mut rng = SplitMix64::new(7);
        for n in 0..200 {
            let v: Vec<(u32, usize)> = (0..n).map(|i| ((rng.next_u64() % 8) as u32, i)).collect();
            let (mut ours, mut std) = (v.clone(), v);
            sort_by_key(&mut ours, |a| a.0);
            std.sort_by_key(|a| a.0);
            assert_eq!(ours, std, "n {n}");
        }
    }
}
