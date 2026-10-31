// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-test-support — helpers for tests and benchmarks only
//!
//! ## Purpose
//! Provide the **allocation-counting global allocator** required by ADR-0007 rule 1 ("a test
//! fails if the audio thread allocates") and by ADR-0015 §9 (every `Rational` operation is
//! allocation-free). The design is fixed by ADR-0016 §5–§6.
//!
//! ## How to use it
//! A test (or benchmark) binary registers the allocator once, at the top level of the file, and
//! then measures closures:
//!
//! ```ignore
//! orchestra_test_support::install_counting_allocator!();
//!
//! #[test]
//! fn addition_never_allocates() {
//!     let sum = orchestra_test_support::assert_no_allocations(|| 2 + 2);
//!     assert_eq!(sum, 4);
//! }
//! ```
//!
//! (The example is `ignore`d because a doctest cannot register a global allocator in a way that
//! is meaningful for the example itself; the real tests live in `tests/`.)
//!
//! ## Why per-thread counters?
//! Test runners execute tests in parallel threads (`cargo test`) and the program itself has many
//! threads (UI, audio, disk). A single global counter would mix the allocations of every thread,
//! so a test asserting "this closure does not allocate" would fail at random whenever another test
//! happened to allocate at the same time. Each thread therefore owns its counters, and a
//! measurement only looks at the thread that runs the measured closure. This is also exactly what
//! the audio-thread test of Phase 3 needs: "the audio thread does not allocate", whatever the
//! other threads do.
//!
//! ## What this crate must NOT do
//! - Be used outside `[dev-dependencies]` (ADR-0016 §6, checked by `cargo xtask check`): a
//!   counting allocator in the shipped program would cost a thread-local update on every
//!   allocation for no benefit.
//! - Depend on any workspace crate: it must stay at the bottom of the graph.
//!
//! ## Allowed dependencies
//! None (std only).
//!
//! ## Unsafe code
//! This crate is designated for `unsafe` (ADR-0016 §6) because implementing [`GlobalAlloc`] is
//! `unsafe` by definition. The only unsafe operations are the forwarding calls to the system
//! allocator; each carries a `// SAFETY:` comment.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::hint::black_box;

/// Allocation activity of one thread, as counted by [`CountingAllocator`].
///
/// The counters are monotonically increasing totals since the thread started; a measurement is
/// the difference of two snapshots ([`AllocationStats::since`]).
///
/// # Units and semantics
/// - `allocations`: calls to `alloc` and `alloc_zeroed` (successful or not: a failed request
///   still entered the system allocator, which is what real-time code must avoid).
/// - `reallocations`: calls to `realloc` (growing a `Vec`, for example).
/// - `deallocations`: calls to `dealloc`. Freeing memory counts as heap activity too: `free` may
///   take a lock inside the system allocator, so the audio thread must not do it either.
/// - `bytes_allocated`: bytes **requested** by `alloc`/`alloc_zeroed` plus the **new** size passed
///   to `realloc`. It is a measure of demand, not of memory in use (frees are not subtracted).
///
/// # Overflow
/// The counters are `u64` and use wrapping arithmetic: overflowing them would need 2⁶⁴
/// allocations on one thread (centuries at a billion per second), and a panic inside an allocator
/// would abort the process, so wrapping is the only safe choice. [`AllocationStats::since`] uses
/// wrapping subtraction, which stays correct across a (theoretical) wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllocationStats {
    /// Calls to `alloc` and `alloc_zeroed`.
    pub allocations: u64,
    /// Calls to `realloc`.
    pub reallocations: u64,
    /// Calls to `dealloc`.
    pub deallocations: u64,
    /// Bytes requested by `alloc`/`alloc_zeroed` plus the new sizes passed to `realloc`.
    pub bytes_allocated: u64,
}

impl AllocationStats {
    /// All counters at zero: the state of a thread that never touched the heap.
    ///
    /// A `const` (rather than `Default::default()`) so that it can initialise the thread-local
    /// counters in a `const` block, which is what keeps their access allocation-free (see
    /// [`COUNTERS`]).
    pub const ZERO: Self = Self {
        allocations: 0,
        reallocations: 0,
        deallocations: 0,
        bytes_allocated: 0,
    };

    /// Returns `true` if there was no heap activity at all: no allocation, no reallocation and no
    /// deallocation. This is the condition real-time code must satisfy.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.allocations == 0 && self.reallocations == 0 && self.deallocations == 0
    }

    /// The activity between an `earlier` snapshot and `self` (a later snapshot of the same
    /// thread), counter by counter.
    ///
    /// Wrapping subtraction: see the "Overflow" section of the type. O(1).
    #[must_use]
    pub const fn since(&self, earlier: &Self) -> Self {
        Self {
            allocations: self.allocations.wrapping_sub(earlier.allocations),
            reallocations: self.reallocations.wrapping_sub(earlier.reallocations),
            deallocations: self.deallocations.wrapping_sub(earlier.deallocations),
            bytes_allocated: self.bytes_allocated.wrapping_sub(earlier.bytes_allocated),
        }
    }
}

thread_local! {
    /// The counters of the current thread.
    ///
    /// # Why this is safe to touch from inside the allocator
    /// The allocator must never allocate while counting, or it would recurse forever. Three
    /// properties guarantee that reading and writing this cell never allocates:
    /// 1. the initialiser is a `const` block, so there is no lazy initialisation on first access;
    /// 2. `Cell<AllocationStats>` has no destructor (`AllocationStats` is `Copy`), so std does not
    ///    need to register a thread-exit destructor (registration may allocate);
    /// 3. with 1 and 2, std uses the platform's native thread-local storage on Linux, Windows and
    ///    macOS (the `#[thread_local]` fast path), which is a plain memory access.
    ///
    /// `Cell` rather than atomics: the value is only ever touched by its own thread, so a plain
    /// load and store are enough and cost nothing extra.
    static COUNTERS: Cell<AllocationStats> = const { Cell::new(AllocationStats::ZERO) };
}

/// Applies `update` to the current thread's counters.
///
/// Uses `try_with`, which never panics: during thread teardown the thread-local may already be
/// gone, and a panic inside the allocator would abort the process. In that window the event is
/// simply not counted, which is harmless (no measurement can be running on a dying thread).
///
/// O(1): one thread-local load, the update, one store.
#[inline]
fn record(update: impl FnOnce(&mut AllocationStats)) {
    // The `Result` only says whether the thread-local was still alive; see above for why
    // ignoring it is correct.
    let _ = COUNTERS.try_with(|cell| {
        let mut stats = cell.get();
        update(&mut stats);
        cell.set(stats);
    });
}

/// Converts a byte count to `u64` for the statistics.
///
/// `usize` is at most 64 bits on every platform we support, so the conversion is lossless in
/// practice; the saturating fallback only exists so that no `as` cast (and no panic) is needed.
#[inline]
fn bytes(size: usize) -> u64 {
    u64::try_from(size).unwrap_or(u64::MAX)
}

/// A global allocator that forwards every request to the system allocator and counts it in the
/// calling thread's [`AllocationStats`].
///
/// Register it with [`install_counting_allocator!`] in each test or benchmark binary that needs
/// it. Without registration every count stays at zero, which would make "no allocation" tests
/// pass vacuously: [`count_allocations`] therefore checks [`is_installed`] first.
///
/// # Cost
/// One thread-local read-modify-write per allocator call, on top of the system allocator.
#[derive(Debug, Clone, Copy, Default)]
pub struct CountingAllocator;

// SAFETY: every method forwards to `System`, which implements `GlobalAlloc` correctly, with
// exactly the arguments it received, and returns `System`'s result unchanged. The bookkeeping
// done before forwarding never allocates (see `COUNTERS`), never panics (`record` uses
// `try_with`, the arithmetic wraps) and never touches the memory being managed, so the contract
// of `GlobalAlloc` (returned blocks fit the layout, no unwinding, no reentrancy into ourselves)
// is exactly the contract `System` already satisfies.
unsafe impl GlobalAlloc for CountingAllocator {
    /// Counts one allocation of `layout.size()` bytes, then forwards to `System::alloc`.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(|s| {
            s.allocations = s.allocations.wrapping_add(1);
            s.bytes_allocated = s.bytes_allocated.wrapping_add(bytes(layout.size()));
        });
        // SAFETY: the caller guarantees what `GlobalAlloc::alloc` requires (a non-zero-sized
        // `layout`); we pass it through unchanged.
        unsafe { System.alloc(layout) }
    }

    /// Counts one allocation of `layout.size()` bytes, then forwards to `System::alloc_zeroed`.
    ///
    /// Overridden (instead of relying on the default `alloc` + `write_bytes`) so that the system
    /// allocator can hand out pages that are already zero, as it would without us.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(|s| {
            s.allocations = s.allocations.wrapping_add(1);
            s.bytes_allocated = s.bytes_allocated.wrapping_add(bytes(layout.size()));
        });
        // SAFETY: same requirements as `alloc`, guaranteed by the caller and passed through.
        unsafe { System.alloc_zeroed(layout) }
    }

    /// Counts one deallocation, then forwards to `System::dealloc`.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(|s| s.deallocations = s.deallocations.wrapping_add(1));
        // SAFETY: the caller guarantees that `ptr` was allocated by this allocator with
        // `layout`; every block we hand out comes from `System`, so `ptr` was allocated by
        // `System` with that same layout, which is what `System::dealloc` requires.
        unsafe { System.dealloc(ptr, layout) }
    }

    /// Counts one reallocation to `new_size` bytes, then forwards to `System::realloc`.
    ///
    /// Overridden (instead of the default `alloc` + copy + `dealloc`) so that `System` can grow a
    /// block in place, and so that a reallocation is counted as one event, not as two.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(|s| {
            s.reallocations = s.reallocations.wrapping_add(1);
            s.bytes_allocated = s.bytes_allocated.wrapping_add(bytes(new_size));
        });
        // SAFETY: the caller guarantees the `GlobalAlloc::realloc` requirements (`ptr` allocated
        // by us with `layout`, valid non-zero `new_size`); as in `dealloc`, `ptr` really comes
        // from `System` with that layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Registers [`CountingAllocator`] as the global allocator of the current binary.
///
/// Use it once, at the top level of a test or benchmark file (each file under `tests/` and
/// `benches/` is its own binary). A second use in the same binary is a compile error, as for any
/// duplicate `#[global_allocator]`.
#[macro_export]
macro_rules! install_counting_allocator {
    () => {
        /// The allocation-counting global allocator of this test binary (ADR-0016 §5).
        #[global_allocator]
        static ORCHESTRA_COUNTING_ALLOCATOR: $crate::CountingAllocator = $crate::CountingAllocator;
    };
}

/// Returns a snapshot of the current thread's allocation totals.
///
/// Never allocates. O(1).
#[must_use]
pub fn thread_stats() -> AllocationStats {
    // `try_with` fails only during thread teardown; report zero activity then (see `record`).
    COUNTERS
        .try_with(Cell::get)
        .unwrap_or(AllocationStats::ZERO)
}

/// Returns `true` if [`CountingAllocator`] is the global allocator of this binary.
///
/// # How
/// It performs one tiny probe allocation and checks that the current thread's counter moved.
/// `black_box` stops the optimiser from removing the allocation (allocation elision is legal for
/// a `Box` that is created and dropped without being observed).
///
/// # Cost
/// One allocation and one deallocation: do not call it inside a measurement.
#[must_use]
pub fn is_installed() -> bool {
    let before = thread_stats();
    drop(black_box(Box::new(0_u8)));
    thread_stats().since(&before).allocations > 0
}

/// Error returned when a measurement is requested but [`CountingAllocator`] is not the global
/// allocator, so every count would be zero and the measurement meaningless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotInstalledError;

impl fmt::Display for NotInstalledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Developer-facing diagnostic (test output only), never shown in the program's interface,
        // so it is not a Fluent message (CLAUDE.md §7 concerns user-visible text).
        f.write_str(
            "CountingAllocator is not the global allocator of this binary: \
             add `orchestra_test_support::install_counting_allocator!();` to the test file",
        )
    }
}

impl Error for NotInstalledError {}

/// Runs `f` on the current thread and returns its result together with the heap activity it
/// caused **on this thread**.
///
/// Allocations made by other threads (including threads that `f` spawns) are not counted: see
/// the crate documentation for why the counters are per thread.
///
/// # Errors
/// [`NotInstalledError`] if [`CountingAllocator`] is not the global allocator; `f` is not run
/// in that case.
///
/// # Cost
/// The [`is_installed`] probe (one allocation, before the measurement starts) plus two counter
/// snapshots around `f`. The probe is outside the measured window, so it never shows up in the
/// returned statistics.
pub fn count_allocations<R>(
    f: impl FnOnce() -> R,
) -> Result<(R, AllocationStats), NotInstalledError> {
    if !is_installed() {
        return Err(NotInstalledError);
    }
    let before = thread_stats();
    let result = f();
    let stats = thread_stats().since(&before);
    Ok((result, stats))
}

/// Runs `f` and fails the test if it caused any heap activity (allocation, reallocation or
/// deallocation) on the current thread; returns `f`'s result otherwise.
///
/// This is the helper behind "this operation never allocates" tests (ADR-0007 rule 1,
/// ADR-0015 §9). Note that values `f` returns are dropped by the caller, outside the measured
/// window: return plain values, or drop heap values inside `f` if their deallocation matters.
///
/// # Panics
/// - if [`CountingAllocator`] is not installed (the test would otherwise pass vacuously);
/// - if `f` touched the heap, with the counted statistics in the message.
///
/// Panicking is the intended behaviour: this is an assertion for test code, like `assert!`.
#[allow(
    clippy::panic,
    reason = "a test assertion helper: failing the calling test is its whole purpose"
)]
pub fn assert_no_allocations<R>(f: impl FnOnce() -> R) -> R {
    let (result, stats) = match count_allocations(f) {
        Ok(measured) => measured,
        Err(e) => panic!("{e}"),
    };
    // `assert!` states the condition being checked; its message is only built on failure.
    assert!(
        stats.is_zero(),
        "expected no heap activity on this thread, got {stats:?}"
    );
    result
}
