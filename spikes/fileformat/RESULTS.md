# Spike F1.15 — project file encodings, measured

Run: `cargo run --release -p fileformat` in `spikes/` (2026-11-06, development machine, native
times, best of five; **not** the reference laptop, which is expected to be about 2–3× slower).

The scores are the real normal and heavy scores of `core-model`'s tests
(`crates/core-model/tests/common/score.rs`), read through the model's public API into a file schema
of plain structs with serde derives. Only the voice contents are encoded (well over 99% of the
data). "Read" = bytes → typed schema; "via tree" = bytes → `serde_json::Value` → typed schema (the
path of a file whose migrations run on a generic tree). Converting the schema into a `Document` is
not measured; the real builder needs 3.0 ms (normal) and 85.7 ms (heavy) to build the same
documents, a fair proxy.

- **verbose**: `{"id":145,"pitch":{"step":"C","alter":"0","octave":4}}`, `{"base":"Quarter","dots":0}`
- **compact**: `[145,"C4"]`, `"q"`, one-letter keys

## Normal score — 1 400 cells, 7 044 events, 7 444 noteheads

| encoding            |    size | deflate (level 6) | write ms | read ms | via tree ms |
|---|---|---|---|---|---|
| JSON verbose        | 1.09 MB | 0.07 MB |  1.9 |  5.9 | 21.2 |
| JSON verbose pretty | 3.60 MB | 0.09 MB |  6.3 |  7.7 | — |
| JSON compact        | 0.43 MB | 0.06 MB |  0.9 |  3.4 |  8.3 |
| RON verbose         | 0.93 MB | 0.07 MB | 10.2 | 29.0 | not possible |
| RON verbose pretty  | 5.20 MB | 0.11 MB | 16.0 | 38.6 | — |
| postcard (binary)   | 0.18 MB | 0.05 MB |  0.5 |  2.3 | — |

## Heavy score — 41 000 cells, 206 530 events, 221 169 noteheads

| encoding            |      size | deflate (level 6) | write ms | read ms | via tree ms |
|---|---|---|---|---|---|
| JSON verbose        |  35.02 MB | 2.04 MB |  78.1 |  212.9 | 723.4 |
| JSON verbose pretty | 112.64 MB | 2.71 MB | 192.6 |  244.9 | — |
| JSON compact        |  13.68 MB | 1.85 MB |  32.2 |  100.2 | 314.4 |
| RON verbose         |  28.29 MB | 1.96 MB | 350.8 | 1038.3 | not possible |
| RON verbose pretty  | 154.41 MB | 3.30 MB | 564.3 | 1373.8 | — |
| postcard (binary)   |   5.95 MB | 1.52 MB |  22.6 |   64.3 | — |

## Compression (deflate, as a zip entry would store it)

| data | level 1: size / pack / unpack | level 6: size / pack / unpack |
|---|---|---|
| normal, JSON verbose | 0.12 MB / 2.0 ms / 1.4 ms | 0.07 MB / 20.4 ms / 0.8 ms |
| normal, JSON compact | 0.09 MB / 1.5 ms / 1.0 ms | 0.06 MB / 20.9 ms / 0.7 ms |
| heavy, JSON verbose  | 3.57 MB / 67.6 ms / 46.2 ms | 2.04 MB / 688 ms / 28.1 ms |
| heavy, JSON compact  | 2.72 MB / 49.1 ms / 33.6 ms | 1.85 MB / 649 ms / 22.4 ms |

## Findings

1. **Every candidate meets ADR-0007's "100 bars ≤ 1 s"** by two orders of magnitude (normal score:
   read ≤ 8 ms with JSON). Even the heavy score (1 000 bars × 30 staves) reads in 0.1–0.25 s with
   JSON on this machine.
2. **RON is 3–5× slower than JSON** both ways, and its generic tree (`ron::Value`) **cannot read
   our enums back** (`InvalidValueForType { expected: "enum VBody", found: "a map" }`), so
   migrations on RON's tree are not possible without a second, typed schema per version.
3. **postcard (binary)** is the fastest and smallest, but not diffable, not inspectable, and **not
   self-describing**: a field written conditionally (`skip_serializing_if`) cannot be read back
   (`DeserializeBadUtf8`), and a migration needs the old typed schema.
4. **Pretty-printed JSON is 3× larger** than compact JSON with the same keys and only ~15–30%
   slower to read; after compression the difference almost vanishes (2.04 → 2.71 MB).
5. **Short values matter more than short keys**: the compact schema halves the reading time.
6. **Compression level 6 costs ~10× level 1 to write** (heavy: 0.65 s against 0.05 s) and saves
   ~30%; unpacking is cheap at both levels (≤ 50 ms).
7. **The generic-tree path is ~3× slower** than reading the typed schema directly: fine for the
   one-off opening of an old file, not for the current version.
