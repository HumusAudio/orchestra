// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the allocation counter with [`CountingAllocator`] installed (ADR-0016 §5).
//!
//! Each file under `tests/` is a separate binary with its own global allocator: this one
//! registers the counting allocator; `not_installed.rs` deliberately does not, to test the
//! "vacuous pass" protection.
//!
//! The counts asserted here are exact on purpose: the whole point of the tool is that "no
//! allocation" means zero, not "few". The closures therefore only do what they describe; values
//! that would allocate on drop are dropped inside the closure or after the measurement, as each
//! test states.

use std::hint::black_box;
use std::sync::Barrier;

use orchestra_test_support::{
    AllocationStats, assert_no_allocations, count_allocations, is_installed,
};
use proptest::prelude::*;

orchestra_test_support::install_counting_allocator!();

/// The allocator registered above is detected by the probe.
#[test]
fn installed_allocator_is_detected() {
    assert!(is_installed());
}

/// Pure arithmetic touches no heap at all.
#[test]
fn arithmetic_does_not_allocate() {
    let ((), stats) = count_allocations(|| {
        let x = black_box(40_u64) + black_box(2_u64);
        black_box(x);
    })
    .unwrap();
    assert_eq!(stats, AllocationStats::ZERO);
}

/// One `Vec::with_capacity` is one allocation of exactly the requested bytes, and dropping it
/// inside the closure is one deallocation.
#[test]
fn one_vec_is_one_allocation_and_one_deallocation() {
    let ((), stats) = count_allocations(|| {
        let v: Vec<u32> = Vec::with_capacity(16);
        drop(black_box(v));
    })
    .unwrap();
    assert_eq!(
        stats,
        AllocationStats {
            allocations: 1,
            reallocations: 0,
            deallocations: 1,
            // 16 elements × 4 bytes per `u32`.
            bytes_allocated: 64,
        }
    );
}

/// Growing a `Vec` beyond its capacity goes through `realloc`, which is counted as a
/// reallocation (not as an allocation plus a deallocation).
#[test]
fn growing_a_vec_is_a_reallocation() {
    // Built outside the measured window: only the growth is measured.
    let mut v: Vec<u8> = Vec::with_capacity(1);
    v.push(0);
    let ((), stats) = count_allocations(|| {
        // Capacity is 1 and full: this push must grow the buffer.
        v.push(1);
    })
    .unwrap();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 1);
    assert_eq!(stats.deallocations, 0);
    // `v` is dropped here, after the measurement.
}

/// Allocations made by another thread are not attributed to the measuring thread.
#[test]
fn other_threads_are_not_counted() {
    // Spawning a thread allocates on the spawning thread (handle, packet for the result), so the
    // worker is spawned BEFORE the measurement and synchronised with a barrier; the measured
    // window contains only barrier waits on this thread while the worker allocates.
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // Warm-up round (see below), then the measured round.
            barrier.wait();
            barrier.wait();
            // Allocated on the worker thread, between the two measured waits of the main thread.
            drop(black_box(vec![0_u8; 1024]));
            barrier.wait();
        });
        // Warm-up round: on some platforms (macOS) std's mutex and condition variable allocate
        // their OS object lazily on first use. One complete round outside the measurement makes
        // sure that this happens here, not inside the measured window.
        barrier.wait();
        let ((), stats) = count_allocations(|| {
            // Releases the worker, then waits until it has allocated and freed its buffer.
            barrier.wait();
            barrier.wait();
        })
        .unwrap();
        assert_eq!(stats, AllocationStats::ZERO);
    });
}

/// The asserting helper returns the closure's result when nothing was allocated.
#[test]
fn assert_no_allocations_passes_through_the_result() {
    let product = assert_no_allocations(|| black_box(6_u64) * black_box(7_u64));
    assert_eq!(product, 42);
}

/// The asserting helper fails the test when the closure allocates.
#[test]
#[should_panic(expected = "expected no heap activity on this thread")]
fn assert_no_allocations_fails_on_allocation() {
    assert_no_allocations(|| drop(black_box(Box::new(1_u64))));
}

proptest! {
    /// For any list of buffer sizes, allocating one zeroed buffer per size is counted as exactly
    /// that many allocations and exactly the sum of the sizes in bytes.
    ///
    /// Zero sizes are excluded because `vec![0; 0]` does not allocate (an empty `Vec` uses a
    /// dangling pointer), which is a property of `Vec`, not of our counter.
    #[test]
    fn counts_match_the_requested_buffers(sizes in prop::collection::vec(1_usize..4096, 0..32)) {
        // Room for every buffer reserved up front, so that the pushes inside the measurement
        // never reallocate `buffers` itself.
        let mut buffers: Vec<Box<[u8]>> = Vec::with_capacity(sizes.len());
        let ((), stats) = count_allocations(|| {
            for &size in &sizes {
                // `vec![0; n]` goes through `alloc_zeroed`; `into_boxed_slice` does not
                // reallocate because length and capacity are equal.
                buffers.push(vec![0_u8; size].into_boxed_slice());
            }
        })
        .unwrap();
        let expected_bytes: usize = sizes.iter().sum();
        prop_assert_eq!(stats.allocations, sizes.len() as u64);
        prop_assert_eq!(stats.reallocations, 0);
        prop_assert_eq!(stats.deallocations, 0);
        prop_assert_eq!(stats.bytes_allocated, expected_bytes as u64);
        // `buffers` is dropped after the measurement.
    }
}
