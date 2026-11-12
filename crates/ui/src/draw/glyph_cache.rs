// SPDX-License-Identifier: GPL-3.0-or-later

//! The glyph cache of the CPU renderer (ADR-0044 TX8, TC5): rasterised glyphs kept between
//! frames as 8-bit coverage masks, bounded in memory.
//!
//! # Why a cache
//! The ADR-0012 spike measured 243 ms for 2000 glyphs shaped and filled at every frame, 28.8 ms
//! with the outlines kept as paths: filling the paths was still most of the cost. A mask is
//! filled once and then only blended, a few instructions a pixel.
//!
//! # What is cached
//! A glyph's coverage depends on its face, its glyph ID, its size in device pixels and its
//! position relative to the pixel grid. Sizes are quantised to 1/64 of a pixel (the 26.6 fixed
//! point FreeType uses) and horizontal positions to a quarter of a pixel (TX7), vertical ones to
//! whole pixels, so one glyph has at most four masks per size. The colour is not part of the
//! key: a mask is coverage only, blended with the paint at draw time, so a theme change redraws
//! from the same masks (D3).
//!
//! # Bound and eviction
//! At most [`GLYPH_CACHE_BYTES`] of masks. When a new mask would exceed it, the least recently
//! used masks are evicted, in one batch, down to three quarters of the bound: eviction sorts the
//! entries, O(n log n), so doing it in batches keeps its cost rare instead of paying a scan for
//! every new glyph while zooming.
//!
//! This module knows nothing of `tiny-skia`: the renderer passes the function that rasterises
//! (`skia.rs` stays the only file that knows `tiny-skia`).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use orchestra_text::{FaceId, GlyphId};

/// The memory bound of the cache, in bytes of masks (ADR-0044 TX8: 8 MB).
pub(crate) const GLYPH_CACHE_BYTES: usize = 8 * 1024 * 1024;

/// The largest mask side that is cached, device pixels (TX8). A glyph larger than this (a
/// strong zoom on the score) is filled as a path at every frame instead: such a glyph is rare,
/// and one mask of it would take 64 KB or more of the bound.
pub(crate) const MAX_CACHED_SIDE: u32 = 256;

/// What identifies a rasterised glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    /// The face.
    pub(crate) face: FaceId,
    /// The glyph.
    pub(crate) glyph: GlyphId,
    /// The size in device pixels per em, times 64, rounded.
    pub(crate) ppem_64: u32,
    /// The horizontal position's quarter of a pixel, 0–3.
    pub(crate) quarter: u8,
}

/// The result of rasterising a glyph, as the cache keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Raster {
    /// No ink (a space, an invisible glyph, a glyph the font does not have).
    Empty,
    /// Larger than [`MAX_CACHED_SIDE`]: drawn as a path. Remembered so that later frames do not
    /// measure the outline again before filling it.
    Large,
    /// A coverage mask.
    Mask(GlyphMask),
}

/// A glyph's coverage mask, placed relative to its rounded origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlyphMask {
    /// Device pixels from the rounded origin's column (the whole part of the quantised x) to
    /// the mask's left column.
    pub(crate) left: i32,
    /// Device pixels from the rounded baseline row to the mask's top row (negative: above).
    pub(crate) top: i32,
    /// The mask's width, device pixels (1–[`MAX_CACHED_SIDE`]).
    pub(crate) width: u32,
    /// The mask's height, device pixels.
    pub(crate) height: u32,
    /// Coverage 0–255, row by row, `width × height` bytes.
    pub(crate) coverage: Vec<u8>,
}

impl Raster {
    /// The bytes the entry counts against the bound (a small fixed cost for empty and large
    /// entries, so that a flood of them is bounded too).
    fn bytes(&self) -> usize {
        /// What an entry costs besides its mask: the key, the bookkeeping, the hash slot.
        const OVERHEAD: usize = 64;
        match self {
            Self::Empty | Self::Large => OVERHEAD,
            Self::Mask(mask) => OVERHEAD + mask.coverage.len(),
        }
    }
}

/// One entry of the cache.
#[derive(Debug)]
struct Entry {
    /// What it is.
    key: GlyphKey,
    /// The rasterised glyph.
    raster: Raster,
    /// The frame it was last used in.
    last_used: u64,
}

/// A fast hasher for [`GlyphKey`]: the keys are small integers we produce ourselves, so the
/// protection of the standard SipHash against crafted keys buys nothing, and its cost would be
/// paid for every glyph of every frame. This is the multiply-rotate scheme of `rustc`'s FxHash.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct KeyHasher(u64);

impl KeyHasher {
    /// FxHash's multiplier (a large odd constant with well-mixed bits).
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    /// Mixes one word into the state.
    const fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Used only for types we do not write by hand (none today): one byte at a time is
        // slow but correct.
        for &byte in bytes {
            self.add(u64::from(byte));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_isize(&mut self, value: isize) {
        // `derive(Hash)` on a field-less enum writes its discriminant as an `isize`.
        self.add(value.cast_unsigned() as u64);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
}

/// The cache: entries in a vector, found through a hash map from key to index.
#[derive(Debug, Default)]
pub(crate) struct GlyphCache {
    /// The entries, in no particular order.
    entries: Vec<Entry>,
    /// From a key to its entry's index.
    index: HashMap<GlyphKey, usize, BuildHasherDefault<KeyHasher>>,
    /// The bytes of all entries, against [`GLYPH_CACHE_BYTES`].
    bytes: usize,
}

impl GlyphCache {
    /// The rasterised glyph of `key`, rasterising it with `rasterise` if it is not cached (and
    /// evicting old entries if the bound would be exceeded). `frame` is the current frame's
    /// number, for the eviction.
    ///
    /// # Complexity
    /// A hit is one hash lookup, O(1), with no allocation. A miss costs the rasterisation, an
    /// insertion, and now and then a batch eviction, O(n log n) in the entries.
    pub(crate) fn get_or_rasterise(
        &mut self,
        key: GlyphKey,
        frame: u64,
        rasterise: impl FnOnce() -> Raster,
    ) -> Option<&Raster> {
        if let Some(&index) = self.index.get(&key) {
            let entry = self.entries.get_mut(index)?;
            entry.last_used = frame;
            return Some(&entry.raster);
        }
        let raster = rasterise();
        let bytes = raster.bytes();
        if self.bytes + bytes > GLYPH_CACHE_BYTES {
            self.evict(GLYPH_CACHE_BYTES / 4 * 3 - bytes.min(GLYPH_CACHE_BYTES / 4 * 3));
        }
        let index = self.entries.len();
        self.entries.push(Entry {
            key,
            raster,
            last_used: frame,
        });
        self.index.insert(key, index);
        self.bytes += bytes;
        self.entries.get(index).map(|entry| &entry.raster)
    }

    /// Evicts the least recently used entries until the cache holds at most `target` bytes.
    fn evict(&mut self, target: usize) {
        // Sort by last use, oldest first, and drop from the front.
        self.entries.sort_unstable_by_key(|entry| entry.last_used);
        let mut drop = 0;
        for entry in &self.entries {
            if self.bytes <= target {
                break;
            }
            self.bytes -= entry.raster.bytes();
            drop += 1;
        }
        self.entries.drain(..drop);
        // Every index moved: rebuild the map (O(n), no allocation beyond its capacity).
        self.index.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.index.insert(entry.key, index);
        }
    }

    /// The bytes the cache holds now (masks and bookkeeping), always ≤ [`GLYPH_CACHE_BYTES`]
    /// after an insertion.
    pub(crate) const fn bytes(&self) -> usize {
        self.bytes
    }

    /// The number of entries.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key for glyph `n` at 13 px.
    fn key(n: u32) -> GlyphKey {
        GlyphKey {
            face: FaceId::InterRegular,
            glyph: GlyphId(n),
            ppem_64: 13 * 64,
            quarter: 0,
        }
    }

    /// A mask of `side × side` pixels.
    fn mask(side: u32) -> Raster {
        Raster::Mask(GlyphMask {
            left: 0,
            top: 0,
            width: side,
            height: side,
            coverage: vec![255; (side * side) as usize],
        })
    }

    #[test]
    fn a_hit_does_not_rasterise_again() {
        let mut cache = GlyphCache::default();
        let mut calls = 0;
        for frame in 0..3 {
            cache.get_or_rasterise(key(1), frame, || {
                calls += 1;
                mask(10)
            });
        }
        assert_eq!(calls, 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn the_bound_is_kept_and_the_oldest_go_first() {
        let mut cache = GlyphCache::default();
        // 64 × 64 masks: 4 KiB + overhead each, so the 8 MiB bound is reached near 2000.
        let side = 64;
        for n in 0..3000 {
            cache.get_or_rasterise(key(n), u64::from(n), || mask(side));
            assert!(cache.bytes() <= GLYPH_CACHE_BYTES);
        }
        // The most recent glyph survived; the first ones did not.
        let mut rasterised = false;
        cache.get_or_rasterise(key(2999), 5000, || {
            rasterised = true;
            mask(side)
        });
        assert!(!rasterised, "the newest entry was evicted");
        cache.get_or_rasterise(key(0), 5001, || {
            rasterised = true;
            mask(side)
        });
        assert!(rasterised, "the oldest entry was kept");
    }

    #[test]
    fn keys_differ_by_every_field() {
        let base = key(7);
        let others = [
            GlyphKey {
                face: FaceId::InterBold,
                ..base
            },
            GlyphKey {
                glyph: GlyphId(8),
                ..base
            },
            GlyphKey {
                ppem_64: base.ppem_64 + 1,
                ..base
            },
            GlyphKey { quarter: 1, ..base },
        ];
        let mut cache = GlyphCache::default();
        cache.get_or_rasterise(base, 0, || mask(4));
        for other in others {
            let mut rasterised = false;
            cache.get_or_rasterise(other, 0, || {
                rasterised = true;
                mask(4)
            });
            assert!(rasterised, "{other:?} was taken for {base:?}");
        }
    }
}
