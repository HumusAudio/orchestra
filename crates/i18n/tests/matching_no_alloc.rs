// SPDX-License-Identifier: GPL-3.0-or-later

//! Bringing a system's language tag down to one of ours allocates nothing (ADR-0053 LR3): it runs
//! at start-up and whenever *System language* is chosen, and its subtags are slices of the tag it
//! is given, so it must stay cheap enough to be called without a thought.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

use std::hint::black_box;

use orchestra_i18n::Locale;
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// A POSIX name with codeset and modifier, a BCP 47 tag with a script, a language that is not
/// ours, and a whole negotiation over them.
#[test]
fn matching_and_negotiating_do_not_allocate() {
    let tags = ["de_DE.UTF-8@euro", "en-Latn-GB", "pt-BR"];
    assert_no_allocations(|| {
        for tag in tags {
            let _ = black_box(Locale::matching(black_box(tag)));
        }
        let _ = black_box(Locale::negotiate(black_box(tags)));
    });
}
