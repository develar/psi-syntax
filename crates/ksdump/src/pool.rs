//! Work over a list on all cores, read back in the list's order: each thread keeps its own state (a `Parser` per
//! language), takes the next item from a shared counter as `--bench` does (in the order it is given, largest files
//! first), and the caller sees every result in list order as soon as it and those before it are done, so the output is
//! deterministic and only results that arrive ahead of their turn are held.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// A worker's stack: the main thread's, where these modes ran before they were parallel.
const STACK: usize = 8 << 20;

/// The threads a mode uses unless `--threads` says otherwise: every core.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// `work(state, i)` for every `i` of `order`, a permutation of `0..order.len()`, taken in that order on `threads`
/// threads, each with its own `init()`; `each(i, result)` runs on the calling thread in the order of `i`. A panic in
/// `work` propagates once the other threads have stopped.
pub fn ordered<S, R: Send>(
    order: &[usize],
    threads: usize,
    init: impl Fn() -> S + Sync,
    work: impl Fn(&mut S, usize) -> R + Sync,
    mut each: impl FnMut(usize, R),
) {
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, order.len().max(1)) {
            let sender = sender.clone();
            let (next, init, work) = (&next, &init, &work);
            let spawned = std::thread::Builder::new().stack_size(STACK).spawn_scoped(scope, move || {
                let mut state = init();
                while let Some(&i) = order.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if sender.send((i, work(&mut state, i))).is_err() {
                        break;
                    }
                }
            });
            if let Err(e) = spawned {
                eprintln!("cannot start a thread: {e}");
                std::process::exit(1);
            }
        }
        drop(sender);
        let mut pending = BTreeMap::new();
        let mut cursor = 0;
        for (i, result) in receiver {
            pending.insert(i, result);
            while let Some(result) = pending.remove(&cursor) {
                each(cursor, result);
                cursor += 1;
            }
        }
    });
}

/// `0..n`, a list's own order.
pub fn list_order(n: usize) -> Vec<usize> {
    (0..n).collect()
}

/// The indices of `sizes` from the largest to the smallest (ties in list order), so that the slowest files start first
/// rather than finish a run alone.
pub fn largest_first(sizes: &[u64]) -> Vec<usize> {
    let mut order = list_order(sizes.len());
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i]));
    order
}

#[cfg(test)]
mod tests {
    #[test]
    fn results_come_in_list_order_with_one_state_per_thread() {
        let mut seen = Vec::new();
        let sizes: Vec<u64> = (0..100).map(|i| (i * 7919 % 13) as u64).collect();
        let work = |calls: &mut usize, i: usize| {
            *calls += 1;
            std::thread::sleep(std::time::Duration::from_micros(((i * 37) % 11) as u64 * 50));
            i * 2
        };
        super::ordered(&super::largest_first(&sizes), 7, || 0usize, work, |i, r| seen.push((i, r)));
        assert_eq!(seen, (0..100).map(|i| (i, i * 2)).collect::<Vec<_>>());
        let mut none = 0;
        super::ordered(&[], 4, || (), |_, _| (), |_, _: ()| none += 1);
        assert_eq!(none, 0);
        assert_eq!(super::largest_first(&[1, 5, 1, 7]), [3, 1, 0, 2]);
    }
}
