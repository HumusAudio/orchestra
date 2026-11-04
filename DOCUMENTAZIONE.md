# Orchestra — documentazione per sviluppatori

Questa è la documentazione per sviluppatori di Orchestra, in italiano. Il suo gemello inglese è
`DOCUMENTATION.md`: i due file dicono le stesse cose con **la stessa struttura** (stessi titoli
agli stessi livelli, stessi blocchi di codice, identici byte per byte); cambia solo la lingua del
testo. Un hook di Claude Code e `cargo xtask check` lo verificano entrambi, così i due documenti
non possono divergere.

Il *perché* di ogni decisione importante sta negli ADR (`docs/adr/`); la mappa dei crate sta in
`docs/ARCHITECTURE.md`; lo stato dei lavori in `ROADMAP.md`. Questo documento spiega *cosa c'è in
ogni file e come funziona*, in modo colloquiale e con frammenti di codice veri. Cresce insieme al
codice: chi modifica un file sorgente aggiorna la sezione corrispondente qui, in entrambe le
lingue, nello stesso passaggio.

## Dove si trovano le cose

Dopo la Fase 0 il repository è fatto così:

```text
orchestra/
├── Cargo.toml              workspace manifest: members, shared metadata, lints, dependencies
├── Cargo.lock              exact dependency versions (committed: we build an application)
├── rustfmt.toml            formatter settings (edition repeated for single-file rustfmt)
├── clippy.toml             clippy settings (test exemptions, domain words for doc lints)
├── deny.toml               cargo-deny policy: licenses, sources, duplicates, advisories
├── .cargo/config.toml      the `cargo xtask` alias
├── .gitattributes          LF line endings everywhere, binary assets untouched
├── .gitignore              build output, generated PDFs, editor files, secrets
├── .github/workflows/      CI: cargo xtask check on 3 OS, guides as artifacts
├── .githooks/commit-msg    enforces the `User-visible: yes|no` trailer
├── .claude/settings.json   Claude Code: sandbox network, rustfmt and documentation hooks
├── scripts/                helper scripts used by the hooks
├── crates/                 the program, one crate per responsibility
├── xtask/                  project automation (`cargo xtask …`)
├── assets/fonts/           bundled OFL fonts (text, code, SMuFL music font)
├── assets/logo/            the only Orchestra logo (outlined SVG, single colour)
├── docs/                   ADRs, architecture, i18n process, user guide sources
├── spikes/                 Phase 0 evaluation prototypes (separate workspace, never shipped)
├── CLAUDE.md               permanent rules for every development session
├── ROADMAP.md              status, next steps, open decisions
├── DOCUMENTAZIONE.md       this document, in Italian
└── DOCUMENTATION.md        this document, in English
```

(Le descrizioni nell'albero restano in inglese perché i blocchi di codice devono essere identici
nei due documenti.)

## Il workspace Cargo

### Il manifest principale

`Cargo.toml` dichiara il workspace. Alcune scelte meritano una spiegazione.

**Cartelle e nomi dei pacchetti.** Ogni crate vive in `crates/<responsabilità>/` (lo strumento di
automazione in `xtask/`, come vuole la convenzione cargo-xtask). I nomi dei pacchetti hanno il
prefisso `orchestra-`: `crates/io` è il pacchetto `orchestra-io`, che in Rust si importa come
`orchestra_io`. Nomi nudi come `io` o `ui` confonderebbero accanto a `std::io` e andrebbero in
conflitto con crates.io se un giorno pubblicassimo qualcosa.

**Metadati condivisi.** Versione, edition, versione minima di Rust, licenza e repository sono
scritti una volta sola in `[workspace.package]` e ogni crate li eredita:

```toml
[workspace.package]
version = "0.0.0"
edition = "2024"
rust-version = "1.99"
license = "GPL-3.0-or-later"
repository = "https://github.com/HumusAudio/orchestra"
publish = false
```

L'edition 2024 è la più recente tra quelle stabili (rustc 1.99.1 rifiuta `2027`, verificato il
28/10/2026). `rust-version` è la toolchain con cui sviluppiamo: la si alza di proposito, insieme a
quella della CI, invece di lasciarla scivolare. `publish = false` impedisce un `cargo publish` per
errore.

**Dipendenze interne.** Ogni crate è elencato in `[workspace.dependencies]` con il suo percorso, e
i membri lo richiamano con `orchestra-core-time.workspace = true`. Spostare un crate significa
quindi cambiare una riga sola. Nella stessa tabella stanno le dipendenze esterne, ognuna con un
commento che dice chi l'ha verificata e dove è giustificata.

**Resolver 3.** È già il predefinito per l'edition 2024, ma lo scriviamo esplicitamente: rende la
risoluzione delle dipendenze consapevole di `rust-version`, preferendo versioni che compilano con
la nostra toolchain minima.

### I lint: le regole che fa rispettare il compilatore

Le tabelle `[workspace.lints]` trasformano le regole di stile di `CLAUDE.md` in errori del
compilatore, così non dipendono dalla memoria di nessuno:

```toml
[workspace.lints.rust]
missing_docs = "deny"
unsafe_op_in_unsafe_fn = "deny"

[workspace.lints.clippy]
missing_docs_in_private_items = "deny"
undocumented_unsafe_blocks = "deny"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
```

- `missing_docs` (di rustc) copre l'API pubblica; `missing_docs_in_private_items` (di clippy) copre
  tutto il resto: ogni funzione, struct, campo e costante, anche privati, vuole un doc comment.
- `undocumented_unsafe_blocks` richiede un commento `// SAFETY:` su ogni blocco `unsafe`.
- `unwrap_used`, `expect_used` e `panic` vietano le scorciatoie che fanno andare in panico il
  codice di libreria: gli errori sono tipizzati e restituiti. `clippy.toml` toglie questi tre
  divieti dentro le funzioni `#[test]`, dove un'asserzione fallita *è* il modo giusto di
  segnalare un errore.
- `dbg_macro`, `todo` e `unimplemented` sono avvisi in locale, ma `cargo xtask check` esegue
  clippy con `-D warnings`, quindi nella CI diventano anch'essi errori.

Il gruppo **pedantic** è attivo, con tre lint disattivati (`module_name_repetitions`,
`must_use_candidate`, `similar_names`) e un commento in `Cargo.toml` che spiega ogni scelta; il
proprietario ha approvato questa selezione il 2026-10-30 (passo F0.16 della SCALETTA, decisione
D01). I lint sulle conversioni numeriche (cast) restano attivi di proposito: il codice audio e del
tempo converte di continuo tra numero di campioni, fotogrammi e numeri decimali, e ogni
conversione con perdita deve essere visibile.

Oltre a pedantic, alcuni lint sono scelti uno per uno dal gruppo `restriction` di clippy (mai
attivato per intero: i suoi lint si contraddicono a vicenda per costruzione) e da rustc:

```toml
string_slice = "deny"                     # slicing a str off a char boundary panics
get_unwrap = "deny"                       # v.get(i).unwrap() is an unwrap in disguise
unreachable = "deny"                      # unreachable!() is a panic with another name
allow_attributes_without_reason = "warn"  # every #[allow] says why
print_stdout = "warn"                     # libraries never print
print_stderr = "warn"
exit = "warn"                             # libraries never end the process
mem_forget = "warn"                       # nor leak on purpose
```

- I primi tre completano la regola "niente panic nel codice di libreria", quindi sono `deny` come
  lei.
- `allow_attributes_without_reason` vuol dire che un lint disattivato deve portare con sé la sua
  spiegazione: `#[allow(clippy::panic, reason = "...")]`. È la nostra regola "commenta ogni
  scelta", verificata dal compilatore.
- Le librerie non stampano: l'output spetta a chi le chiama. Il crate `app` disattiva i due lint
  di stampa nella sua radice (con il motivo), perché l'eseguibile è il posto giusto per scrivere
  sul terminale. Servirà anche per i bridge dei plugin, il cui stdout deve restare pulito
  (ADR-0011).
- Dal lato di rustc: `unreachable_pub` (un elemento `pub` che nessuno fuori dal crate può
  raggiungere sembra API pubblica ma non lo è: si usa `pub(crate)`), `unused_qualifications` e
  `missing_debug_implementations` (ogni tipo pubblico implementa `Debug`, per i messaggi di
  errore e i test falliti).

Due lint più severi valgono solo per i crate di cui si fidano tutti gli altri. Poiché un crate che
dichiara `[lints] workspace = true` non può aggiungere una tabella sua, sono attributi in cima
alla radice del crate:

```rust
// crates/core-time/src/lib.rs
#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
// crates/core-model/src/lib.rs
#![deny(clippy::indexing_slicing)]
```

`indexing_slicing` vieta `v[i]` (va in panic se `i` è fuori dai limiti: si usa `get` o si
itera); `arithmetic_side_effects` vieta `+ - *` semplici sugli interi in `core-time`, così ogni
operazione sul tipo razionale passa dall'aritmetica controllata e dalla politica di overflow di
ADR-0005, progettata nel passo F1.01. `clippy.toml` esenta i test da `indexing_slicing` e dai lint
di stampa.

Scartati, con i motivi in `Cargo.toml`: il gruppo `nursery` (lint ancora in sviluppo, che
romperebbero la CI a ogni aggiornamento del compilatore) e `as_conversions` (i lint pedantic sulle
conversioni segnalano già quelle pericolose).

`unsafe_code` non è nella tabella del workspace. La radice di ogni crate comincia con
`#![forbid(unsafe_code)]`, tranne `audio-engine`, `plugin-host` e `io`, i tre crate che parleranno
con API in C. Un `forbid` a livello di workspace non si potrebbe allentare solo per quei tre, e
l'attributo rende l'eccezione visibile proprio in cima al crate.

`xtask` non eredita questa tabella: è uno strumento per sviluppatori che stampa messaggi ed esce
con codici di errore, quindi dichiara la sua (documentazione obbligatoria, pedantic attivo,
`unreachable_pub` e `unused_qualifications` attivi, `unsafe` vietato).

### Formattatore, clippy e l'alias di cargo

`rustfmt.toml` ripete l'edition. Sembra ridondante, ma l'hook PostToolUse esegue `rustfmt` su
singoli file, fuori da cargo, e senza quella riga rustfmt presumerebbe l'edition 2015.

`clippy.toml` contiene le esenzioni per i test descritte sopra e l'elenco delle parole del nostro
dominio che il lint `doc_markdown` non deve chiedere di mettere tra apici inversi (SMuFL, MusicXML,
PipeWire, CoreAudio…).

`.cargo/config.toml` definisce l'alias che fa funzionare `cargo xtask` senza installare nulla:

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

### cargo-deny: licenze, fonti, duplicati, vulnerabilità

`deny.toml` (schema di cargo-deny 0.20) è la politica sulle dipendenze dell'ADR-0002:

- **licenze**: solo licenze compatibili con la distribuzione di Orchestra sotto
  GPL-3.0-or-later (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, LGPL, GPL-3.0, GPL-2.0-*or-later*,
  Unicode-3.0…). La GPL-2.0-*only* non è compatibile e quindi manca;
- **divieti**: due versioni dello stesso crate sono un errore (`multiple-versions = "deny"`),
  perché costano tempo di compilazione e dimensione del binario; le versioni "jolly" (`*`) sono
  vietate;
- **fonti**: solo crates.io;
- **avvisi di sicurezza**: vulnerabilità note e crate non più mantenuti, dal database RustSec.

Il grafo viene controllato per tutti e sei i target che distribuiremo (Linux, Windows e macOS, su
x86_64 e aarch64), così una dipendenza solo-Windows con una licenza sbagliata viene scoperta anche
su Linux. Il database RustSec sta in `target/advisory-dbs` invece che in `~/.cargo`, che nel
sandbox di Claude Code è in sola lettura.

### Fine riga e file ignorati

`.gitattributes` salva ogni file di testo con LF e lo estrae con LF su ogni sistema, Windows
compreso. Gli script di shell devono essere LF o bash li rifiuta, e l'impronta della parità dei
documenti deve essere identica ovunque. Font, immagini, PDF e file audio sono marcati come
binari.

`.gitignore` esclude `target/`, `dist/` (i PDF generati non si committano mai), le cartelle degli
editor e i file `.env`.

## I crate

Nella Fase 0 ogni crate conteneva solo la sua documentazione di crate (scopo, responsabilità,
cosa *non* deve fare, dipendenze ammesse), tranne `orchestra-i18n`, che funziona già. La Fase 1 li
riempie dal basso: per ora esistono `orchestra-test-support` e, in `orchestra-core-time`, il
tipo razionale, i tipi posizione e durata, la mappa delle misure e le unità del tempo reale. Le
regole sulle
dipendenze sono riassunte in `docs/ARCHITECTURE.md`; qui c'è a cosa serve ciascun crate.

### orchestra-core-time

Il tempo musicale esatto: posizioni e durate come frazioni di semibreve, l'unica mappa dei tempi,
le conversioni in secondi, campioni e fotogrammi video (ADR-0005). Le funzioni di conversione
verranno chiamate dal thread audio, quindi non devono allocare memoria. Nessuna dipendenza
all'interno del workspace (in `[dev-dependencies]` solo `orchestra-test-support` e `proptest`, per
i test). Per ora contiene il tipo razionale (passo F1.03), i tipi posizione e durata (F1.04) e la
mappa delle misure con le suddivisioni dei movimenti (F1.04b), le unità del tempo reale (F1.05:
tempo, secondi, campioni, fotogrammi) e la mappa dei tempi con i cambi graduali e gli indici esatti
di campione e di fotogramma (F1.05b).

#### Il tipo razionale

`Rational` (in `src/rational.rs`, riesportato alla radice del crate insieme a `RationalError`) è
il numero che ogni altro crate userà per dire "quando" e "quanto dura": una frazione di due `i64`,
misurata in semibrevi. Una semiminima è `1/4`, una croma di terzina `1/12`, la battuta 3 di un
brano in 4/4 comincia a `2/1`. È stato progettato in ADR-0015; ecco come funziona in pratica.

```rust
use orchestra_core_time::{Rational, RationalError};

let triplet_eighth = Rational::new(1, 12)?;
let three = triplet_eighth.checked_add(triplet_eighth)?.checked_add(triplet_eighth)?;
assert_eq!(three, Rational::new(1, 4)?); // exactly a quarter note
assert_eq!(Rational::new(6, -8)?.numerator(), -3); // always reduced: -3/4
assert_eq!(Rational::new(1, 0), Err(RationalError::DivisionByZero));
```

**Sempre ridotto ai minimi termini.** I due campi sono privati, e ogni costruttore e ogni
operazione lascia il valore con tre proprietà (gli *invarianti*): il denominatore è almeno 1 (il
segno sta nel numeratore), numeratore e denominatore non hanno fattori comuni (quindi zero è
sempre `0/1`), e il numeratore non è mai `i64::MIN`. Siccome ogni valore si può scrivere in un
solo modo, uguaglianza e hash sono semplicemente "stessi campi", e la terza regola fa sì che `-x`
e `abs` non possano mai andare in overflow: `Neg` è l'unico operatore che il tipo implementa.

**Perché niente `+`, `-`, `*`, `/`.** Un operatore deve restituire un valore, quindi in caso di
overflow potrebbe solo far crashare il programma, "girare" (wrap) o saturare, e una posizione
girata o saturata è una nota sbagliata nel posto sbagliato, senza che nessuno se ne accorga. Per
questo le operazioni che possono fallire sono metodi (`checked_add`, `checked_sub`,
`checked_mul`, `checked_div`, `checked_recip`) che restituiscono
`Result<Rational, RationalError>`, con due errori: `Overflow` e `DivisionByZero`. I testi degli
errori (`Display`) sono in inglese, per gli sviluppatori; l'interfaccia tradurrà ogni variante
con una chiave Fluent, come ogni testo visibile all'utente.

**Esatto anche con numeri enormi.** Ogni operazione allarga i valori a `i128`, calcola lì, riduce,
e solo alla fine controlla che il risultato rientri in `i64`. Quindi fallisce solo quando il
risultato *ridotto* non ci sta: `(i64::MAX/3) · (3/i64::MAX)` dà `1`, anche se il prodotto
semplice dei numeratori sarebbe enorme. È proprio ciò che un crate esistente come `num-rational`
non fa (fallisce appena un passaggio intermedio va in overflow). La somma, per esempio:

```rust
pub const fn checked_add(self, rhs: Self) -> Result<Self, RationalError> {
    // Fast path: equal denominators (very common: notes in the same tuplet, integer bars).
    if self.den == rhs.den {
        let num = or_overflow!(wide(self.num).checked_add(wide(rhs.num)));
        return Self::from_wide(num, wide(self.den));
    }
    // ... otherwise (a·(d/g) + c·(b/g)) / ((b/g)·d) in i128, then reduce and narrow.
}
```

I prodotti di due `i64` stanno sotto 2¹²⁶ e la loro somma sotto 2¹²⁷, quindi in `i128` nulla può
andare in overflow; il codice usa comunque `checked_*`, così un errore in questo ragionamento
diventerebbe un errore e non un valore sbagliato. Lo stesso limite rende il confronto (`Ord`)
esatto e infallibile: `a/b < c/d` equivale a `a·d < c·b` calcolato in `i128`, con una scorciatoia
quando i denominatori sono uguali.

**Massimo comun divisore.** La riduzione usa l'algoritmo binario (di Stein), che usa solo
scorrimenti di bit e sottrazioni invece della lenta divisione intera dell'algoritmo di Euclide, e
una versione a 64 bit quando entrambi i numeri ci stanno (quasi sempre: i denominatori musicali
sono piccoli). La moltiplicazione semplifica *in croce* prima di moltiplicare (`a` con `d`, `c`
con `b`), quindi non ha bisogno di una riduzione finale. Costo: O(log n) per l'aritmetica, O(1)
per i confronti e **nessuna allocazione** in alcun punto. I benchmark a conteggio di istruzioni
sono il passo F1.03b.

**`const fn`.** Costruttori e aritmetica possono girare durante la compilazione, quindi un valore
fisso viene controllato quando si compila il programma:

```rust
const QUARTER: Rational = match Rational::new(1, 4) {
    Ok(r) => r,
    Err(_) => panic!("1/4 is a valid rational"),
};
```

Essere `const` costa un po' di prolissità: dentro una `const fn` non si possono ancora usare `?` e
`TryFrom`, quindi il file ha due piccole macro (`const_try!`, `or_overflow!`) che fanno il lavoro
di `?`, e alcune conversioni `as`, ognuna con il suo motivo. Gli altri metodi sono `abs`,
`signum`, `is_zero`, `is_positive`, `is_negative`, `is_integer`, `floor`/`ceil`/`trunc` (verso
`i64`, non possono fallire) e `to_f64`, un'approssimazione solo per la visualizzazione che non
deve mai rientrare nel modello. Di proposito non ci sono `Display` né lettura da testo: la forma
testuale si sceglie insieme al formato del file (F1.15). `Debug` stampa `Rational(3/8)`.

Poiché `core-time` vieta `+ - *` semplici sugli interi (`clippy::arithmetic_side_effects`), ogni
operazione intera nel file è `checked_*` oppure `wrapping_*` con un commento che dimostra che non
può "girare". Una curiosità: clippy rifiuta anche `wrapping_div` (potrebbe andare in panico con un
divisore zero), così `trunc` è scritto come `floor` o `ceil` a seconda del segno.

#### Come è testato

Tre file di test in `crates/core-time/tests/`, ciascuno un eseguibile a sé:

- `rational_oracle.rs`: **test di proprietà** con proptest confrontati con un **oracolo**, un
  calcolo esatto indipendente scritto nel test stesso in `i128`. L'oracolo usa le formule dei
  libri di scuola (`a/b + c/d = (a·d + c·b)/(b·d)`) e il MCD di Euclide, mentre la libreria usa
  altri algoritmi (divisione preventiva per il MCD, semplificazione in croce, Stein): se uno dei
  due sbaglia, non sono d'accordo.

  ```rust
  /// Oracle sum: `(a·d + c·b) / (b·d)`.
  fn oracle_add(x: Rational, y: Rational) -> Result<(i64, i64), RationalError> {
      let ((a, b), (c, d)) = (wide(x), wide(y));
      expected(a * d + c * b, b * d)
  }
  ```

  Degli `i64` casuali uniformi non cadono quasi mai sui casi interessanti, quindi gli ingressi
  vengono da un insieme di generatori: valori vicini a `i64::MAX` e `i64::MIN`, potenze di due,
  prodotti di primi piccoli (come le terzine di terzine) e coppie *collegate* (uguali, opposte,
  reciproche, con lo stesso grande denominatore) in cui i valori intermedi sono enormi ma il
  risultato ci sta. Ogni proprietà prova 2048 casi. Per essere sicuri che i test colgano davvero
  gli errori, durante lo sviluppo sono stati introdotti di proposito due bug (un `<` al posto di
  `<=` nel controllo dell'intervallo, una regola del segno sbagliata): entrambi sono stati trovati
  subito, con un controesempio minimo.
- `rational_cases.rs`: i casi limite scritti a mano, verificabili su carta: `i64::MIN` come
  ingresso, risultati che ci stanno solo dopo la riduzione, gli overflow che devono essere
  segnalati (`-i64::MAX − 1` sarebbe esattamente `i64::MIN`), gli arrotondamenti dai due lati
  dello zero, la forma `Debug`, una semiminima puntata `const`.
- `rational_no_alloc.rs`: installa l'allocatore contatore di `orchestra-test-support` e verifica
  che nessuna operazione tocchi lo heap, sia quando riesce sia quando restituisce un errore.

Dentro `src/rational.rs` un piccolo modulo `#[cfg(test)]` controlla le funzioni private (Stein
contro Euclide, il controllo dell'intervallo).

I benchmark (`benches/rational.rs`, passo F1.03b) e i loro primi numeri sono descritti più sotto,
in "Benchmark e test di proprietà".

#### Posizioni e durate

`src/position.rs` aggiunge i due **tipi di dominio** di ADR-0018 §1, riesportati alla radice del
crate: `Position` (un punto della linea del tempo scritta, in semibrevi dall'inizio della battuta
0, che è la battuta in levare quando c'è) e `Duration` (la distanza con segno fra due posizioni).
Entrambi sono un `Rational` travestito: stessi 16 byte (`#[repr(transparent)]`), stesso costo.
Quello che aggiungono è **quali operazioni hanno senso**:

```rust
use orchestra_core_time::{Duration, Position, Rational};

let quarter = Duration::from_rational(Rational::new(1, 4)?);
let beat_two = Position::ZERO.checked_add(quarter)?; // position + duration = position
let gap = beat_two.checked_duration_since(Position::ZERO)?; // position − position = duration
let dotted = quarter.checked_mul(Rational::new(3, 2)?)?; // duration × number = duration
let per_bar = Duration::from_rational(Rational::new(6, 8)?).checked_ratio(dotted)?; // 2
```

Sommare due posizioni o moltiplicare una posizione non compila: la documentazione del modulo ha
due doctest `compile_fail` che lo dimostrano. Con `Rational` ovunque, "inizio battuta + posizione
della nota" per errore compilerebbe e finirebbe una battuta più in là; qui lo ferma il
compilatore.

Alcuni dettagli:

- **Segni.** Entrambi i tipi accettano valori negativi. Una posizione prima di zero è un punto
  prima della partitura (la mappa delle misure la rifiuterà, ma una prima nota anticipata da uno
  scostamento di microtiming suona lì); una durata è negativa quando si misura all'indietro.
  "Una nota dura più di zero" è una regola del modello, non del tipo.
- **Il nome `checked_duration_since`** segue `std::time::Instant::duration_since`: il ricevente
  è la posizione successiva, l'argomento quella precedente.
- **`Duration::checked_ratio`** risponde a "quante volte questa sta in quella": le unità si
  semplificano, quindi restituisce un semplice `Rational`. Il suo `floor` dirà alla mappa delle
  misure quale battuta di una serie di battute uguali contiene una posizione, quindi su
  `Rational` non serve un'operazione di resto.
- **Vie d'ingresso e d'uscita:** `from_rational` e `as_rational` sono esplicite apposta. Le
  useranno la mappa dei tempi e il formato del file, e vederle in un diff vuol dire "qui un
  valore esce (o entra) dalla protezione dei tipi".
- Ogni metodo è una `const fn` che inoltra all'operazione di `Rational`, con gli stessi errori.
  La negazione chiama `negated` di `Rational`, privata al crate, invece dell'operatore `-`,
  perché `arithmetic_side_effects` di clippy non sa distinguere `-razionale` (infallibile) da una
  negazione intera che potrebbe traboccare.
- `Debug` stampa `Position(3/8)` e `Duration(-1/4)`; niente `Display` fino a F1.15.

I test sono in due file. `tests/position.rs` controlla con proptest che ogni metodo restituisca
esattamente ciò che restituisce la sua operazione di `Rational` (errori compresi), poi l'algebra
per cui i tipi esistono (`(p + d) − p = d`, `a − b = −(b − a)`, rapporto × unità = durata), poi
casi musicali scritti a mano. Un bug inserito apposta (`checked_sub` che somma invece di
sottrarre) è stato preso da due proprietà insieme. `tests/position_no_alloc.rs` controlla con
l'allocatore contatore che nulla allochi, sui percorsi di successo e di errore.

#### Metri e suddivisioni dei movimenti

`src/meter.rs` contiene due tipi piccoli che rispondono a due domande diverse su una battuta
(ADR-0018 §2, ADR-0019):

- **`Meter`**: come la battuta è *scritta*, l'indicazione di tempo stampata nella partitura. Da 1
  a 8 gruppi al numeratore e un denominatore: 4/4 è `[4]/4`, l'additivo 3+2+2/8 è `[3, 2, 2]/8`.
  Si accetta qualunque denominatore da 1 a 65535, non solo le potenze di due (decisione D04): 4/3
  dura quattro minime di terzina.
- **`BeatGrouping`**: come la battuta è *contata*, se la regola predefinita non è quella che
  l'utente vuole. Da 1 a 16 movimenti, ciascuno di `gruppo/unità` semibrevi, con un'unità propria:

```rust
use orchestra_core_time::{BeatGrouping, Meter, MeterChange, MeterMap};

let nine_eight = Meter::new(&[9], 8)?; // written "9/8"
let mixed = Meter::new(&[3, 2, 2], 8)?; // written "3+2+2/8"
// 9/8 still written "9/8", but counted 3+2+2+2 for the whole section:
let counted = BeatGrouping::new(&[3, 2, 2, 2], 8)?;
let map = MeterMap::new(MeterChange::with_grouping(nine_eight, counted))?;
// A 3/4 bar counted as two dotted quarters (hemiola): the grouping has its own unit.
let hemiola = BeatGrouping::new(&[3, 3], 8)?;
```

Entrambi tengono i gruppi in un array fisso (8 o 16 posti) più una lunghezza, quindi sono `Copy` e
non allocano mai. I posti non usati sono sempre zero: è questo che rende corretti l'uguaglianza e
l'hash derivati. Gli ingressi non validi (nessun gruppo, troppi, un gruppo zero, un denominatore
zero) sono un `MeterError`. `Debug` scrive `Meter(3+2+2/8)` e `BeatGrouping(3+2+2+2/8)`.

Il **conteggio predefinito**, usato quando non c'è una suddivisione, sta nella mappa delle misure:
un metro additivo ha un movimento per gruppo scritto; un numeratore unico multiplo di 3 maggiore di
3 è composto (6/8 in due semiminime puntate, 12/8 in quattro); tutto il resto è semplice, un
movimento per unità. Quindi 7/8 conta sette crome: il proprietario ha scelto di non indovinare
2+2+3 o 3+2+2, e l'interfaccia chiederà quale suddivisione usare quando si inserisce un metro misto
(ADR-0019, Conseguenze).

#### La mappa delle misure

`src/meter_map.rs` trasforma battute e movimenti in posizioni e viceversa. Conserva i **dati
sorgente** che il documento memorizza e i comandi modificano, in liste ordinate per battuta:

- i cambi di metro (`MeterChange`: un metro e una suddivisione facoltativa per la sua sezione), il
  primo sempre alla battuta 0;
- le battute irregolari (`IrregularBar`: una lunghezza esplicita e un `BarKind`): `AlignStart`
  (l'inizio di una battuta nominale, per esempio la battuta incompleta prima di un ritornello, o
  una battuta più lunga del suo metro), `AlignEnd` (la fine di una battuta nominale: il levare),
  `Free` (metro sospeso: una cadenza libera);
- le suddivisioni per singola battuta, che vincono su quella della sezione;
- le regole di numerazione: battute non contate e ripartenze.

Da questi ricava i **segmenti**, serie di battute con lo stesso metro, la stessa lunghezza e lo
stesso conteggio, ciascuno con la sua prima battuta e la sua posizione iniziale già sommata:

```text
bars:      0      1    2    3    4    5   …      (bar 0 a 1/4 pickup, 3/4 from bar 3)
segments: [pickup][ 4/4 × 2 ][ 3/4 forever  …
starts:    0      1/4        9/4
```

Così ogni ricerca è una ricerca binaria (`partition_point`) seguita da una o due divisioni
esatte: `locate` (posizione → battuta e scostamento), `bar_start`, `position_of`, `beat_of` (punto
→ movimento, oppure `None` in una battuta libera), `beat_position` (l'inversa), `beat_range` (i
movimenti che iniziano dentro una battuta, per il metronomo), `display_number`. Nessuna alloca
(`tests/meter_map_no_alloc.rs`). L'ultimo segmento continua per sempre: dove finisce la partitura
è affare del modello.

Alcune regole da sapere:

- **I movimenti si numerano da 0** e si contano dall'inizio virtuale della battuta nominale. Un
  levare di semiminima in 4/4 è il movimento 3 (il "quarto movimento" del musicista), e
  `beat_range` dà `3..4`; un levare di croma dà l'intervallo vuoto `4..4`, perché l'unico
  movimento che tocca è iniziato prima.
- **Una battuta più lunga del suo metro** ripete lo schema e continua a contare (opzione "a" del
  proprietario): un 5/4 in 4/4 ha i movimenti `0..5`; un 3+2+2/8 allungato a 9/8 ha un quarto
  movimento di 3/8 troncato a 2/8 dalla stanghetta.
- **Le battute libere** non hanno movimenti, ma il metro resta registrato e riprende alla prima
  battuta regolare; un cambio di metro messo su una battuta libera è "il metro dopo la cadenza".
  Nella numerazione contano, salvo che siano marcate come non contate.
- **Numerazione:** la battuta 0 ha il numero 0 se è un levare, altrimenti 1; le battute non
  contate non hanno numero e non fanno avanzare il conteggio; una ripartenza fissa il numero della
  sua battuta.

**Le modifiche sono tutto o niente.** Ogni modifica (`set_meter_change`, `set_irregular_bar`,
`set_bar_grouping` e i loro `remove_*`) lavora su una copia dei dati sorgente, ricostruisce i
segmenti e convalida tutto; solo allora sostituisce la mappa:

```rust
fn edit<T>(&mut self, change: impl FnOnce(&mut Sources) -> T) -> Result<T, MeterMapError> {
    let mut sources = self.sources.clone();
    let result = change(&mut sources);
    let segments = build_segments(&sources)?;
    self.sources = sources;
    self.segments = segments;
    Ok(result)
}
```

Quindi una modifica che lascerebbe incoerente qualche battuta (un levare ora più lungo di un nuovo
metro, una suddivisione per battuta che non torna più) restituisce un `MeterMapError` tipizzato e
la mappa resta esattamente com'era, come richiedono i comandi di ADR-0003. Ogni modifica
restituisce il valore precedente, che è ciò che un annulla memorizzerà. La ricostruzione costa
O(n log n) nel numero di voci sorgente e alloca, il che va bene sul percorso di modifica.

**Test.** `tests/meter_map_cases.rs` ha 20 casi scritti a mano, uno per voce dell'elenco della
SCALETTA (levare, cambi di metro, composti, additivi, 4/3, cadenza libera su una e più battute,
emiolia, suddivisioni che non tornano, numerazione, errori). `tests/meter_map_oracle.rs` costruisce
mappe da sequenze casuali di modifiche e confronta ogni battuta e punti casuali con un **oracolo
ingenuo** che cammina battuta per battuta dai dati sorgente, con scansioni lineari ed elenchi
espliciti dei movimenti, e controlla che le modifiche fallite lascino la mappa invariata. Due bug
inseriti apposta (composto da 3 invece che da 6; sommare una sola battuta per segmento invece di
tutte) sono stati presi da entrambi i file.

**Benchmark** (`benches/meter_map.rs`): usano una mappa di riferimento di 256 battute con circa 150
segmenti, molti più di una partitura reale, e lotti di 64 ricerche; la mappa è costruita nelle
funzioni di preparazione di Gungraun, che non vengono misurate. Primi numeri (macchina di sviluppo,
2026-10-31):

```text
locate            61609 instructions / 64 lookups ≈  960 each
bar_start         29850                            ≈  470
beat_of           59567                            ≈  930
beat_position     72693                            ≈ 1140
display_number     6557                            ≈  100
edit_meter_change 97488 for one edit (clone, rebuild, validation)
```

Gran parte del costo è l'aritmetica razionale esatta (i massimi comun divisori), non la ricerca.
Una ricerca da circa mille istruzioni sta ben sotto il microsecondo sull'hardware di riferimento.

#### Inserire e cancellare battute

I comandi di F1.11b inseriscono e cancellano battute (ADR-0022 §8, ADR-0026 §1), quindi la mappa
delle misure ha due operazioni che sono l'una l'**inversa esatta** dell'altra.
`insert_bars(at, &span)` inserisce le battute di un `BarSpan` prima della battuta `at` e sposta in
avanti di altrettante battute ogni voce dei dati di origine da `at` in poi; `remove_bars(at, count)`
toglie `count` battute, restituisce ciò che portavano (cambi di metro, battute in levare,
suddivisioni, battute non contate, ripartenze della numerazione, a indici relativi allo span) e
riporta indietro le voci successive. Reinserire lo span restituito ridà la stessa identica mappa,
che è ciò che serve all'undo.

```rust
let mut map = MeterMap::new(MeterChange::new(Meter::new(&[4], 4)?))?;
map.set_meter_change(BarIndex::new(4), MeterChange::new(Meter::new(&[3], 4)?))?;
// Two empty 4/4 bars before bar 2: the 3/4 moves to bar 6, still on the same written bar.
map.insert_bars(BarIndex::new(2), &BarSpan::empty(2))?;
// Taking them out again returns what they carried (nothing) and gives back the same map.
let span = map.remove_bars(BarIndex::new(2), 2)?;
assert!(span.has_no_entries());
```

Le battute nuove sono esattamente ciò che dice lo span: con `BarSpan::empty(n)` seguono il metro in
vigore subito prima di `at`, e una voce che stava in `at` si sposta con la sua battuta. Inserire alla
battuta 0 sposta anche il cambio di metro della battuta 0, quindi lo span deve portarne uno suo (la
mappa ne vuole sempre uno alla battuta 0); togliere battute può far sparire il cambio di metro che
era in vigore dopo di loro. Decidere che cosa deve conservare la musica che resta è compito del
comando del documento (ADR-0026 §6), non della mappa. Entrambe le operazioni sono tutto-o-niente e
costano O(n log n) nelle voci dei dati di origine, qualunque sia il numero di battute.

**Test** (`tests/meter_map_bars.rs`): otto casi scritti a mano (le battute nuove seguono il metro in
vigore, una voce nel punto d'inserimento si sposta con la sua battuta, inserire all'inizio richiede un
metro, una voce fuori dal suo span è rifiutata, le voci tolte se ne vanno con le loro battute,
togliere l'inizio richiede un metro dopo, oltre l'ultima battuta, zero battute) e due test di
proprietà contro un oracolo che sposta le voci una per una in `BTreeMap`, controllando che le battute
prima del punto di modifica non si muovano e che ogni operazione annulli l'altra. Due bug iniettati
(una voce nel punto d'inserimento lasciata indietro; le voci dopo una rimozione non riportate
indietro) sono stati presi. **Benchmark** (stesso file delle ricerche): inserire quattro battute a
metà della mappa di riferimento ≈ 95 000 istruzioni, toglierne quattro ≈ 94 700, circa quanto un
cambio di metro.

#### Tempo, secondi, campioni e fotogrammi

`src/tempo.rs` e `src/clock.rs` contengono le unità del tempo reale che usa la mappa dei tempi
(passo F1.05b) (ADR-0020 §2, §5, §7):

- **`Tempo`**: un'unità di movimento (una `Duration`: ♩ = 1/4, ♩. = 3/8) e un numero razionale di
  movimenti al minuto (120, oppure `185/2` per 92,5). Quello che conta per il suono è la sua
  **velocità**, `unità × bpm / 60` semibrevi al secondo, quindi ♩ = 120 e ♩. = 80 sono scritti
  diversamente ma vanno alla stessa velocità. A tempo costante `seconds_for` e `duration_in`
  convertono in modo esatto nei due sensi.
- **`Seconds`**: un istante esatto, un newtype sopra `Rational`; `to_f64` esiste solo per la
  visualizzazione.
- **`SampleRate`**: un numero intero di Hz, mai zero. `sample_at(t)` dà il campione **più
  vicino**, `⌊t·rate + 1/2⌋`, con i pareggi al successivo; `time_of_sample(n)` è esattamente
  `n/rate`.
- **`FrameRate`**: un razionale positivo (24, 25, 30000/1001). `frame_at(t)` dà il fotogramma che
  **contiene** `t`, `⌊t·fps⌋`; `frame_start(k)` è esattamente `k/fps`.

```rust
use orchestra_core_time::{Duration, FrameRate, Rational, SampleRate, Seconds, Tempo};

let rate = SampleRate::new(48_000)?;
let half_sample = Seconds::from_rational(Rational::new(1, 96_000)?);
assert_eq!(rate.sample_at(half_sample)?, 1); // a tie goes to the later sample
let ntsc = FrameRate::new(Rational::new(30_000, 1001)?)?;
assert_eq!(ntsc.frame_at(Seconds::from_rational(Rational::ONE))?, 29); // the frame containing 1 s
let quarter_120 = Tempo::new(Duration::from_rational(Rational::new(1, 4)?), Rational::from_integer(120)?)?;
let dotted_80 = Tempo::new(Duration::from_rational(Rational::new(3, 8)?), Rational::from_integer(80)?)?;
assert_eq!(quarter_120.speed()?, dotted_80.speed()?); // both 1/2 whole note per second
```

Perché due regole diverse: un campione è un istante, quindi la risposta giusta è il più vicino e
l'errore è al massimo mezzo campione; un fotogramma è un intervallo di tempo mostrato sullo
schermo, quindi un istante appartiene al fotogramma che lo contiene. È tutto aritmetica razionale
esatta, quindi ogni piattaforma dà lo stesso indice. Gli indici sono `i64` e possono essere
negativi (pre-roll).

Test: `tests/clock.rs` controlla con proptest le regole di arrotondamento **per definizione** (per
esempio `(n − 1/2)/rate ≤ t < (n + 1/2)/rate` ogni volta che `sample_at(t) = n`), che le inverse
ricadano sul proprio indice, che le conversioni a tempo costante tornino al punto di partenza e
che ♩ = x abbia la velocità di ♩. = 2x/3; più pareggi, istanti negativi, un'ora di NTSC ed errori
scritti a mano. Un bug inserito apposta (arrotondare per difetto invece che al campione più
vicino) ha fatto fallire due test. `tests/clock_no_alloc.rs` controlla che nessuna conversione
allochi.

#### La mappa dei tempi

`src/tempo_map.rs` trasforma le posizioni scritte in secondi e viceversa (ADR-0020, corretto da
ADR-0021). I suoi **dati di origine** sono un elenco di `TempoChange` ordinato per posizione, al
più uno per posizione, il primo sempre un cambio immediato alla posizione 0 (il tempo iniziale):

- `TempoChange::Immediate(tempo)`: da qui in poi, questo tempo;
- `TempoChange::Gradual(GradualChange { from, to, end })`: da qui a `end` la velocità va
  linearmente **nel tempo** (accelerazione costante, ADR-0020 §1) fino a `to`, che poi resta.
  `from` è un tempo di partenza facoltativo per "Meno mosso, poco a poco accel." (salto, poi
  cambio graduale); senza, il cambio parte dal tempo in vigore.

Un cambio strettamente dentro un cambio graduale è un errore, mentre un cambio immediato proprio
alla sua fine ("rit. … a tempo") e due cambi graduali uno dopo l'altro vanno bene. Dai dati di
origine la mappa ricava i **segmenti**, costanti o graduali; dopo un cambio graduale non seguito
subito da un altro cambio, un segmento costante implicito mantiene il tempo d'arrivo:

```text
changes:   0: ♩=120        8: rit. to ♩=60 until 12        16: ♩=96
segments: [ constant 120 ][ gradual 120 → 60 ][ constant 60 ][ constant 96 … forever
positions: 0               8                   12             16
```

Il primo segmento copre anche le posizioni e gli istanti negativi (il primo tempo prolungato
all'indietro); l'ultimo continua per sempre.

**Istanti d'inizio su una griglia.** Gli istanti esatti cumulati supererebbero i limiti di
qualunque frazione a larghezza fissa dopo qualche decina di cambi graduali (le misure sono in
ADR-0021). Perciò ogni segmento comincia all'inizio del precedente più la durata **esatta** del
segmento precedente, arrotondata al tick più vicino di `TIME_GRID` = 705 600 000 tick al secondo
(la "flick", divisibile per tutte le frequenze di campionamento e i fotogrammi al secondo comuni).
Dato che l'inizio precedente sta già sulla griglia, basta arrotondare la durata, e gli istanti
d'inizio si tengono come semplici numeri interi di tick:

```rust
// `t_k` is on the grid, so `round(t_k + D) = t_k + round(D)`: integers only.
ticks = ticks
    .checked_add(rational_ticks(duration)?)
    .ok_or(OVERFLOW)?;
```

L'errore è al massimo mezzo tick (≈ 0,7 ns) per confine ed è lo stesso su ogni piattaforma. Dato
che un segmento occupa così `[t_k, t_{k+1})`, che può differire dalla sua durata esatta di mezzo
tick, entrambe le conversioni si **fermano** ai bordi del segmento, così da non tornare mai
indietro attraversando un confine.

**Le due conversioni** sono una ricerca binaria sugli inizi dei segmenti (posizioni o istanti)
seguita da calcoli O(1), senza allocare:

- `time_at(position)` restituisce un `TimeAt`: `Exact(Seconds)` nei segmenti costanti e
  all'inizio di ogni segmento, `Approximate(f64)` dentro un cambio graduale, dove l'istante è
  irrazionale. Lì si calcola come `τ(x) = 2x / (s₀ + √(s₀² + 4h·x))`, con `h = (s₁² − s₀²)/(4L)`
  metà dell'accelerazione: questa forma evita la cancellazione di quella dei libri quando `h` è
  piccolo.
- `position_at(seconds)` è esatto nei segmenti costanti; dentro un cambio graduale calcola
  `s₀·τ + h·τ²` in modo esatto con intermedi `i128` (il valore esatto raramente sta in una
  frazione `i64`) e lo arrotonda al più vicino `1/TIME_GRID` di semibreve, circa 2,8 ns a ♩ = 120.

```rust
let mut map = TempoMap::new(tempo(120)?)?;
let rit = GradualChange { from: None, to: tempo(60)?, end: at(6, 1)? };
map.set_change(at(2, 1)?, TempoChange::Gradual(rit))?;
assert_eq!(map.time_at(at(6, 1)?)?, TimeAt::Exact(secs(44, 3)?)); // 4 s + 32/3 s
assert!(matches!(map.time_at(at(4, 1)?)?, TimeAt::Approximate(_)));
```

L'arrotondamento alla griglia separa prima la parte intera, perché il prodotto con la griglia
resti piccolo; per un istante il cui denominatore non divide la griglia (diciamo `n/3001` s) quel
prodotto può comunque uscire dai limiti, e allora la frazione si riduce col massimo comun divisore
e il prodotto si riprova una volta. Quel che resta è un errore di overflow tipizzato: nei test è
successo per l'1,3% di questi istanti artificiali e mai per istanti di campioni, di fotogrammi o
decimali.

**Le modifiche** funzionano come nella mappa delle misure: `set_change` e `remove_change`
cambiano una copia dei dati di origine, ricostruiscono i segmenti e convalidano tutto,
sostituiscono la mappa solo se va tutto bene (`TempoMapError`: `BeforeStart`, `NoTempoAtStart`,
`GradualAtStart`, `NonPositiveGradual`, `ChangeInsideGradual`, `Arithmetic`) e restituiscono il
valore precedente per l'annullamento.

**Costruire in un colpo solo** (F1.09c, la modifica in blocco di ADR-0022 §8). Siccome il documento
ricava la sua mappa dei tempi dalle indicazioni di tempo dopo ogni modifica che le tocca,
`TempoMap::from_changes` costruisce una mappa da tutto l'elenco dei cambi in una volta: controlla che
le posizioni crescano strettamente (altrimenti `TempoMapError::Unsorted`), poi applica esattamente le
regole delle modifiche. Un test di proprietà ricostruisce ogni mappa modificata a caso dal suo stesso
elenco di cambi e la trova identica.

```rust
// The document rebuilds its tempo map in one go from the changes derived from the tempo marks:
// the same map as one `set_change` per change, in O(t) instead of O(t²).
let map = TempoMap::from_changes(vec![
    (Position::ZERO, TempoChange::Immediate(andante)),
    (at(4, 1)?, TempoChange::Gradual(GradualChange { from: None, to: lento, end: at(5, 1)? })),
])?;
```

**Test.** `tests/tempo_map_cases.rs` ha 12 casi calcolati a mano: il ritardando della tabella di
ADR-0020 (32/3 s), posizioni dentro un cambio sulla griglia e fuori, confini arrotondati per
difetto (♩ = 77) e per eccesso (♩ = 71) con il loro fermo ai bordi, il tempo di partenza
facoltativo, cambi adiacenti, gli errori con la mappa invariata, i valori precedenti, le
posizioni negative, la monotonia. `tests/tempo_map_oracle.rs` confronta mappe casuali con un
**oracolo** che non condivide codice con la libreria: frazioni `i128` sue, segmenti suoi
ricostruiti con un percorso lineare, ricerche lineari, la forma del trapezio `τ·(s₀ + s(τ))/2` e
un altro integrale per gli istanti approssimati; controlla anche che ogni confine stia entro mezzo
tick per ogni confine precedente dall'istante esatto cumulato. Tre bug inseriti apposta (niente
fermo ai bordi, una `h` sbagliata, pareggi arrotondati per difetto) sono stati tutti trovati;
l'oracolo ha trovato anche l'overflow sui denominatori estranei che ora gestisce il ripiego col
massimo comun divisore. `tests/tempo_map_no_alloc.rs` controlla che nessuna ricerca allochi,
compreso il percorso lento.

**Indici di campione e di fotogramma.** `sample_at(position, rate)` dà il campione più vicino e
`frame_at(position, fps)` il fotogramma che contiene l'istante, come `IndexAt`: `Exact(n)` oppure,
solo se il controllo esatto va in overflow, `Estimated(n)` (la stessa idea di `TimeAt`: un'approssimazione non si nasconde mai; il thread audio chiama
semplicemente `.value()`). Nei segmenti costanti si arrotonda l'istante esatto. Dentro un cambio graduale l'istante è irrazionale, ma
l'indice no: l'indice `n` copre un intervallo di tempo con estremi razionali, e la **posizione**
raggiunta in un istante razionale è esatta. Dato che la posizione cresce col tempo, "il punto suona
durante l'indice `n`" equivale a "il punto sta tra le posizioni raggiunte quando l'indice `n`
comincia e quando finisce": due confronti esatti in `i128`. L'istante in `f64` dà la prima ipotesi,
che i confronti confermano o spostano di uno, al massimo quattro volte (un limite fisso, per il
thread audio):

```rust
for _ in 0..=MAX_CORRECTIONS {
    match segment.locate_in_index(x, n, grid, values) {
        Some(Ordering::Equal) => return Ok(IndexAt::Exact(n.min(cap))),
        Some(Ordering::Less) => n = n.checked_sub(1).ok_or(OVERFLOW)?,
        Some(Ordering::Greater) => n = n.checked_add(1).ok_or(OVERFLOW)?,
        None => break,
    }
}
Ok(IndexAt::Estimated(estimate.min(cap)))
```

`cap` è l'indice dell'inizio del segmento successivo: lì `time_at` si ferma, e l'indice deve dargli
ragione. Campioni e fotogrammi condividono il codice attraverso un piccolo `IndexGrid` (l'indice `n`
copre `[(n − offset)/rate, (n + 1 − offset)/rate)`, con `offset` 1/2 per i campioni e 0 per i
fotogrammi). Fuori dal cambio il confronto usa la posizione d'inizio o di fine, mai il polinomio,
che in un rallentando tornerebbe indietro. `position_of_sample(n, rate)` e
`position_of_frame(k, fps)` fanno il percorso inverso, passando per `position_at`.

**Quanto spesso serve la stima?** `tests/tempo_map_fallback.rs` costruisce una partitura scomoda di
2 h 16 min con 750 cambi, 500 dei quali graduali, indicazioni metronomiche con fattori primi grandi
e diverse unità di battito, e chiede 12 834 punti a 44,1, 48, 96, 192 e 384 kHz (384 kHz non divide
la griglia) e a 24, 25 e 30000/1001 fps: **nessuna stima**. Il test fallisce se una frequenza che
divide la griglia ne avesse mai bisogno.

I test degli indici: altri 5 casi calcolati a mano (tempo costante, pareggi, un ritardando
confrontato con un istante calcolato a mano, ogni campione di un cambio che torna a sé stesso, il
fermo ai bordi), un test di proprietà che controlla ogni indice **per definizione** con le frazioni
proprie dell'oracolo, più la monotonia e il ritorno attraverso `position_of_sample`. Cinque bug
inseriti apposta nel codice degli indici sono stati trovati; due solo dopo aver aggiunto due casi
mirati (un indice che oltrepassa il fermo, visibile solo con intervalli più brevi di un tick; un
cambio più breve di un fotogramma, dove il polinomio torna indietro).

**Benchmark** (`benches/tempo_map.rs`): una mappa di riferimento di 150 cambi, due terzi graduali,
circa 250 segmenti, con lotti di 64 ricerche. Primi numeri (macchina di sviluppo, 2026-11-01):

```text
time_at              57893 instructions / 64 lookups ≈  900 each
position_at         129175                            ≈ 2020
sample_at_constant  148272                            ≈ 2320
sample_at_gradual   288218                            ≈ 4500
frame_at            215446                            ≈ 3370
edit_tempo_change   572602 for one edit (clone, rebuild of every segment)
```

Come nella mappa delle misure, quasi tutto il costo è l'aritmetica razionale esatta (i massimi comuni
divisori di denominatori grandi, perché quasi tutti i confini sono fuori dalla griglia), non la
ricerca. 4500 istruzioni sono circa 1,5 µs sull'hardware di riferimento; una modifica circa 0,2 ms,
ben dentro i 16 ms di un aggiornamento dell'interfaccia.

### orchestra-core-model

L'unico documento di progetto (ADR-0003): tracce, note con i loro due strati (intento scritto e
scostamenti di esecuzione, ADR-0004), la forma del brano, i comandi e l'unica cronologia di
annulla/ripristina. Non sa nulla di viste, impaginazione, audio o file. La sua architettura (ID
stabili, eventi come scostamenti relativi alla battuta in blocchi per battuta, voci per strumento)
è fissata da ADR-0022; le parti implementate sono il modulo delle altezze, lo scheletro del
documento, il contenuto delle voci (eventi, gruppi irregolari, legature di valore, chiavi e
armature) e i segni sugli eventi con le legature di portamento.

#### Altezze, intervalli e tonalità

`src/pitch.rs` (passo F1.07, ADR-0023) contiene i tipi puri di cui è fatta l'altezza di ogni nota.
Un `Pitch` è ciò che scrive un musicista: una lettera (`Step`, da C a B, cioè da Do a Si), un'
`Alteration` e un'ottava. Due altezze sono uguali solo se hanno la **stessa grafia**, perché
ADR-0004 conserva l'intento scritto: Fa♯4 e Sol♭4 sono note diverse che per caso suonano uguali, e
`sounds_like` è la domanda separata.

```rust
let b_sharp_3 = Pitch::new(Step::B, Alteration::SHARP, 3);
let c_4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
assert_ne!(b_sharp_3, c_4); // spelled differently…
assert!(b_sharp_3.sounds_like(c_4)?); // …same sound
assert_eq!(c_4.midi()?.key, 60);
```

**L'alterazione è un numero razionale di semitoni** (decisione D07): 1 è un diesis, −1 un bemolle,
1/2 un quarto di diesis, 1/3 un sesto di tono, fino al triplo diesis o triplo bemolle. È il modo in
cui MusicXML scrive `<alter>`, quindi importazione ed esportazione sono dirette, ed è coerente con
il resto del progetto, dove niente è un numero decimale. Quale simbolo disegna un quarto di tono è
affare dell'incisione (F2B); i segni di comma dell'intonazione naturale non sono numeri di semitoni
e aspettano l'intonazione, che sarà una regola successiva di `playback`.

**Le ottave sono scientifiche** (Do4 è il Do centrale) e cambiano al Do: Si♯3 suona come Do4, Do♭4
come Si3. L'**altezza sonora** è esatta, `12 × (ottava + 1) + semitoni della nota naturale +
alterazione` sulla scala MIDI, e `midi()` la trasforma in un `MidiNote`: il tasto più vicino e uno
scostamento (bend) in semitoni. Un quarto di tono cade esattamente tra due tasti; la regola sceglie
il tasto **sotto** e piega verso l'alto di mezzo semitono, così il risultato è deterministico. Un
tasto fuori da 0–127 è un `PitchError::MidiOutOfRange`, mai tagliato in silenzio. Le altezze
cromatiche prendono una scorciatoia con semplici somme di interi.

Un **`Interval`** è una coppia (passi diatonici, semitoni), entrambi con segno: la stessa coppia che
MusicXML usa per gli strumenti traspositori. La **qualità** si ricava dalla differenza tra i suoi
semitoni e quelli dell'intervallo maggiore o giusto dello stesso numero: per unisoni, quarte e
quinte 0 è giusto e ±n è n volte eccedente o diminuito; per gli altri 0 è maggiore, −1 minore, +n
eccedente, −(n+1) diminuito. Un intervallo microtonale non ha una qualità standard e risponde
`None`.

```rust
let c4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
let third = Interval::from_quality(Quality::Major, 3, false)?;
assert_eq!(c4.transposed(third)?, Pitch::new(Step::E, Alteration::NATURAL, 4));
// The B♭ clarinet sounds a major second lower than written:
let clarinet = Interval::from_quality(Quality::Major, 2, true)?;
let written = c4.transposed(clarinet.reversed())?; // D4
```

La **trasposizione** aggiunge i passi al posto dell'altezza nella sequenza delle lettere (che dà la
nuova lettera e l'ottava), poi sceglie l'alterazione che sposta il suono esattamente
dell'intervallo: Fa♯4 più una sesta eccedente è Re𝄪5, mai Mi5. Un risultato oltre il triplo diesis
è un errore.

Le **tonalità** (`Key`, `TraditionalKey`, `Mode`) contano diesis o bemolli in quinte, da −14 a +14
(le tonalità oltre sette, come Re♯ maggiore per un clarinetto in Si♭ che suona in Do♯ maggiore,
escono dalla trasposizione), con sette modi più "nessuno". `Key` è di proposito un enum con un solo
caso: le armature libere (Bartók, maqam…) le ha rimandate il proprietario e saranno un secondo caso,
all'importazione MusicXML (dopo F2B) o al primo bisogno concreto.
I calcoli usano la **linea delle quinte** (… Si♭ −2, Fa −1, Do 0, Sol 1, Re 2 …): una tonalità di k
quinte occupa i sette posti `[k − 1, k + 5]`, quindi l'alterazione di ogni lettera, la tonica di
ogni modo e la trasposizione di una tonalità sono una somma ciascuna. `Pitch::spell_midi` sceglie la
grafia di un tasto MIDI dentro una finestra di dodici quinte, `[k − 4, k + 7]` (Do maggiore: Do♯ Mi♭
Fa♯ La♭ Si♭), spostata di una in minore perché la sensibile sia un diesis (La minore: Sol♯).

**Test.** `tests/pitch_cases.rs` ha 14 gruppi di casi scritti a mano che un musicista può
controllare (Si♯3 e Do♭4, quarti di tono in MIDI, qualità da più che diminuita a più che eccedente,
clarinetto in Si♭, corno in Fa, ottavino, contrabbasso, tonalità teoriche). `tests/pitch_oracle.rs`
confronta la libreria con un oracolo che non condivide con lei nessuna formula: le note naturali
percorrendo la scala maggiore, le tonalità applicando un'alterazione alla volta nell'ordine dei
diesis, le qualità da una tabella di manuale, le toniche come gradi della scala maggiore; controlla
anche ogni tasto MIDI in ogni tonalità e modo, in modo esaustivo. Cinque bug iniettati apposta sono
stati scoperti; uno (una tonica del lidio sbagliata) solo dopo aver aggiunto l'oracolo delle toniche.
`tests/pitch_no_alloc.rs` controlla che niente allochi.

**Benchmark** (`benches/pitch.rs`, lotti da 64, istruzioni per chiamata): conversione MIDI ≈ 155 su
un insieme con un quarto di tono ogni otto note (≈ 650 prima della scorciatoia cromatica),
trasposizione ≈ 560, intervallo e qualità ≈ 800, grafia di un tasto MIDI ≈ 100.

#### Lo scheletro del documento

`src/id.rs` e `src/document.rs` (passo F1.08, ADR-0022 §1–3) contengono la **struttura** del
documento; gli eventi dentro le voci sono descritti nella sezione seguente, la forma arriva in
F1.10.

**ID.** Ogni oggetto a cui qualcos'altro può fare riferimento ha un ID: `TrackId`, `InstrumentId`,
`StaffId`, `VoiceId`, `BarId` (i passi successivi aggiungono eventi, teste di nota e marcature
estese). Sono tipi distinti, quindi il compilatore rifiuta un rigo dove serve una voce, ma sotto
sono tutti valori `u64` presi da **un solo contatore per documento**, che parte da 1 e cresce
soltanto. Un ID non viene mai dato due volte, nemmeno dopo un annulla, così selezioni, cache e passi
di ripristino non possono mai confondere due oggetti. Il valore è un `NonZeroU64`: 0 non è mai un
ID, quindi un `Option<BarId>` occupa 8 byte e non 16. Esaurire gli ID (impossibile in pratica: un
miliardo di oggetti al secondo per 584 anni) è un errore tipizzato, mai un ritorno a capo.

**Struttura.** Documento → tracce (per ora solo tracce di strumento) → uno `Instrument` per traccia,
con il nome che gli ha dato l'utente (testo suo, non una chiave di traduzione), la sua trasposizione
(l'intervallo da scritto a reale di ADR-0023 §7) e i suoi **righi** (un rigo è l'insieme delle righe
su cui si scrivono le note) e le sue **voci**. Una voce appartiene allo strumento, non a un rigo: ha
un *rigo di riferimento*, e da F1.09 ogni nota registra il rigo su cui è disegnata, così una melodia
che passa dalla mano destra alla sinistra di un pianoforte resta una voce sola.

**Battute.** La `BarList` è l'elenco ordinato dei `BarId`: dice quale battuta è quale e quante
sono, e nient'altro. Lunghezze, metri e numeri delle battute restano nella mappa delle misure, che
il documento tiene accanto alla mappa dei tempi: ripeterli qui sarebbe la duplicazione che ADR-0003
vieta. Ciò che è attaccato a una battuta (da F1.09 le note) usa come chiave il `BarId`, così
inserire una battuta non sposterà nient'altro. Un indice inverso (`BarId` → posizione) risponde in
O(1) in media, con una funzione di hash nostra: una moltiplicazione per 2⁶⁴/φ invece del `SipHash`
standard, che difende da chiavi scelte da un attaccante, mentre le nostre chiavi vengono dal nostro
contatore.

**Come si crea un documento.** Con un `DocumentBuilder` (decisione del proprietario, 2026-11-02):
serve per i documenti che non esistono ancora (il file che si apre, il generatore della partitura di
riferimento, test e benchmark), mentre un documento esistente cambierà solo con i comandi di F1.11
(ADR-0003 §4). Uno strumento nuovo riceve **una voce per rigo**, ognuna con il proprio rigo come
rigo di riferimento; `add_voice` ne aggiunge altre su qualunque rigo dello stesso strumento. Non
esistono metro, tempo o tonalità predefiniti: li sceglie chi crea il documento. Da F1.09b il
builder riceve l'armatura iniziale (in suoni reali) e una chiave per rigo al posto del numero di
righi, così non si può costruire un rigo senza chiave né una partitura senza armatura (decisione
del proprietario, 2026-11-02), e `build` restituisce un `Result`. Da F1.09c riceve l'**indicazione
di tempo** iniziale al posto di una mappa dei tempi già fatta, e anche `new` restituisce un
`Result`: la mappa dei tempi si ricava dalle indicazioni (sezioni seguenti).

```rust
let c_major = Key::traditional(0, Mode::Major)?;
let mut builder = DocumentBuilder::new(meter_map, initial_tempo_mark, c_major)?;
for _ in 0..8 {
    builder.add_bar()?;
}
// A piano: two staves with their clefs, so two voices, one at home on each staff.
let unison = Interval::new(0, Rational::ZERO);
let piano = builder.add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])?;
let upper = builder.document().track(piano).unwrap().instrument().staves()[0].id();
builder.add_voice(upper)?; // a second line in the right hand
let document = builder.build()?; // build() validates the document (next section)
assert_eq!(document.bars().len(), 8);
```

Il builder prende gli ID in un ordine documentato (una battuta uno; una traccia di strumento quello
della traccia, quello dello strumento, uno per rigo, uno per voce), così le stesse chiamate danno
sempre lo stesso documento, ID compresi. Ogni metodo controlla tutto **prima** di creare qualcosa:
un errore lascia il builder com'era e non consuma nessun ID.

**Limiti** (decisione del proprietario, 2026-11-02): al massimo **16 righi** e **256 voci** per
strumento. Non sono limiti musicali (l'interfaccia potrà comunque offrire quattro voci per rigo, o
qualunque altro numero) ma tecnici: nessuna partitura vera ci si avvicina, e impediscono a un file
corrotto o malevolo di chiedere milioni di voci quando lo si apre. Alzarli cambia una costante, non
il formato del file.

**Ricerche.** Le battute sono molte, quindi hanno l'indice hash; tracce, righi e voci sono pochi
(qualche centinaio in una grande partitura orchestrale) e si trovano scorrendoli, il che costa meno
che tenere aggiornato un altro indice. L'indice da ID a posizione descritto da ADR-0022 §1 è per gli
eventi, che sono centinaia di migliaia (F1.09).

**Test.** `tests/document_cases.rs` ha 12 casi scritti a mano (ID in sequenza tra tipi diversi, una
voce per rigo, voci aggiunte, strumenti traspositori, inizio delle battute dopo una levare, i limiti
a 16 e 256 senza nulla consumato quando si rifiuta, righi e battute sconosciuti, determinismo, testi
degli errori); `src/id.rs` e `src/document.rs` provano dall'interno l'esaurimento del contatore.
`tests/document_oracle.rs` applica sequenze casuali di chiamate al builder sia alla libreria sia a un
oracolo ingenuo scritto nel test (un semplice contatore e vettori di numeri), confronta l'intero
documento dopo ogni chiamata e controlla che ogni ID sia distinto e che l'indice delle battute sia
l'esatto inverso dell'elenco. Quattro bug iniettati apposta (voci sul rigo sbagliato, limite delle
voci sbagliato di uno, controllo dello spazio nel contatore tolto, indice di battuta sbagliato) sono
stati tutti scoperti. `tests/document_no_alloc.rs` controlla che nessuna ricerca allochi.

**Benchmark** (`benches/document.rs`, uno scheletro grande come la partitura pesante di ADR-0022:
1 000 battute, 20 strumenti, 30 righi, 40 voci; lotti da 64, istruzioni per chiamata): `BarId` →
indice ≈ 42, inizio della battuta ≈ 510 (indice più ricerca nella mappa delle misure tra 20
segmenti), voce per ID con il suo strumento ≈ 280.

#### Il contenuto delle voci

`src/event.rs`, `src/content.rs`, `src/marks.rs` e `src/validate.rs` (passo F1.09b, ADR-0024
§1–§4 e §9) riempiono le voci di musica. I segni sugli eventi, gli scostamenti di esecuzione e le
legature di portamento sono nella sezione seguente, dinamiche, forcelle e indicazioni di tempo in
quella dopo.

**Eventi.** Una voce ha un **blocco** per battuta, allineato all'elenco delle battute: un blocco è
vuoto (`None`, nessuna memoria usata) oppure un `Arc` di un vettore di eventi ordinati per
**offset** dall'inizio della battuta (gli eventi allo stesso offset restano nell'ordine in cui sono
stati inseriti). L'`Arc` serve alle istantanee (ADR-0022 §7): chi legge in background condivide i
blocchi, e una modifica copia solo il blocco che tocca. Un evento è un `Chord` (accordo) o un `Rest`
(pausa), con il suo `EventId` e il rigo su cui è disegnato (di norma il rigo di riferimento della
voce, qualunque rigo dello stesso strumento per le note che passano all'altro rigo). Un accordo ha
una o più `Note`, ognuna con il suo `NoteId`, la sua altezza con grafia in suoni reali e la sua
legatura di valore; le teste di nota restano ordinate per suono (Fa♯4 prima di Sol♭4, per lettera),
e una nota singola non costa nessuna allocazione propria.

**Figure.** Si salva la figura scritta: un `BaseValue` (dalla massima alla 1024ª) e fino a quattro
punti. La lunghezza in semibrevi è **ricavata**, mai salvata: `NoteValue::length` la costruisce con
degli scorrimenti di bit, `Voice::metric_duration` moltiplica il rapporto di ogni gruppo irregolare
che contiene l'evento. Salvare anche il numero vorrebbe dire due copie dello stesso fatto
(ADR-0003). Una **pausa di battuta intera** (`RestValue::WholeBar`) dura quanto dura la sua battuta
nella mappa delle misure, levare comprese, così un cambio di metro non può renderla sbagliata.

**Gruppi irregolari.** `Tuplet::new(3, 2, croma)` vuol dire "tre crome nel tempo di due". I gruppi
irregolari stanno in una tabella della loro voce, e ogni evento registra il gruppo più interno che
lo contiene; `parent` fa dell'annidamento una catena di al massimo quattro (con termini fino a 255
nulla può traboccare). Un gruppo irregolare può attraversare una stanghetta: la nota che starebbe a
cavallo diventa due note legate, entrambe del gruppo. Un gruppo incompleto è uno stato valido
(capita mentre si scrive).

**Legature di valore.** Una legatura di valore è un collegamento salvato sulla testa di nota da cui
parte: `Tie::To(nota)`, oppure `Tie::LetRing` per un laissez-vibrer. La nota di arrivo può stare in
un'altra voce dello stesso strumento e avere un'altra grafia, purché suoni uguale e venga dopo
nell'ordine di partitura (vedi "Ordine di partitura" più sotto).

**Chiavi e armature.** Non possono essere eventi di una voce: un rigo porta più voci, e una voce può
passare da un rigo all'altro. Ogni rigo ha la sua chiave iniziale e un elenco di cambi di chiave a
(battuta, offset); il documento ha la sua armatura iniziale (in suoni reali) e un elenco di cambi;
uno strumento può scostarsi dall'armatura da un punto in poi (`KeyOverride`: un'altra armatura,
"nessuna armatura" per i corni scritti senza, oppure di nuovo quella della partitura). `clef_at`,
`key_at`, `instrument_key_at` e `written_key_at` rispondono con una ricerca binaria; l'armatura
scritta di uno strumento traspositore si ricava dalla sua trasposizione (un clarinetto in Si♭ in Do
maggiore mostra Re maggiore).

```rust
// A triplet of eighths on beat 1, then a dotted quarter from 5/8 to the barline, tied to an
// eighth at the start of the next bar.
let eighth = NoteValue::plain(BaseValue::Eighth);
let triplet = builder.add_tuplet(voice, Tuplet::new(3, 2, eighth)?, None)?;
let g4 = Pitch::new(Step::G, Alteration::NATURAL, 4);
for i in 0..3 {
    // Offsets 0, 1/12 and 2/12 of a whole note.
    let offset = Duration::from_rational(Rational::new(i, 12)?);
    builder.add_chord(Placement::new(voice, bar0, offset).in_tuplet(triplet), eighth, &[g4])?;
}
let dotted_quarter = NoteValue::new(BaseValue::Quarter, 1)?;
let five_eighths = Duration::from_rational(Rational::new(5, 8)?);
let long = builder.add_chord(Placement::new(voice, bar0, five_eighths), dotted_quarter, &[g4])?;
let next = builder.add_chord(Placement::new(voice, bar1, Duration::ZERO), eighth, &[g4])?;
builder.set_tie(long.notes[0], Tie::To(next.notes[0]))?;
let document = builder.build()?;
// Derived, never stored: a dotted quarter lasts 3/8.
assert_eq!(document.metric_duration(long.event)?, Duration::from_rational(Rational::new(3, 8)?));
```

**Cosa controlla `build`.** Ogni metodo del builder controlla ciò che vede localmente prima di
creare qualcosa (battuta, voce, rigo e gruppo irregolare esistono e stanno insieme, gli offset non
sono negativi, l'annidamento è di al massimo quattro livelli). `Document::validate`, che `build`
esegue, controlla le regole che chiedono di più: due eventi di una voce non si sovrappongono dentro
una battuta (i vuoti vanno bene, e una battuta **troppo piena** resta valida: le sue ultime note
vanno semplicemente oltre la stanghetta), una pausa di battuta intera è da sola, e ogni legatura di
valore va allo stesso suono, più avanti, nello stesso strumento. La prima violazione torna come
`Violation` tipizzata, con l'evento o la testa di nota.

**Ricerche.** Un indice ricavato da `EventId` e `NoteId` a (voce, battuta) trova qualunque evento o
testa di nota: una ricerca hash, lo scorrimento delle voci, lo scorrimento di un blocco.
`events(voce, battuta)` dà un blocco, `metric_duration` ed `event_position` il tempo ricavato di un
evento. Nessuna di queste alloca.

**Test.** `tests/voice_cases.rs` ha 16 casi scritti a mano, uno per ciascun caso di ADR-0024 §13 di
questo passo (accordo enarmonico, gruppi irregolari annidati con 1/12 e 1/30, gruppo incompleto,
gruppo a cavallo della stanghetta con la nota divisa e legata, battuta troppo piena, pausa di
battuta intera dopo una levare e un cambio di metro, legature di valore ammesse e vietate, cambi di
chiave a metà battuta, armature di un clarinetto in Si♭, di un corno senza armatura e di un violino
con un'eccezione, note sull'altro rigo, errori di posizionamento). `tests/voice_oracle.rs` confronta
voci casuali con gruppi irregolari annidati casuali con un oracolo che ha le sue frazioni `i128`:
ogni durata, e se `build` riesce o quale violazione riporta; un'altra proprietà confronta
`clef_at` con uno scorrimento lineare. Sei bug iniettati apposta (lunghezza sbagliata con due
punti, gruppo genitore ignorato, sovrapposizione con `<=`, ricerca della chiave sbagliata di un
punto, eventi allo stesso offset inseriti nell'ordine sbagliato, regola della pausa di battuta
allentata) sono stati tutti scoperti; quello dell'ordine di inserimento solo dall'oracolo.
`tests/voice_no_alloc.rs` controlla che nessuna ricerca allochi e che `validate` faccia una sola
allocazione.

**Benchmark** (`benches/voice.rs`, 200 battute × 10 voci, 10 000 eventi con terzine e accordi di
due note; istruzioni per chiamata): durata mentre si percorre un sistema ≈ 320, durata per ID
dell'evento ≈ 625, evento per ID ≈ 200, `clef_at` ≈ 470, `written_key_at` ≈ 555; `validate`
dell'intera partitura ≈ 5,5 milioni (≈ 550 per evento, circa 2 ms).

#### Segni sugli eventi e legature di portamento

`src/marking.rs` e `src/slur.rs` (prima parte del passo F1.09c, ADR-0024 §5–§7 e §10) aggiungono ciò
che è scritto **sugli** eventi, e le legature di portamento. Dinamiche, forcelle e indicazioni di
tempo sono nella sezione seguente.

**Abbellimenti.** Un accordo d'abbellimento è un accordo con un `Grace`: un'acciaccatura o
un'appoggiatura, messa **prima** dell'evento che orna (all'offset di quell'evento) oppure **dopo**
l'evento che segue (all'offset in cui quell'evento finisce: il *Nachschlag*, o le note di chiusura di
un trillo). Conserva la sua figura scritta (la croma che si disegna) ma la sua durata metrica è
**zero**, quindi non sposta mai gli eventi successivi; quanto tempo toglie ai vicini quando suona è
una regola di esecuzione (F1.13). Dato che ora più eventi possono stare allo stesso offset, un
frammento è ordinato per (offset, **slot**): prima gli abbellimenti `After` dell'evento che finisce
lì, poi gli abbellimenti `Before` dell'evento che comincia lì, poi quell'evento. Lo slot si ricava
dall'abbellimento, non si salva. Gli abbellimenti non possono stare in un gruppo irregolare
(`DocumentError::GraceInTuplet`).

**Articolazioni, ornamenti, corone, arpeggi.** Appartengono all'accordo, perché si disegnano una
volta per gambo: un `ArticulationSet` (un insieme di bit in un `u16`: accento, marcato, staccato,
staccatissimo, spiccato, tenuto, portato, stress, unstress, accento morbido; il portato è un segno a
sé, non staccato + tenuto), al massimo un `Ornament` (trillo con l'eventuale alterazione della nota
ausiliaria, gruppetto, gruppetto rovesciato, mordente e mordente superiore, con i nomi di SMuFL
perché quelli di MusicXML vengono spesso scambiati, tremolo su una nota con da 1 a 4 tagli), una
`Fermata` (corona breve, normale, lunga, molto lunga; anche su una pausa, così una pausa generale è
una pausa di battuta intera con la corona) e un `Arpeggio` (semplice, verso l'alto, verso il basso, o
la parentesi di "non arpeggiato"). Mettere su una pausa uno dei segni riservati agli accordi dà
`DocumentError::NotAChord`.

**Scostamenti di esecuzione.** Ogni testa di nota porta i suoi `PerformanceOffsets` al proprio
interno: `onset` e `length` come durate con segno in **tempo musicale** (una nota anticipata di un
sessantaquattresimo ha `onset = −1/64`, così il "feeling" segue ogni cambio di tempo) e `velocity`
come differenza con segno sulla scala a 16 bit di MIDI 2.0. Valgono zero per default e si salvano
esattamente come arrivano: i limiti (una nota prima dello zero, una lunghezza negativa, una velocity
fuori scala) li applica l'esecuzione al risultato, così non si perde nulla quando più avanti
cambiano le dinamiche scritte.

**Legature di portamento.** Uno `Slur` unisce un evento di partenza e uno di arrivo e sta nella
tabella del suo **strumento**, ordinata per ID, perché può unire due voci dello strumento (una frase
che passa da una mano all'altra del pianoforte). Le legature possono annidarsi e condividere gli
estremi. Un indice derivato risponde a "quali legature cominciano o finiscono su questo evento"
(`slurs_at`) con una sola ricerca nella tabella hash; fino a due legature per evento tiene gli ID al
proprio interno, quindi non costa un'allocazione per voce dell'indice.

**Ordine di partitura.** Con gli abbellimenti "dopo" non vuol più dire solo "in una posizione
successiva": un'acciaccatura e la sua nota principale cominciano nello stesso punto.
`Document::comes_before` confronta gli eventi per posizione, poi per slot, poi (dentro un frammento)
per il loro posto nel frammento; due eventi di voci diverse con la stessa posizione e lo stesso slot
sono simultanei. Le legature di valore e di portamento devono andare avanti in quest'ordine
(decisione del proprietario del 2026-11-02), così un'acciaccatura può essere legata, di portamento o
di valore, alla sua nota principale; per gli eventi senza abbellimenti la regola è la stessa di
prima.

```rust
// An acciaccatura slurred to an accented quarter with a trill; the quarter played a 64th early.
let quarter = NoteValue::plain(BaseValue::Quarter);
let main = builder.add_chord(Placement::new(voice, bar, Duration::ZERO), quarter, &[g4])?;
let grace = Grace { kind: GraceKind::Acciaccatura, attach: GraceAttach::Before };
let eighth = NoteValue::plain(BaseValue::Eighth);
let a4 = Pitch::new(Step::A, Alteration::NATURAL, 4);
let small = builder.add_grace_chord(Placement::new(voice, bar, Duration::ZERO), grace, eighth, &[a4])?;
builder.set_articulations(main.event, ArticulationSet::EMPTY.with(Articulation::Accent))?;
builder.set_ornament(main.event, Some(Ornament::Trill { auxiliary: None }))?;
let early = PerformanceOffsets { onset: Duration::from_rational(Rational::new(-1, 64)?), ..PerformanceOffsets::NONE };
builder.set_offsets(main.notes[0], early)?;
let slur = builder.add_slur(small.event, main.event, SlurPlacement::Auto)?;
let document = builder.build()?;
// The grace lasts zero and comes first in score order, although both start at the same point.
assert_eq!(document.metric_duration(small.event)?, Duration::ZERO);
assert!(document.comes_before(small.event, main.event)?);
assert_eq!(document.slurs_at(main.event), [slur]);
```

**Cosa aggiunge `validate`.** Regola 4: un abbellimento `Before` ha un evento principale al suo
offset nella sua voce e battuta, un abbellimento `After` sta esattamente dove finisce l'evento
principale precedente del suo frammento (anche più d'uno di fila); un abbellimento dentro un evento
principale viene segnalato come sovrapposizione. Regola 5: i due estremi di una legatura di
portamento esistono e appartengono al suo strumento, e l'arrivo viene dopo la partenza nell'ordine
di partitura. Le due regole girano nella stessa unica passata sui frammenti e non aggiungono
allocazioni.

**Test.** `tests/marking_cases.rs` ha 9 casi scritti a mano (andata e ritorno delle articolazioni e
il portato, abbellimenti di durata zero e il loro ordine per slot, un abbellimento rifiutato in un
gruppo irregolare, abbellimenti senza il loro evento principale, ornamenti con i limiti del tremolo e
corone sulle pause, scostamenti per testa di nota, legature che si annidano e passano da una voce
all'altra con l'indice, ordine di partitura con abbellimenti e voci simultanee, errori sulle legature
che non consumano ID). `tests/marking_oracle.rs` costruisce battute casuali di note principali e
abbellimenti con legature casuali e le confronta con un oracolo che conta in sedicesimi come semplici
interi: l'ordine del frammento, se `build` riesce, `comes_before` su ogni coppia, `slurs_at` su ogni
evento e la durata zero degli abbellimenti. Quattro errori introdotti apposta (slot scambiati, la
regola `After` spenta, lo spareggio dell'ordine di partitura dentro un frammento rovesciato, l'indice
che perde la terza legatura) sono stati tutti scoperti dall'oracolo. `tests/marking_no_alloc.rs`
controlla che nessuna ricerca allochi e che `validate` faccia ancora una sola allocazione.

**Benchmark** (`benches/marking.rs`, 200 battute × 10 voci, 12 000 eventi con un abbellimento prima
e dopo ogni battuta, 4 500 legature; istruzioni per chiamata): `slurs_at` ≈ 48, legatura per ID
≈ 640, `comes_before` ≈ 1 670; `validate` dell'intera partitura ≈ 13,8 milioni (≈ 1 150 per evento,
circa 4,5 ms). La regola 4 gira dentro la passata della regola 1 e ne riusa le durate già calcolate:
una passata separata aveva raddoppiato il costo di `validate`. Sulla partitura di `benches/voice.rs`,
che non ha abbellimenti né legature di portamento, `validate` è passato da ≈ 5,49 a ≈ 5,76 milioni
(+5%, circa 27 istruzioni per evento per i controlli nuovi); tutte le ricerche sono rimaste entro
±0,5%.

#### Dinamiche, forcelle e indicazioni di tempo

`src/dynamic.rs` e `src/tempo_mark.rs` (seconda parte del passo F1.09c, ADR-0024 §8 e §11)
aggiungono quanto forte e quanto veloce la musica è scritta. Entrambe sono **ancorate a una
posizione**: stanno in un `ScorePoint` (una battuta e un offset, nuovo in `src/marks.rs`), non su una
nota, così un *p* sotto una pausa o un cambio di tempo a metà di un accordo tenuto sono scrittura
normale, e seguono la loro battuta quando se ne inseriscono altre prima.

**Dinamiche.** Una `Dynamic` dice un `DynamicMark`: un livello della scala (`DynamicLevel`, da
*pppppp* a *ffffff*, ordinati dal piano al forte così che `Ord` risponda a "è più forte?"), uno degli
accenti e delle combinazioni (*sf*, *sfz*, *sffz*, *fz*, *rf*, *rfz*, *fp*, *sfp*, *sfpp*, *pf*,
*n*), oppure un testo libero (`OtherDynamic`, "subito p", con un livello facoltativo per
l'esecuzione). I nomi standard sono italiani in tutte le lingue e si disegnano, non si traducono; il
testo libero è dell'utente e non è mai vuoto (`DocumentError::EmptyText`). Ogni strumento tiene le
sue dinamiche in un elenco ordinato secondo la partitura. Un `DynamicPlacement` contiene due cose
distinte: l'**ambito** (tutto lo strumento, il caso normale, compresa la dinamica di un pianoforte
fra i due righi, oppure una sola voce, per i divisi) e il **rigo** su cui è disegnata, che non cambia
nulla nell'esecuzione. C'è al massimo una dinamica per ambito in un punto (`DuplicateMark`); una
dinamica di strumento e una di voce possono condividerlo. Qui niente è una velocity: l'esecuzione
(F1.13) ricava le velocity dalle dinamiche, e gli scostamenti di velocity delle note sono delta in
più, perciò un test controlla che aggiungere dinamiche non tocchi mai una nota.

**Forcelle.** Una `Hairpin` va da un punto d'inizio a un punto di fine **escluso**, con una
`HairpinShape` (crescendo o diminuendo, a cuneo o a testo, *al niente*). Il builder rifiuta una
forcella che non va in avanti (`HairpinNotForward`); `validate` (regola 6) rifiuta due forcelle dello
stesso ambito che si sovrappongono, con una sola passata sull'elenco dello strumento ordinato per
inizio, ricordando l'ultima fine per ambito. Toccarsi va bene ("cresc. fino a qui, dim. da qui"), e
così una dinamica dentro una forcella.

**Indicazioni di tempo** (scelta C5 = T2 di ADR-0024). Un `TempoMark` è ciò che si stampa, testo e/o
un cambio: `TempoMarkChange::Immediate(Metronome)` (un tempo, stampato o nascosto: uno nascosto è
l'automazione del tempo di chi usa la DAW) oppure `Gradual(GradualMark)` (verso un tempo, fino a un
punto di fine esplicito, con un eventuale salto iniziale a un tempo di partenza). La **mappa dei
tempi si ricava dalle indicazioni** e non si modifica mai direttamente, così la partitura non può
dire 120 e suonare 90. Le regole approvate dal proprietario (E1–E3): l'indicazione iniziale è un
campo del documento con il suo `TempoMarkId`, prende il primo ID del documento e deve fissare subito
un tempo (`InitialTempoNotImmediate`); un'indicazione di solo testo non cambia nulla nella mappa; un
cambio graduale finisce in un suo punto.

Come si ricava la mappa (`Document::walk_tempo_marks`, poi `TempoMap::from_changes`): le indicazioni
si leggono in ordine di partitura; un cambio graduale deve finire dopo l'inizio e nessun altro cambio
può cominciare al suo interno, giudicati entrambi in ordine di partitura, su ciò che vede l'utente
(`Violation::GradualTempoNotForward`, `Violation::TempoChangeInsideGradual`). Ogni punto diventa
l'inizio della sua battuta più l'offset, **fermato alla stanghetta** (decisione del proprietario del
2026-11-03): una battuta troppo piena è uno stato valido, e senza questo limite un'indicazione
all'offset 5/4 di una battuta in 4/4 finirebbe dopo un'indicazione all'inizio della battuta seguente.
Il limite mantiene l'ordine; due indicazioni possono allora cadere nella stessa posizione, e vince
quella che viene dopo nella partitura (l'altra resta scritta); un cambio graduale ridotto a niente
vale come cambio immediato al suo tempo d'arrivo. `add_tempo_mark` prova l'indicazione con l'ID che il
contatore darebbe come prossimo (`IdCounter::peek`), ricava la mappa e, se qualcosa non va, la
toglie di nuovo: tutto o niente, nessun ID consumato. `validate` (regola 7) fa lo stesso percorso
senza costruire la mappa, quindi non alloca nulla di più.

```rust
// "Andante ♩ = 60" at the start, a ritardando over bar 4 and "a tempo" at bar 5.
let quarter = Duration::from_rational(Rational::new(1, 4)?);
let andante_tempo = Tempo::new(quarter, Rational::from_integer(60)?)?;
let andante = TempoMark::immediate(Some("Andante"), Metronome::shown(andante_tempo))?;
let mut builder = DocumentBuilder::new(meter_map, andante, c_major)?;
let bars: Vec<BarId> = (0..8).map(|_| builder.add_bar()).collect::<Result<_, _>>()?;
let rit = GradualMark { from: None, to: Tempo::new(quarter, Rational::from_integer(40)?)?, end: ScorePoint::start_of(bars[5]) };
builder.add_tempo_mark(ScorePoint::start_of(bars[4]), TempoMark::new(Some("rit."), Some(TempoMarkChange::Gradual(rit)))?)?;
builder.add_tempo_mark(ScorePoint::start_of(bars[5]), TempoMark::immediate(Some("a tempo"), Metronome::hidden(andante_tempo))?)?;
// A p for the whole piano under the upper staff, and a crescendo over bar 2.
builder.add_dynamic(piano, ScorePoint::start_of(bars[0]), DynamicPlacement::instrument(upper), DynamicMark::Level(DynamicLevel::P))?;
builder.add_hairpin(piano, ScorePoint::start_of(bars[2]), ScorePoint::start_of(bars[3]), DynamicPlacement::instrument(upper), HairpinShape::CRESCENDO)?;
let document = builder.build()?;
// The tempo map is derived: 0 → ♩ = 60, 4 → rit. to ♩ = 40 until 5, 5 → ♩ = 60.
assert_eq!(document.tempo_map().changes().len(), 3);
```

**Test.** `tests/dynamic_cases.rs` (7 casi: l'ordine dei livelli, l'ordinamento e una dinamica per
ambito, i controlli di collocazione senza ID consumati, il testo libero, "una dinamica non tocca mai
gli scostamenti di una nota" di ADR-0024 §13, forcelle che si toccano, sovrapposizioni solo dentro un
ambito) e `tests/tempo_mark_cases.rs` (11 casi calcolati a mano: l'indicazione iniziale, le
indicazioni vuote, quelle di solo testo e nascoste, un ritardando con "a tempo", un tempo di
partenza, un cambio dentro un graduale rifiutato tutto o niente, cambi graduali che non vanno in
avanti, i punti, i metri, il limite alla stanghetta con "vince la successiva", un cambio graduale
ridotto a niente). `tests/tempo_mark_oracle.rs` confronta indicazioni e forcelle casuali su metri
casuali con un oracolo che conta in crome: se ogni indicazione è accettata e con quale errore, il
builder invariato dopo un rifiuto, la mappa ricavata e la prima sovrapposizione di forcelle che
`build` segnala. Sette bug iniettati (niente limite alla stanghetta, vince l'indicazione precedente,
un cambio graduale ridotto a niente tenuto, "dentro" con `<=`, forcelle che si toccano trattate come
sovrapposte, ambiti fusi, dinamiche duplicate ammesse) sono stati tutti scoperti, i primi sei anche
dall'oracolo. `tests/dynamic_no_alloc.rs` controlla che leggere gli elenchi, l'indicazione in vigore
in un punto e la mappa ricavata non allochino mai, e che `validate` faccia al massimo due
allocazioni.

**Memoria per nota** (scelta E7 del proprietario). `tests/memory.rs` costruisce con il builder vero
una partitura con la forma di quella pesante di `spikes/model/` (1 000 battute, 30 righi, 41 voci,
221 169 teste di nota; niente gruppi irregolari) e misura ciò che il documento trattiene con il nuovo
`net_bytes` di `orchestra-test-support`: **257 byte per nota** (56,9 MB), compresi eventi, indici
ricavati e scostamenti in linea. Il test fallisce oltre **285** (la misura più circa il 10%), così che
una struttura che cresce sia una decisione voluta.

**Benchmark** (`benches/dynamic.rs`; istruzioni): aggiungere un'indicazione di tempo vicino all'inizio
di 1 000 battute con 250 indicazioni, metà graduali, che ricava di nuovo tutta la mappa, ≈ 985 000
(circa 0,3 ms, dentro la stima di 0,6 ms di ADR-0024); l'indicazione in vigore in un punto ≈ 620;
`validate` di 200 battute × 10 strumenti con 8 000 eventi, 2 000 dinamiche e 2 000 forcelle ≈ 3,3
milioni. Rispetto alla prima parte di F1.09c tutti i benchmark del programma già esistenti sono
rimasti entro ±0,9%.

#### La forma: ritornelli e volte

`src/form.rs` (prima parte del passo F1.10b, ADR-0025 §2–§3) aggiunge la forma del brano: ciò che è
scritto una volta e che `orchestra-playback` (F1.12) **srotolerà** in ciò che si suona. Segni, salti,
ripetizioni di battuta e regole di passaggio sono nella sezione seguente.

**Sulle stanghette, nel documento.** Un `RepeatStart` (‖:) appartiene all'inizio di una battuta, un
`RepeatEnd` (:‖) alla sua fine, una `Volta` a una serie di battute intere (ADR-0025 F1): niente ha
un offset, quindi un passaggio suonato è sempre una serie di battute intere. Un ritornello a metà
di una battuta scritta si scrive dividendo la battuta in due battute irregolari, come ADR-0018
permette già. La forma appartiene al **documento** (`Document::form`), non a uno strumento: una
stanghetta di ritornello attraversa tutto il sistema.

**Passaggi.** Un `PassSet` è un insieme di passaggi 1–64 in un solo `u64` (il bit `k − 1` per il
passaggio `k`): appartenenza, unione, disgiunzione e "è esattamente 1..=n?" sono singole operazioni
sui bit. 64 è `MAX_PLAYS`, il massimo di esecuzioni di una sezione (F10).

**Ritornelli.** Una fine di ritornello porta `times`, il numero totale di esecuzioni (2 di default,
da 2 a 64). Una fine senza inizio torna all'inizio della partitura (F2); le sezioni ripetute non si
annidano né si sovrappongono mai (F3). Le **volte** formano gruppi: un gruppo comincia dalla volta
che contiene il passaggio 1, le volte seguenti vengono subito dopo con passaggi più alti, insieme
contengono esattamente 1..=n, sono almeno due, e **ogni volta tranne l'ultima finisce con una fine
di ritornello** della sezione ("1. :‖ 2. :‖ 3."; il proprietario ha corretto ADR-0025 su questo
punto durante l'implementazione). Con le volte, `times` deve valere 2 oppure n: una contraddizione
si rifiuta, non si ignora (decisione del proprietario del 2026-11-03).

**Cosa aggiunge `validate`.** La regola 8 unisce gli inizi e le fini di ritornello in ordine di
partitura e li accoppia: un inizio non deve trovarne un altro aperto; una fine senza un inizio
aperto va bene solo come prima fine della partitura, o come fine successiva di un gruppo di volte.
La regola 9 legge le volte una volta sola, le divide in gruppi e controlla ognuno. Nessuna delle due
alloca.

**Numeri di battuta** (`Document::bar_label`, ADR-0025 F9). Uno stile `BarNumbering` per documento:
`Continue` (il default: il numero è la battuta scritta), `SameAsFirstLettered` ("8a", "8b") e
`SameAsFirst` ("8", "8"). Il numero semplice viene dalla mappa delle misure con le sue battute non
contate e le ripartenze; gli altri due stili lo modificano sopra: le battute di una volta successiva
prendono i numeri delle battute contate della prima volta (proseguendo se la volta successiva è più
lunga: 8a | 8b 9b), le battute dopo un gruppo tornano indietro di quanto il gruppo aggiunge oltre la
sua volta più lunga, e una ripartenza della numerazione dentro un gruppo vince sullo stile. Le
battute contate di una volta si contano con due ricerche binarie nell'elenco delle battute non
contate, quindi un numero costa O(v log u) per le v volte che lo precedono.

```rust
// ‖: bars 1–7 [1. bar 8 :‖] [2. bar 9]: the first example of ADR-0025 §3.
builder.add_repeat_start(bars[0])?;
builder.add_volta(bars[7], bars[7], PassSet::range(1, 1)?, true, None)?;
builder.add_repeat_end(bars[7], 2)?;
builder.add_volta(bars[8], bars[8], PassSet::range(2, 2)?, false, None)?;
builder.set_bar_numbering(BarNumbering::SameAsFirstLettered);
let document = builder.build()?;
// The second volta is "8b", and the bar after it is 9 again.
let label = document.bar_label(bars[8])?.unwrap();
assert_eq!((label.number, label.letter()), (8, Some('b')));
assert_eq!(document.bar_label(bars[9])?.unwrap().number, 9);
```

**Test.** `tests/form_cases.rs` ha 12 casi scritti a mano: insiemi di passaggi, ritornelli
semplici, annidamento rifiutato, volte e "1.–3.", "1. :‖ 2. :‖ 3.", ogni regola delle volte, i
controlli del builder, e i numeri di battuta degli esempi di ADR-0025 nei tre stili, con volte
successive più lunghe, più gruppi, battute non contate e ripartenze. `tests/form_oracle.rs` genera
sezioni, volte e mutazioni casuali (metà dei casi validi, metà no) e le confronta con un oracolo che
ridice le regole e numera le battute **una per una** con un contatore, come farebbe un copista,
mentre la libreria calcola ogni numero da solo con degli spostamenti. L'oracolo ha trovato un caso
vero: una prima volta con tutte le battute non contate, dove la libreria usava il numero della
battuta stessa; ora le volte partono dalla prima battuta contata del gruppo. Nove bug iniettati
sono stati tutti scoperti, sette dall'oracolo fin dall'inizio e gli altri due dopo aver aggiunto le
mutazioni che li producono (un gruppo di una sola volta, un `times` contraddittorio).
`tests/form_no_alloc.rs` controlla che i numeri e gli elenchi della forma non allochino mai e che le
regole 8 e 9 non aggiungano allocazioni a `validate`.

**Benchmark** (`benches/form.rs`, 400 battute in 50 sezioni con 150 volte, molto più di una
partitura reale; istruzioni per chiamata): `bar_label` con `Continue` ≈ 112, con lo stile a lettere
≈ 13 800 per battute dopo 25–50 gruppi di volte (circa 275 per gruppo: una partitura reale ne ha
pochi), da ≈ 18 800 prima del conteggio con le ricerche binarie; `validate` della forma ≈ 178 000. Tutti i benchmark già
esistenti sono rimasti entro ±0,01% rispetto a `main`.

#### Segni, salti, ripetizioni di battuta e regole di passaggio

`src/jump.rs` e `src/bar_repeat.rs` (seconda parte del passo F1.10b, ADR-0025 §4 e §6–§7)
completano la forma.

**Segni e salti.** Un `Marker` è un segno o una coda all'inizio della sua battuta, oppure un Fine o
un "To Coda" alla sua fine (`MarkerKind::at_start`). Un `Jump`, alla fine della sua battuta, è la
terna che usano tutti i programmi di notazione, "vai a / suona fino a / continua da", in forma di
`JumpSpec`: `to` (l'inizio della partitura o un segno), `until` (un Fine o un "To Coda"), `then`
(la coda dopo un "To Coda"), più le due scelte di ADR-0025: `RepeatsAfterJump` (`Skip` di default:
ogni sezione ripetuta una volta, solo la sua ultima volta; `Play`) e `JumpFires` (`AfterRepeats` di
default: all'ultimo passaggio della sezione che contiene il salto; `OnPass(n)`). I segni si
indicano per **ID**, mai con un'etichetta, così un D.S.S. non può puntare al segno sbagliato.
`JumpSpec::da_capo()`, `dal_segno(segno)`, `.al_fine(fine)` e `.al_coda(to_coda, coda)` costruiscono
quelli usuali; un `text` sostituisce le parole ricavate per i salti scritti come testo libero
("D.C. senza ritornelli"), e ce l'hanno anche segni e volte. Al massimo un segno d'inizio, un segno
di fine e un salto per battuta.

**Ripetizioni di battuta** (`BarRepeat`): `count` battute da `first`, su un solo **rigo**, ripetono
le `span` battute precedenti (`BarRepeatSpan::One`, `Two`, `Four`); `count` è un multiplo positivo
dell'ampiezza. Le battute ripetute **non contengono eventi** su quel rigo: F1.12 suonerà le battute
d'origine, e una modifica dell'origine arriva a ogni ripetizione, perché non c'è nessuna copia.
**Regole di passaggio** (`PassRule`): un tratto di uno strumento o di una voce suonato solo nei
passaggi di un `PassSet` della sezione ripetuta che lo contiene ("tacet la 1ª volta" = {2}).

**Cosa aggiunge `validate`.** Regola 10: i bersagli sono del tipo giusto, il segno del "fino a" sta
fra il bersaglio e il salto in ordine di partitura (altrimenti il salto non si fermerebbe mai lì), e
`OnPass(n)` vale solo per un salto dentro una sezione ripetuta con almeno n esecuzioni. Regola 11:
una ripetizione di battuta ha le sue battute d'origine, le sue battute non contengono eventi sul suo
rigo, due dello stesso rigo non si sovrappongono, e nessuna marcatura di forma sta fra la sua prima
battuta d'origine e la sua ultima battuta ripetuta (altrimenti "le battute precedenti"
dipenderebbero dalla strada fatta); una regola di passaggio sta dentro una sola sezione ripetuta, i
suoi passaggi sono entro le esecuzioni della sezione, e due dello stesso ambito non si
sovrappongono. Entrambe hanno bisogno della "sezione ripetuta che contiene questa battuta":
`Document::section_at` unisce le stanghette di ritornello come la regola 8, allunga una sezione
fino alle fini successive delle sue volte, si ferma alla prima sezione dopo la battuta e calcola il
numero di esecuzioni solo per la sezione trovata.

```rust
// Segno at bar 2, a repeated chorus 2–4 with "To Coda" at the end of bar 3, D.S. al Coda at the
// end of bar 5, coda at bar 6: the jump is taken after the chorus's last pass, and the repeat is
// skipped after it (the classical convention, ADR-0025 F4).
let segno = builder.add_marker(bars[2], MarkerKind::Segno, None)?;
builder.add_repeat_start(bars[2])?;
let to_coda = builder.add_marker(bars[3], MarkerKind::ToCoda, None)?;
builder.add_repeat_end(bars[4], 2)?;
let coda = builder.add_marker(bars[6], MarkerKind::Coda, None)?;
builder.add_jump(bars[5], JumpSpec::dal_segno(segno).al_coda(to_coda, coda), None)?;
// Two bars of the piano's left hand repeated with the two-bar sign; the trumpet tacet the first
// time through the chorus.
builder.add_bar_repeat(lower_staff, bars[7], 2, BarRepeatSpan::Two)?;
builder.add_pass_rule(trumpet, ScorePoint::start_of(bars[2]), ScorePoint::start_of(bars[5]),
    DynamicScope::Instrument, PassSet::range(2, 2)?, Some("1st time tacet"))?;
let document = builder.build()?;
```

**Test.** `tests/jump_cases.rs` ha 11 casi scritti a mano: un D.S. al Coda intorno a un ritornello,
un D.C. al Fine con i ritornelli suonati e un testo libero, un salto a un passaggio scelto, due
segni, i tipi dei bersagli, il "fino a" fra bersaglio e salto, `OnPass` fuori da una sezione o oltre
le sue esecuzioni, i controlli del builder, ripetizioni di battuta sul rigo inferiore del pianoforte
con una sovrapposizione e un rigo non vuoto, l'origine e le marcature di forma di una ripetizione di
battuta, le regole di passaggio con le loro violazioni, e una sezione con "1. :‖ 2. :‖ 3." che
arriva alla sua seconda volta. `tests/jump_oracle.rs` costruisce una forma valida con segni, salti,
ripetizioni di battuta, regole di passaggio e note della mano sinistra casuali (ogni caso con solo
alcuni di questi, circa un terzo dei casi valido) e confronta `build` con un oracolo che ridice le
regole 10 e 11 su semplici interi. Dieci bug iniettati sono stati tutti scoperti, otto
dall'oracolo; all'inizio il generatore produceva quasi solo casi non validi (109 validi su 1 024,
pochissimi con salti), ed è stato orientato verso oggetti validi. `tests/form_no_alloc.rs` ora ha
anche segni, un salto e una ripetizione di battuta: leggerli non alloca mai, e `validate` fa ancora
una sola allocazione (il controllo delle regole di passaggio alloca un piccolo elenco di ambiti solo
quando uno strumento ha regole di passaggio).

**Benchmark** (`benches/form.rs`, stessa partitura): `validate` con un segno, un Fine e un D.S. al
Fine al passaggio 2 in ognuna delle 50 sezioni ≈ 933 000 istruzioni, circa 15 000 per salto per la
ricerca della sezione su una partitura molto oltre quelle reali; erano ≈ 1,96 milioni prima che la
ricerca si fermasse alla prima sezione dopo la battuta e calcolasse le esecuzioni una volta sola.

#### Modifiche, transazioni e undo

Dalla prima parte di F1.11b un documento esistente cambia solo attraverso un **`Editor`**
(ADR-0003 §4, ADR-0026): il documento, la sua unica cronologia di undo e la transazione in corso.
Quattro moduli nuovi:

- `edit`: le **modifiche elementari**, tre verbi su un unico enum di tipi di oggetto più le
  operazioni sulle battute e sul metro e i valori che il documento ha una volta sola:

  ```text
  Edit = Insert(Object) | Remove(ObjectRef) | Replace(Object)
       | InsertBars { at, ids, meter } | RemoveBars { at, count }
       | Meter(MeterEdit) | SetInitial(Initial)
  ```

  Applicare una modifica (`Document::apply_edit`, interna al crate) restituisce la sua **inversa
  esatta**: un `Insert` dà il `Remove` dello stesso ID, un `Remove` l'`Insert` dell'oggetto intero
  con il suo posto esatto, un `Replace` un `Replace` con il contenuto vecchio, `InsertBars` dà
  `RemoveBars` e viceversa. L'undo applica le inverse in ordine contrario, e ciò che restituisce è
  esattamente il redo.
- `change`: l'**insieme delle modifiche** (`ChangeSet`) che ogni transazione, undo e redo
  restituisce: le celle (voce, battuta) i cui eventi sono cambiati, gli oggetti inseriti, tolti e
  sostituiti (battute comprese), la prima battuta da cui la mappa delle misure può rispondere
  diversamente, la prima posizione da cui la mappa dei tempi è diversa, e se la forma è cambiata.
  Ogni modifica registra le proprie voci, quindi nessuna notifica può essere dimenticata; gli elenchi
  si ordinano e si ripuliscono dai doppioni una volta sola alla fine.
- `editor`: `Editor::transact` esegue una chiusura come **una transazione = un passo di undo**; se
  la chiusura restituisce un errore, ogni modifica che ha applicato viene annullata. La mappa dei
  tempi si ricava di nuovo una volta sola, alla fine, e solo se è cambiata un'indicazione di tempo o
  l'inizio di una battuta; se fallisce, anche la transazione viene annullata. La cronologia è
  **lineare** e limitata in **memoria** (`DEFAULT_HISTORY_LIMIT`, 128 MB, l'ultimo passo sempre
  tenuto), e il documento ha ora un numero di **revisione**.
- `editor_tests` (solo test, vedi sotto).

```rust
let mut editor = Editor::new(document);
// One transaction = one step of undo. `?` on a refused edit reverts the whole transaction.
let changes = editor.transact(|tx| {
    tx.apply(Edit::Remove(ObjectRef::Event(grace)))?;
    tx.apply(Edit::Remove(ObjectRef::Slur(slur)))
})?;
assert_eq!(changes.removed().len(), 2);
editor.undo()?;   // both come back, with the same IDs and in the same places
editor.redo()?;   // and go again
```

**Che cosa controlla una modifica.** La struttura: il contenitore giusto, elenchi ordinati, indici
derivati allineati, ID unici e già distribuiti dal contatore (un ID non ancora distribuito
entrerebbe in collisione con un oggetto futuro). Gli invarianti musicali (niente sovrapposizioni in
una voce, legature di valore alla stessa altezza, una forma valida) sono compito dei comandi, nella
seconda parte. Ne segue una regola: **un inserimento controlla solo ciò che nessun'altra modifica
può togliere**. Le battute si possono togliere solo quando nulla vi fa riferimento
(`BarsNotEmpty`), i gruppi irregolari solo quando nessun evento li usa (`TupletInUse`); una legatura
di portamento invece non controlla che i suoi eventi esistano, né un salto che esistano i suoi segni,
perché una modifica successiva della stessa transazione può averli tolti, e allora l'undo
fallirebbe.

**Ordini che non dipendono dalla storia.** Dove un elenco può avere più oggetti nello stesso punto
(dinamiche, forcelle, volte, ripetizioni di battuta, regole di passaggio) sono ordinati per (punto,
ID), e l'indice "legature su un evento" per ID: togliere un oggetto e reinserirlo dà un elenco
identico. Gli eventi sono l'eccezione, perché nello stesso (offset, slot) il loro ordine è musicale
(abbellimenti in fila): un evento tolto ricorda il suo posto nel blocco (`Chunk::remove` e
`Chunk::insert_at`). Un blocco svuotato torna a "nessuna allocazione", quindi risulta uguale a un
blocco che non ha mai contenuto nulla. `Document::same_content` confronta due documenti ignorando il
contatore degli ID e la revisione, che crescono soltanto; `Document::indices_are_consistent`
ricostruisce ogni indice derivato e confronta.

**Test** (`src/editor_tests.rs`, dentro il crate perché i costruttori degli oggetti sono interni):
nove casi scritti a mano (abbellimenti di nuovo nel loro ordine dopo un undo, una transazione fallita,
una transazione vuota, gli insiemi delle modifiche delle due mappe, battute occupate, ID mai
distribuiti, la cronologia limitata e lineare, rifiuti strutturali) e un test di proprietà: un
documento ricco (ogni tipo di oggetto) riceve transazioni casuali di modifiche casuali, molte
rifiutate, alternate a undo e redo, contro un modello fatto di una fotografia per ogni passo della
cronologia; dopo ogni passo gli indici devono coincidere con una ricostruzione da zero e la mappa
dei tempi con le indicazioni. Il test ha scoperto subito che una transazione può fallire anche alla
fine, quando un'indicazione di tempo spostata finisce dentro un cambio graduale: ora il modello lo
prevede. Cinque bug iniettati (undo nell'ordine sbagliato, indice delle teste di nota lasciato
indietro da una sostituzione, voce dell'indice delle legature lasciata indietro, nessun blocco per
le battute nuove, un evento tolto che dimentica il suo posto) sono stati tutti presi; il secondo solo
dopo che il generatore ha imparato a cambiare le teste di nota di un accordo. I benchmark dell'editor
arrivano con i comandi (seconda parte).

### orchestra-playback

Ricava ciò che si suona da ciò che è scritto: ritornelli e salti srotolati, legature di valore
unite, dinamiche trasformate in velocity e curve, la soglia di tolleranza che decide se uno
spostamento nella DAW è microtiming o una nuova posizione scritta. Il suo risultato è una cache,
mai una seconda copia modificabile.

### orchestra-notation-layout

L'incisione (engraving) incrementale in glifi posizionati: pura geometria, nessun disegno, così
si può testare senza display. Modificare una nota reimpagina solo ciò che ne è toccato.

### orchestra-plugin-host e orchestra-audio-engine

L'host dei plugin espone la nostra interfaccia `PluginHost` sopra CLAP, VST3, LV2 (e AU), con ogni
plugin nel suo processo e uno strato di host scritto da noi per ogni formato (ADR-0011). Il motore audio esegue il grafo in tempo
reale con le regole severe dell'ADR-0007 e offre un backend nullo/offline per i test. Entrambi
possono contenere codice `unsafe`, sempre con un commento `// SAFETY:`.

### orchestra-io, orchestra-ui, orchestra-video, orchestra-app

`io` gestisce il file di progetto versionato con migrazioni (ADR-0010), MIDI e MusicXML. `ui`
disegna le viste e trasforma l'input in comandi (senza toolkit grafico: widget e disegno sono nostri, ADR-0012). `video` è vuoto di
proposito fino all'ultima fase. `app` produce l'eseguibile `orchestra` e si limita a collegare
gli altri.

### orchestra-test-support: contare le allocazioni nei test

Un crate che esiste **solo per test e benchmark** (ADR-0016). Tutti gli altri crate possono
usarlo, ma solo nelle `[dev-dependencies]`: altrimenti `cargo xtask check` fallisce, quindi non
può mai finire nel programma. Non dipende da nulla del workspace, ed è per questo che perfino
`core-time`, in fondo al grafo, può usarlo.

Quello che offre è un **allocatore globale che conta**: `CountingAllocator` passa ogni richiesta
all'allocatore di sistema e, prima di farlo, aumenta di uno un contatore. Lo richiede ADR-0007 ("un
test fallisce se il thread audio alloca") e serve ad ADR-0015 per dimostrare che il tipo razionale
non alloca mai. Un file di test lo registra con una riga e poi misura delle closure:

```rust
// At the top of a file under tests/ (each file there is its own binary):
use std::hint::black_box;

orchestra_test_support::install_counting_allocator!();

#[test]
fn comparison_never_allocates() {
    let a = black_box(3_u64);
    let b = black_box(4_u64);
    // Fails the test if the closure allocates, reallocates or frees on this thread.
    let less = orchestra_test_support::assert_no_allocations(|| a < b);
    assert!(less);
}
```

`count_allocations` restituisce il risultato insieme a un `AllocationStats` con cinque contatori:
allocazioni, riallocazioni, deallocazioni, byte richiesti e byte restituiti (una riallocazione
restituisce il blocco vecchio e ne prende uno nuovo). Per esempio, da `tests/counting.rs`:

```rust
let ((), stats) = count_allocations(|| {
    let v: Vec<u32> = Vec::with_capacity(16);
    drop(black_box(v));
})
.unwrap();
// 1 allocation of 16 × 4 bytes, 1 deallocation.
assert_eq!(stats.bytes_allocated, 64);
```

Tre dettagli che contano:

- **I contatori sono per thread.** I test girano in parallelo: con un solo contatore globale, un
  test che verifica "questo non alloca" fallirebbe a caso ogni volta che un altro test alloca nello
  stesso momento. Ogni thread ha i suoi contatori (un `thread_local!` con inizializzazione `const`
  e senza distruttore, così leggerlo non alloca mai, altrimenti l'allocatore richiamerebbe se
  stesso all'infinito). È anche ciò che servirà al test del thread audio nella Fase 3.
- **Anche liberare memoria conta.** `assert_no_allocations` fallisce per allocazioni,
  riallocazioni *e* deallocazioni: `free` può prendere un lock dentro l'allocatore di sistema,
  quindi il thread audio non deve chiamare nemmeno quella.
- **Niente promozioni a vuoto.** Se un file di test dimentica `install_counting_allocator!()`, ogni
  contatore resta a zero e "non alloca" passerebbe sempre. Perciò `count_allocations` fa prima una
  minuscola allocazione di prova e controlla che il contatore si sia mosso; se no, restituisce
  `NotInstalledError` (e `assert_no_allocations` fa fallire il test). `tests/not_installed.rs` è un
  binario di test senza l'allocatore che verifica proprio questo.

**Quanta memoria trattiene una struttura** (F1.09c). `net_bytes()` è la differenza fra i byte
richiesti e quelli restituiti nella finestra misurata: la crescita della memoria viva, in cui i
buffer temporanei si annullano. Il test di memoria di `core-model` lo usa per misurare i byte per nota
della partitura pesante. Conta ciò che il programma chiede, non gli arrotondamenti dell'allocatore:
un limite inferiore della memoria residente, uguale su ogni piattaforma. Il contatore in più rende
più lenti i benchmark del contatore stesso (istantanee ≈ 1 167 → 1 423 istruzioni, un'allocazione
contata ≈ 7 641 → 8 025): un costo che esiste solo nei test, accettato con il trailer
`Bench-regression: accepted` di ADR-0017.

```rust
let (document, stats) = count_allocations(build_the_heavy_score).unwrap();
// What the document keeps: allocated minus given back, temporary buffers cancel out.
let per_note = stats.net_bytes() / i128::try_from(noteheads(&document)).unwrap();
assert!(per_note <= CEILING_BYTES_PER_NOTE);
```

È il quarto crate in cui è ammesso `unsafe`, perché il trait `GlobalAlloc` è `unsafe` per
definizione; le uniche operazioni unsafe sono le chiamate inoltrate all'allocatore di sistema,
ognuna con il suo commento `// SAFETY:`.

### orchestra-i18n: l'interfaccia in sei lingue

È il primo crate con del codice vero. Possiede ogni testo visibile all'utente, in cataloghi Fluent
per en-US (lingua sorgente), en-GB, it, fr, de ed es (ADR-0008).

#### I cataloghi

Ogni lingua ha una cartella in `crates/i18n/locales/` con un file `main.ftl`. I messaggi si
cercano per chiave, mai per testo inglese; numeri e altri valori sono variabili, e i plurali si
scrivono secondo le regole di ciascuna lingua:

```ftl
status-measures-selected =
    { $count ->
        [one] { $count } measure selected
       *[other] { $count } measures selected
    }
```

Il catalogo en-GB è un file a sé, non una correzione ortografica: lì lo stesso messaggio dice
"bars", perché nell'uso britannico la battuta è *bar*.

I cataloghi sono compilati dentro il programma con `include_str!`. Così non serve leggere file
all'avvio (più veloce, niente da installare accanto all'eseguibile, nessun percorso da sbagliare
su Windows), al prezzo di ricompilare questo crate quando un catalogo cambia, cosa trascurabile.

#### Il glossario

`crates/i18n/glossary.toml` elenca ogni termine musicale e tecnico con la parola corretta in
ciascuna lingua, le fonti consultate e una nota sulla scelta. I termini musicali non si traducono
alla lettera: la quarter note è *crotchet* in inglese britannico, *semiminima* in italiano,
*noire* in francese. I primi 45 termini sono stati cercati nei cataloghi di traduzione
dell'interfaccia di MuseScore (notazione) e di Ardour (DAW), nella guida localizzata di Logic Pro
di Apple e nei collegamenti interlingua di Wikipedia. Dove le fonti non sono d'accordo la voce lo
dice e porta `review = true`, in attesa di un musicista madrelingua. La procedura per i nuovi testi
è in `docs/i18n-process.md`.

#### Il Localizer

L'API pubblica è piccola: un enum chiuso `Locale` e un `Localizer` costruito per una lingua.

```rust
use orchestra_i18n::{FluentArgs, Locale, Localizer};

let it = Localizer::new(Locale::It)?;
let mut args = FluentArgs::new();
args.set("count", 3);
let text = it.format("status-measures-selected", Some(&args))?;
```

`Localizer::new` analizza il catalogo della lingua richiesta e quelli di ripiego (en-GB ripiega su
en-US, e anche ogni altra lingua ripiega su en-US) in una catena di bundle Fluent; `format` usa il
primo bundle della catena che conosce la chiave. I bundle sono della variante `concurrent`, così
un `Localizer` si può condividere tra thread.

`format` è severo: una chiave sconosciuta, un messaggio senza valore o un errore di formattazione
(per esempio un `$count` mancante) sono un `I18nError` tipizzato, non una stringa "alla meglio" con
dentro `{$count}`. Fluent inoltre racchiude ogni segnaposto tra i caratteri Unicode invisibili di
isolamento U+2068/U+2069: li teniamo, perché fanno visualizzare correttamente testo misto da
sinistra a destra e da destra a sinistra (una traccia con un nome in arabo, per esempio).

#### I test

`crates/i18n/tests/parity.rs` analizza tutti e sei i cataloghi con `fluent-syntax` e fallisce se
una chiave manca (o avanza) in una lingua, se un messaggio usa variabili diverse da quelle di
en-US, o se un messaggio non si riesce a formattare in qualche lingua con valori di esempio che
raggiungono ogni categoria di plurale.

`crates/i18n/tests/no_hardcoded_strings.rs` cerca nei crate `ui` e `app` le stringhe letterali che
sembrano testo dell'interfaccia (lettera maiuscola e almeno due parole). È un'euristica per i casi
evidenti; una stringa solo per sviluppatori si può esentare con un commento
`// i18n-ignore: <motivo>` sulla stessa riga. Quando esisteranno i nostri widget (ADR-0012), il
piano è una garanzia più forte, a livello di tipi: le loro API per le etichette accetteranno solo
chiavi di messaggio, non stringhe qualsiasi.

## Automazione

### cargo xtask

`xtask/` è un piccolo programma Rust senza dipendenze, che si lancia con l'alias descritto sopra:

```sh
cargo xtask setup        # use the versioned git hooks (once per clone)
cargo xtask check        # everything CI checks
cargo xtask guides       # build dist/GUIDA.pdf and dist/GUIDE.pdf
cargo xtask screenshots  # not implemented yet (needs the UI layer, ADR-0012)
cargo xtask bench        # instruction-count benchmarks (Linux; compile-only elsewhere)
cargo xtask bench --compare main  # same, plus the regression gate against main (ADR-0017)
```

`check` esegue, in quest'ordine: `cargo fmt --check`, i controlli di parità di documenti e guide,
`cargo clippy -D warnings`, `cargo nextest` (che comprende i test di parità delle traduzioni), i
doctest (che nextest non esegue), `cargo deny check` e infine il **controllo dei crate solo per
test**. Si ferma al primo errore.

L'ultimo passaggio garantisce che `orchestra-test-support` non sia mai una dipendenza normale o di
build di nessun crate. Esegue `cargo tree --invert orchestra-test-support --edges normal,build
--target all`: l'output elenca il crate e tutto ciò che ne dipende attraverso dipendenze normali o
di build (quelle di test sono escluse), su ogni piattaforma. In un workspace corretto l'output è
esattamente una riga, il crate stesso; ogni riga in più è un colpevole, e l'errore lo nomina.

I controlli di parità stanno in `xtask/src/parity.rs`. Per i due documenti per sviluppatori
calcolano la stessa firma strutturale dell'hook Stop: la sequenza dei livelli dei titoli più il
contenuto esatto dei blocchi di codice. Per le guide richiedono anche gli stessi file in
`docs/guide/it/` e `docs/guide/en/`, e in ogni coppia di file Typst gli stessi titoli, gli stessi
`#include` e gli stessi screenshot. Se falliscono, indicano la prima differenza.

`screenshots` fallisce di proposito con una spiegazione invece di non fare niente, così nessuno
lo scambia per un passaggio funzionante.

### Benchmark e test di proprietà

Gli strumenti sono stati scelti in ADR-0016 dopo un confronto (versioni, licenze, numero di
dipendenze e tempi di compilazione sono nell'ADR).

**Benchmark: Gungraun.** `cargo xtask bench` esegue `cargo bench --workspace --bench '*'` (solo i
target `[[bench]]`, non l'harness predefinito e vuoto di ogni libreria). Ogni file di benchmark ha
`harness = false` nel suo `Cargo.toml` e finisce con il `main!` di Gungraun, che passa il lavoro al
programma `gungraun-runner`: questo esegue ogni funzione di benchmark **una volta** sotto Callgrind
di Valgrind e conta le istruzioni eseguite. Il conteggio è lo stesso a ogni esecuzione dello stesso
binario, ed è per questo che ADR-0007 lo preferisce ai tempi in secondi (rumorosi sulle macchine
condivise della CI). Valgrind esiste solo su Linux: su Windows e macOS `cargo xtask bench` si
limita a compilare i benchmark (`--no-run`), così non possono rompersi lì senza che ce ne
accorgiamo.

Cosa installare, una volta sola, su Linux:

```sh
sudo pacman -S valgrind                                       # Arch / CachyOS
cargo install gungraun-runner --version 0.20.0 --locked      # same version as gungraun
cargo xtask bench
```

Il runner deve avere **esattamente** la versione della libreria `gungraun`, ed è per questo che
Cargo.toml la fissa con `=0.20.0`: un innocente `cargo update` non deve spostarla. Quando la si
aggiorna, si reinstalla il runner con la stessa versione e `GUNGRAUN_RUNNER_VERSION` in `ci.yml` si
adegua.

Il primo benchmark (`crates/test-support/benches/counting.rs`) misura il contatore stesso e prova
che tutta la catena funziona. Un'esecuzione stampa, per ogni benchmark, il conteggio e la
differenza rispetto all'esecuzione precedente (conservata in `target/gungraun/`):

```text
counting::counting::snapshots
  Instructions:                                       1167|1167                 (No change)
counting::counting::allocations_of_64_bytes
  Instructions:                                       7641|7641                 (No change)
counting::counting::empty_measurements
  Instructions:                                      10067|10067                (No change)
```

Ogni benchmark esegue la sua operazione su un **lotto di 64** ingressi (i conteggi qui sopra sono
per 64 operazioni: si divide per 64 per averne una). Una singola operazione costa qualche decina
di istruzioni, e un'istruzione in più o in meno può comparire quando cambia codice *non collegato*
nello stesso binario (il compilatore espande e dispone il codice in modo diverso): su un benchmark
da 15 istruzioni sarebbe già il 7%, abbastanza per far scattare il controllo dei peggioramenti
descritto più sotto. I lotti portano i conteggi alle migliaia, dove una deriva del genere è una
frazione di punto percentuale.

I benchmark del tipo razionale (`crates/core-time/benches/rational.rs`, passo F1.03b) coprono ogni
operazione citata da ADR-0015 §9. I loro ingressi sono array `const` calcolati durante la
compilazione (i costruttori sono `const fn`), letti attraverso `black_box` così che il compilatore
non possa calcolare in anticipo anche i risultati. Un estratto di un'esecuzione:

```text
rational::rational::add_same_denominator
  Instructions:                                      12686|12686                (No change)
rational::rational::add_wide_intermediate
  Instructions:                                      77147|77147                (No change)
rational::rational::cmp_different_denominators
  Instructions:                                       1410|1410                 (No change)
```

I primi numeri, per operazione (conteggio ÷ 64), sulla macchina di sviluppo:

| Benchmark | Istruzioni per operazione |
|---|---|
| `add_same_denominator` | ~198 |
| `add_different_denominators` | ~233 |
| `add_wide_intermediate` (intermedi oltre i 64 bit) | ~1205 |
| `sub_different_denominators` | ~232 |
| `mul` | ~196 |
| `div` | ~207 |
| `cmp_same_denominator` | ~18 |
| `cmp_different_denominators` | ~22 |
| `new_with_reduction` | ~99 |
| `overflow_errors` (una somma + un prodotto che vanno in overflow) | ~910 |

Mostrano tre cose. I confronti costano un ordine di grandezza meno dell'aritmetica, come previsto
(niente MCD). La scorciatoia della somma per denominatori uguali fa risparmiare meno del previsto,
perché la riduzione finale (un MCD) domina il costo di entrambi i percorsi. E il MCD a 128 bit
costa circa cinque volte quello a 64 bit, il che conferma la scorciatoia di ADR-0015 §9. Nulla di
questo richiede interventi oggi: questi numeri sono il riferimento con cui si giudicheranno le
modifiche future, e le possibili ottimizzazioni sono annotate nella ROADMAP.

Il caso "oltre i 64 bit" richiede ingressi costruiti con cura: `x = a/(G·P)` e `y = c/(G·Q)` con
`G = 2³⁰` e due primi `P`, `Q` appena sotto 2²⁰, così il denominatore comune è circa 2⁷⁰. `c` è
scelto (con un inverso modulare calcolato durante la compilazione) in modo che il numeratore della
somma sia un multiplo di `G`, e il risultato ridotto abbia un denominatore di circa 2⁴⁰, che ci
sta. Tutti i 64 ingressi sono stati verificati con uno script indipendente prima di usarli.

**Il controllo dei peggioramenti (ADR-0017).** `cargo xtask bench --compare <rev>` misura due
versioni del codice nella stessa esecuzione: prima la **base comune** (merge base) di `HEAD` e
`<rev>` (per un branch e `main`: il commit di `main` da cui il branch è partito), poi l'albero di
lavoro, e fallisce se un conteggio di istruzioni è cresciuto più del **5%**. La logica sta in
`xtask/src/bench.rs`:

- il commit di base viene esportato con `git archive <commit> | tar -x` in `target/bench-base/`
  (niente `git worktree`, che scriverebbe in `.git/`, in sola lettura nella sandbox) e compilato
  in una cartella sua, `target/bench-base-target/`. Condividere il solito `target/` farebbe
  compilare le dipendenze una volta sola, ma è sbagliato: cargo dà agli artefatti dei due alberi
  gli stessi nomi (ricavati da percorsi relativi alla radice del workspace), e nella prima
  esecuzione della CI di F1.03b il suo controllo di aggiornamento, basato sulle date dei file, ha
  eseguito i binari dei benchmark del commit di base al posto di quelli dell'albero attuale;
- i suoi numeri vengono salvati come baseline Gungraun `base` in una cartella
  `target/gungraun-compare/` nuova (`GUNGRAUN_SAVE_BASELINE`), poi l'albero di lavoro gira con
  `GUNGRAUN_BASELINE=base` e `GUNGRAUN_CALLGRIND_LIMITS=ir=5%`;
- un benchmark che la base non ha passa (non c'è nulla con cui confrontarlo), e così ogni
  miglioramento;
- se il messaggio del commit `HEAD` ha il trailer `Bench-regression: accepted` (con il motivo nel
  messaggio), il confronto viene comunque stampato ma senza limite.

Perché la base comune e non l'ultimo commit di `main`: un branch va giudicato sulle sue modifiche;
se `main` è andato avanti, contiene il lavoro di altri. Perché misurare entrambi nella stessa
esecuzione: i conteggi sono deterministici per un dato binario, ma dipendono da compilatore,
Valgrind e macchina, quindi solo due misure prese insieme sono confrontabili. Per verificare che il
controllo funzioni, durante lo sviluppo è stato introdotto di proposito un peggioramento (il
benchmark degli snapshot che ne prende due invece di uno):

```text
counting::counting::snapshot
  Instructions:                                         37|15                   (+146.667%) [+2.46667x]
Performance has regressed: Instructions (15 -> 37) regressed by +146.667% (>+5.00000%)
xtask: `cargo bench --workspace --locked --bench *` failed (exit status: 3)
If an instruction count grew by more than the limit (ir=5%) on purpose, explain why in the commit message and add the trailer `Bench-regression: accepted` (ADR-0017).
```

Misurare l'albero di lavoro, e non solo `HEAD`, permette di provare il controllo prima di
committare: `cargo xtask bench --compare HEAD` confronta le modifiche non ancora committate con
l'ultimo commit. Nella CI il controllo gira su ogni branch tranne `main`, e il job
`bench (instruction counts)` è un controllo obbligatorio della regola di `main`, quindi un branch
che peggiora non si può unire senza il trailer. Il prezzo è piccolo: nelle esecuzioni della CI di
F1.03b il job ha impiegato circa 49 s invece di 35 s.

Un benchmark il cui significato cambia prende un nome nuovo. In F1.03b i tre benchmark del
contatore sono diventati lotti e sono stati rinominati (`snapshot` → `snapshots`, e così via):
Gungraun associa le baseline al nome, quindi con i vecchi nomi il confronto avrebbe segnalato un
+7000% privo di senso.

Due note pratiche. Il `#[library_benchmark]` di Gungraun rifiuta i doc comment sulle funzioni di
benchmark (sono attributi `#[doc]`), quindi quelle funzioni hanno normali commenti `//` e il file
disattiva i lint sulla documentazione per il codice generato dalle macro. Inoltre Gungraun svuota
l'ambiente di ciò che esegue, quindi Valgrind scriverebbe i suoi file temporanei in `/tmp`: dove
`/tmp` non è scrivibile (la sandbox di Claude Code) xtask inoltra `TMPDIR` tramite `GUNGRAUN_ENVS`.

**Test di proprietà: proptest.** Un test di proprietà enuncia una regola che deve valere per
*ogni* input (per esempio "allocare n buffer conta n allocazioni e la loro dimensione totale in
byte") e proptest la verifica su centinaia di input casuali. Quando trova un errore **riduce**
l'input al caso più semplice che fallisce ancora, e ne salva il seme in un file
`proptest-regressions/` accanto al test: quei file si committano, così il caso viene rieseguito
per sempre. Lo usiamo senza le sue funzionalità predefinite (`fork` e `timeout` porterebbero altri
crate che non ci servono).

### Gli hook di Claude Code

Il file `.claude/settings.json` del progetto configura tre hook (e il sandbox, vedi l'ultima
sezione):

- **PostToolUse su Edit/Write**: quando Claude scrive un file `.rs`, `rustfmt` lo formatta subito.
  L'hook legge il percorso del file dal JSON che riceve sullo standard input:

```sh
FILE=$(jq -r '.tool_input.file_path // empty'); case "$FILE" in *.rs) rustfmt "$FILE" 2>/dev/null ;; esac; exit 0
```

- **UserPromptSubmit**: quando inizia un nuovo turno, `scripts/turn-start.sh` fotografa il
  working tree e salva l'identificativo dell'albero git in `.git/claude-turn-start-<sessione>`.
  La foto la scatta `scripts/worktree-snapshot.sh` con una copia temporanea dell'indice
  (`git add -A` dentro la copia, poi `git write-tree`), così HEAD, branch, indice vero e stash non
  vengono mai toccati.
- **Stop**: quando Claude sta per chiudere il turno, `scripts/check-docs-updated.sh` scatta una
  nuova foto e la confronta con quella di inizio turno, così vede esattamente i file cambiati *in
  questo turno*, compresi quelli committati durante il turno. Se è cambiato un sorgente (`.rs`,
  `.toml`, `.ftl` sotto `crates/` o `xtask/`; il `Cargo.toml` della radice, `deny.toml`,
  `clippy.toml`, `rustfmt.toml`, `.cargo/config.toml`; i workflow della CI, `scripts/*.sh` e
  `.githooks/`, cioè tutto ciò che questi documenti spiegano, tranne `Cargo.lock` e `spikes/`) e
  `DOCUMENTAZIONE.md` o `DOCUMENTATION.md` non sono
  stati toccati, oppure hanno strutture diverse, risponde `{"decision": "block", "reason": …}` e
  Claude deve sistemare prima di finire. Un indicatore `stop_hook_active` evita un ciclo infinito.
  Senza foto ripiega su tutte le modifiche del branch rispetto a `origin/main`: era il progetto
  originale, e le prove della Fase 0 ne hanno mostrato il difetto: una volta toccati i documenti
  in un punto qualsiasi del branch, non bloccava più nulla.

### L'hook commit-msg

`.githooks/commit-msg` rifiuta ogni commit senza un trailer `User-visible: yes` o
`User-visible: no`; con `yes` il commit deve cambiare anche i sorgenti di entrambe le guide
(`docs/guide/it/` e `docs/guide/en/`). I commit di merge, revert e fixup sono esentati.

Git riconosce i trailer solo nell'**ultimo paragrafo** del messaggio, quindi `User-visible` deve
stare nello stesso blocco degli altri trailer:

```text
Add i18n infrastructure (step 9)

Longer explanation of the change.

User-visible: no
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

Git non versiona `.git/config`, quindi ogni clone deve eseguire `cargo xtask setup` una volta per
attivare l'hook.

### Integrazione continua

`.github/workflows/ci.yml` parte a ogni push su qualunque branch, a richiesta (`workflow_dispatch`)
e **una volta alla settimana su `main`**, il lunedì alle 06:00 UTC:

```yaml
schedule:
  - cron: "0 6 * * 1"
```

Perché un run settimanale se il codice non è cambiato? Perché cambia il mondo intorno: GitHub
aggiorna le immagini dei runner, e il database degli avvisi di sicurezza RustSec che `cargo deny`
scarica si arricchisce di nuove voci. Senza questo run una piattaforma potrebbe diventare rossa in
silenzio e ce ne accorgeremmo solo alla pull request successiva. Valgono due regole di GitHub:
l'email di fallimento arriva a chi ha modificato per ultimo la riga `cron`, e i run programmati
vengono disattivati dopo 60 giorni senza attività nel repository (GitHub avvisa prima via email).

I job:

- **check**, su `ubuntu-latest`, `windows-latest` e `macos-latest`: installa Rust 1.99.1 (la stessa
  versione di `rust-version`), ripristina la cache di compilazione, installa `cargo-nextest` e
  `cargo-deny` già compilati ed esegue `cargo xtask check`. Le tre piattaforme girano in modo
  indipendente (`fail-fast: false`), così un errore su Windows non nasconde quello su macOS;
- **guides**, su Linux: installa Typst 0.15.1, esegue `cargo xtask guides` e carica `GUIDA.pdf` e
  `GUIDE.pdf` come artefatto `user-guides`, conservato per 30 giorni;
- **bench**, su Linux: installa Valgrind (`apt-get`) e il `gungraun-runner` già compilato della
  versione fissata, esegue `cargo xtask bench`, scrive i numeri nella pagina di riepilogo
  dell'esecuzione e li carica (con un `summary.json` per benchmark) come artefatto `benchmarks`. Su
  ogni branch tranne `main` esegue invece `cargo xtask bench --compare origin/main`, il controllo
  dei peggioramenti di ADR-0017 (con tutta la cronologia git, `fetch-depth: 0`, perché serve la
  base comune); la pagina di riepilogo mostra allora solo il confronto. Su Windows e macOS anche il job **check** esegue `cargo xtask bench`, che lì si
  limita a compilare i benchmark.

Usiamo solo i runner standard ospitati da GitHub: sono gratuiti per i repository pubblici, mentre
quelli più grandi si pagano anche lì. Ogni action di terzi è fissata con l'hash completo del commit,
con il tag in un commento, perché un tag può essere spostato da chi controlla il repository
dell'action:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
```

Il workflow ha solo accesso in lettura al repository, e un push più recente sullo stesso branch
annulla l'esecuzione precedente. Il nome dell'evento fa parte del gruppo di annullamento
(`ci-${{ github.ref }}-${{ github.event_name }}`), così un merge in `main` e il run settimanale non
si annullano mai a vicenda.

## Le guide per l'utente

Le guide per l'utente sono scritte in Typst. `docs/guide/template.typ` definisce tutto l'aspetto
una volta sola: una copertina blu inchiostro con il logo di Orchestra in bianco, intestazioni
correnti, aperture di capitolo con un grande numero color ottone, blocchi di
codice, riquadri "nota"/"suggerimento"/"attenzione" e una funzione `screenshot` che incornicia
un'immagine e aggiunge la didascalia. Le due guide si limitano a importarlo:

```typ
#import "/docs/guide/template.typ": guide

#show: guide.with(
  title: "Orchestra",
  // subtitle, edition, lang: set by each guide in its own language
)

#include "chapters/01-introduction.typ"
```

`docs/guide/it/` e `docs/guide/en/` contengono gli stessi file con gli stessi capitoli. Gli
screenshot saranno generati senza display dall'applicazione, sempre in inglese, e condivisi dalle
due guide; finché il nostro strato di interfaccia non esiste, i capitoli usano `screenshot-placeholder`, che
ha la stessa cornice e la stessa didascalia di uno screenshot vero. Le parole di Typst ("Figura",
"Indice") seguono l'impostazione `lang`, quindi il template non contiene testo in nessuna lingua.

`cargo xtask guides` compila entrambi i PDF in `dist/` con `--ignore-system-fonts`, così vengono
usati solo i font di `assets/fonts/` e il risultato è identico su ogni macchina. I PDF non si
committano mai: la CI li pubblica come artefatti.

### Il logo e i temi

`assets/logo/orchestra-logo.svg` è l'unico logo di Orchestra (ADR-0014): la sua forma non cambia
mai, solo il colore. Nel disegno originale la scritta era ancora testo nel font Alte Haas Grotesk,
che su qualunque computer senza quel font verrebbe sostituito da un altro; per questo la versione
nel repository è stata ricavata dall'esportazione PDF fatta dall'autore con `pdftocairo -svg`, che
trasforma i caratteri in tracciati: resa a 1000 × 1000 px, differisce dall'originale in 0 pixel.
Tutto il file è riempito con un solo colore, `#000000`, e ricolorarlo significa sostituire quella
stringa prima del disegno. Il template delle guide fa esattamente questo:

```typ
#let logo(colour, width: 100%) = {
  let svg = read("/assets/logo/orchestra-logo.svg").replace("#000000", colour.to-hex())
  image(bytes(svg), format: "svg", width: width)
}
```

Lo stesso ADR stabilisce che il programma ha un tema chiaro e uno scuro fin dal primo widget:
widget e renderer non usano mai colori scritti nel codice, solo i token semantici del tema attivo;
in modo predefinito si segue il sistema operativo, e la partitura può seguire il tema oppure
restare su carta bianca.

A differenza del codice, il logo **non** è sotto GPL: tutti i diritti riservati (decisione del
proprietario). Lo dice il README del repository, e il logo sta solo in `assets/logo/`, così una
distribuzione che richiede grafica interamente libera può sostituirlo senza toccare il codice.

### I font inclusi

`assets/fonts/` contiene quattro famiglie, tutte con licenza SIL Open Font License 1.1 (verificata
tramite l'API di GitHub e annotata con versioni e fonti in `assets/fonts/README.md`): Source Serif
4 per il testo, Inter per i titoli, JetBrains Mono per il codice e Bravura (con Bravura Text), il
font di riferimento dello standard SMuFL per i font musicali, che userà anche il motore di
notazione.

## Prototipi di valutazione

`spikes/` è un workspace Cargo separato con prototipi a tempo limitato: quelli della Fase 0, Passo
13 e il prototipo della memorizzazione del modello del passo F1.06. È escluso dal workspace
principale (`exclude = ["spikes"]` nel `Cargo.toml` della radice), non viene mai compilato da
`cargo xtask check` né dalla CI e non viene mai distribuito: il suo compito era misurare e decidere.
Le decisioni e tutti i numeri sono negli ADR-0011, ADR-0012 e ADR-0022 (con
`spikes/model/ANALYSIS.md`); questa sezione spiega come rieseguire i prototipi.

### Il prototipo dell'interfaccia grafica

`spikes/scene` è la scena di prova comune: esattamente 2000 glifi SMuFL di Bravura su 10 righi,
una barra dei menu, una frase per ogni lingua dell'interfaccia in Inter e un percorso
deterministico di 120 viste che scorrono e cambiano zoom. Ogni candidato (`gui-own`, `gui-iced`,
`gui-egui`, `gui-slint`) la disegna in modalità `headless` (PNG senza alcuna finestra) e `bench`
(tempi dei fotogrammi sulla CPU):

```sh
cd spikes
cargo build --release
./target/release/gui-own headless out/own.png
./target/release/gui-own bench
ICED_TEST_BACKEND=tiny-skia ICED_GLYPHS=paths ./target/release/gui-iced bench
```

Due lezioni vanno conservate. Primo, i glifi musicali vanno disegnati come contorni messi in
cache, mai come "testo" ricomposto a ogni fotogramma: in iced questo da solo ha portato i
fotogrammi da 243 ms a 29 ms. Secondo, ogni toolkit aveva comunque bisogno di un nostro renderer
per la partitura (egui non sa riempire forme concave, Slint non ha una tela), ed è uno dei motivi
per cui l'ADR-0012 ha scelto di non usare alcun toolkit. `gui-own/src/render.rs` è il renderer
tiny-skia su cui si basa la decisione: i contorni dei glifi vengono letti una volta sola con
ttf-parser, messi in cache per carattere, e al momento del disegno vengono solo trasformati e
riempiti.

### Il prototipo dell'hosting dei plugin

`spikes/plugin-host` è un unico eseguibile con tre ruoli: `scan`, `host` e `bridge`. L'host avvia
un processo bridge per ogni plugin, condivide con lui un file mappato in memoria per l'audio e fa
partire ogni blocco con un byte sullo stdin del bridge; il bridge risponde con un byte su una
copia privata del suo stdout (i plugin stampano su stdout, quindi il descrittore 1 viene deviato
su stderr). L'host misura l'andata e ritorno, uccide il bridge a metà del flusso, verifica di
sopravvivere e lo riavvia. Una modalità `gui` apre l'editor del plugin in una finestra X11 separata
che appartiene al bridge.

```sh
cd spikes
./target/release/plugin-host scan
./target/release/plugin-host host lv2 "Compressor Stereo"
./target/release/plugin-host gui vst3 "ZL Equalizer 2" 60
```

Il prototipo usava truce-rack dentro il bridge; l'ADR-0011 ha deciso invece di scrivere un nostro
strato di host sugli stessi binding di basso livello, mantenendo l'architettura a bridge che le
misure hanno confermato.

### Il prototipo della memorizzazione del modello

`spikes/model` (passo F1.06) confronta tre modi di memorizzare gli eventi di una voce, con i veri
tipi di `core-time`: P1, un vettore per voce ordinato per posizione assoluta; P2, un blocco per
battuta con scostamenti relativi alla battuta, battute indicate dal numero; P3, gli stessi blocchi
indicizzati da ID di battuta stabili dietro `Arc`. `src/lib.rs` genera due partiture deterministiche
(100 battute × 10 righi, 8 690 note; 1 000 battute × 30 righi, 255 776 note) e le modifiche della
mappa dei metri, `tests/agree.rs` controlla che i tre contengano la stessa musica dopo ogni
operazione, `benches/model.rs` conta le istruzioni e `src/bin/report.rs` misura memoria,
allocazioni e tempi reali:

```sh
cd spikes
cargo test --release -p model
GUNGRAUN_ENVS=TMPDIR cargo bench -p model
cargo run --release -p model --bin report
```

Il workspace `spikes` toglie i simboli dai binari release, quindi il suo `[profile.bench]` li
mantiene: Callgrind trova la funzione misurata per nome, e senza simboli ogni conteggio è 0. I numeri
e la scelta di P3 sono in `spikes/model/ANALYSIS.md` e in ADR-0022.

## Lezioni della Fase 0

Cose non ovvie che fanno perdere tempo la prima volta che le si incontra:

- **Il sandbox di Claude Code** filtra il traffico di rete con un proxy HTTP. SSH (`git push` sulla
  porta 22) e il token di `gh` nel portachiavi di sistema (raggiunto via D-Bus) lì dentro non
  funzionano, quindi `git push`, `git fetch`, `git pull`, `git ls-remote` e `gh` sono elencati in
  `sandbox.excludedCommands`. Vale solo per un comando eseguito *da solo*: `cd … && git push` resta
  nel sandbox e fallisce.
- Nel sandbox `~/.cargo`, `~/.rustup` e `.git/config` sono in sola lettura. Le nuove dipendenze si
  scaricano con `cargo fetch` (eseguito da solo, perché è in `sandbox.excludedCommands`; dopo,
  le compilazioni funzionano con `--offline`), gli aggiornamenti della toolchain li fa il proprietario con
  `rustup update stable`, e `cargo xtask setup` si esegue fuori dal sandbox.
- La CI mette in cache `target/` con rust-cache, che elimina i file che non riconosce come
  prodotti della compilazione: il database di cargo-deny che teniamo in `target/advisory-dbs` è
  tornato vuoto alla seconda esecuzione e `cargo deny` è fallito con "failed to load any
  advisories". Il workflow ora cancella quella cartella prima di `cargo xtask check`.
- `gh` dentro una pipeline (`gh … | grep`) gira nel sandbox, senza il suo token: GitHub allora
  risponde come a un utente anonimo (60 richieste l'ora, niente accesso ai log dei job). Esegui
  `gh` da solo.
- I trailer dei commit devono stare nell'ultimo paragrafo del messaggio (vedi l'hook commit-msg).
- `rustfmt` su un singolo file fallisce se quel file dichiara un `mod` il cui file non esiste
  ancora, quindi l'hook di formattazione lo salta in silenzio: scrivi i file dei moduli prima
  della riga `mod` che li usa, oppure esegui `cargo fmt --all` dopo (`cargo xtask check` se ne
  accorge comunque).
- I collegamenti interlingua di Wikipedia sono un buon punto di partenza per la terminologia, ma
  possono trarre in inganno: l'inglese "Bar (music)" punta allo spagnolo "Barra de compás", che è
  la *stanghetta*; la battuta è *compás*. I cataloghi di traduzione dei programmi (MuseScore,
  Ardour) sono una prova migliore di ciò che gli utenti leggono davvero.
- In Typst un solo `*` è grassetto e `_` è corsivo; `**testo**` produce un grassetto vuoto e un
  avviso.
