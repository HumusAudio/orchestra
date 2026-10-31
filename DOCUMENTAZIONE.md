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
tipo razionale, i tipi posizione e durata e la mappa delle misure. Le regole sulle
dipendenze sono riassunte in `docs/ARCHITECTURE.md`; qui c'è a cosa serve ciascun crate.

### orchestra-core-time

Il tempo musicale esatto: posizioni e durate come frazioni di semibreve, l'unica mappa dei tempi,
le conversioni in secondi, campioni e fotogrammi video (ADR-0005). Le funzioni di conversione
verranno chiamate dal thread audio, quindi non devono allocare memoria. Nessuna dipendenza
all'interno del workspace (in `[dev-dependencies]` solo `orchestra-test-support` e `proptest`, per
i test). Per ora contiene il tipo razionale (passo F1.03), i tipi posizione e durata (F1.04) e la
mappa delle misure con le suddivisioni dei movimenti (F1.04b); seguirà la mappa dei tempi con le
conversioni (F1.05).

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

### orchestra-core-model

L'unico documento di progetto (ADR-0003): tracce, note con i loro due strati (intento scritto e
scostamenti di esecuzione, ADR-0004), la forma del brano, i comandi e l'unica cronologia di
annulla/ripristina. Non sa nulla di viste, impaginazione, audio o file.

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

`count_allocations` restituisce il risultato insieme a un `AllocationStats` con quattro contatori:
allocazioni, riallocazioni, deallocazioni e byte richiesti. Per esempio, da `tests/counting.rs`:

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

`spikes/` è un workspace Cargo separato con i prototipi a tempo limitato della Fase 0, Passo 13. È
escluso dal workspace principale (`exclude = ["spikes"]` nel `Cargo.toml` della radice), non viene
mai compilato da `cargo xtask check` né dalla CI e non viene mai distribuito: il suo compito era
misurare e decidere. Le decisioni e tutti i numeri sono negli ADR-0011 e ADR-0012; questa sezione
spiega come rieseguire i prototipi.

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
