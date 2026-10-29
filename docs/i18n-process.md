# How to add or change interface text

Orchestra's interface exists in six languages: **en-US** (source), **en-GB**, **it**, **fr**,
**de**, **es** (ADR-0008). No user-visible text is ever written in the code: the code uses message
*keys*, and the text lives in Fluent catalogs (`crates/i18n/locales/<tag>/main.ftl`).

Follow these steps for every new or changed string.

## 1. Look the terms up in the glossary

Open `crates/i18n/glossary.toml` and find every musical or technical term your text uses
(measure, staff, tie, track, velocity…). Use exactly the term recorded for each language — in
particular the **en-GB** column (bar, stave, crotchet, quantise) and the qualified forms that
distinguish similar concepts (tie vs slur: *legatura di valore* / *di portamento*, *Haltebogen* /
*Bindebogen*, …).

Entries with `review = true` are provisional: use them, and mention in the pull request that a
native-speaking musician should confirm them.

## 2. If a term is missing: research it, then add it

Musical terms are **not** translated literally. Before adding an entry:

1. Check what musicians actually see in established software, in each language. The most useful
   sources so far (see `[sources]` in the glossary for exact URLs):
   - MuseScore Studio's UI catalogs (`share/locale/musescore_<lang>.ts` on GitHub), for notation;
   - Ardour's UI catalogs (`gtk2_ardour/po/<lang>.po`), for DAW concepts — its `en_GB.po` only
     overrides strings that differ from US English, which tells you whether British usage differs;
   - Apple's Logic Pro user guide, same page id in each locale (`support.apple.com/<locale>/guide/…`);
   - Wikipedia interlanguage links of the English article, and its text for US/UK usage notes;
   - localised manuals (Dorico, Cubase) and music dictionaries/academic glossaries.
2. Where sources disagree, choose deliberately, explain why in `note`, and set `review = true`.
3. Add the `[[term]]` entry with all six languages, `sources` and `note`.

## 3. Write the message in all six catalogs

- Choose a key describing *where* and *what* (`menu-edit-undo`, `status-measures-selected`), not
  the English wording: the text may change, the key stays.
- Add it to **every** catalog in the same commit. en-GB is a separate catalog: copy en-US and then
  apply the British terms.
- Numbers, names and other values are **variables** (`{ $count }`), never concatenated pieces.
  Use the same variables in every language.
- Plurals, gender and agreement use Fluent selectors, written for each language's own rules:

  ```ftl
  status-measures-selected =
      { $count ->
          [one] { $count } battuta selezionata
         *[other] { $count } battute selezionate
      }
  ```

  Remember that plural categories differ between languages (French `one` also covers 0).

## 4. Run the tests

```sh
cargo nextest run -p orchestra-i18n
```

They fail if a key is missing (or left over) in any language, if the variables of a message
differ from en-US, if a message cannot be formatted, or if obvious interface text is hard-coded
in the `ui`/`app` crates. `cargo xtask check` runs them too, locally and in CI.

## 5. In the code

```rust
let text = localizer.format("menu-edit-undo", None)?;
```

A developer-only string (log line, internal error) that trips the hard-coded-text check gets a
comment `// i18n-ignore: <reason>` on the same line.
