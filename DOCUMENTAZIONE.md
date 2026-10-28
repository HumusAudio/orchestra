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
├── docs/                   ADRs, architecture, i18n process, user guide sources
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

Il gruppo **pedantic** è attivo come proposta, con tre lint disattivati (`module_name_repetitions`,
`must_use_candidate`, `similar_names`) e un commento in `Cargo.toml` che spiega ogni scelta; la
selezione è ancora una decisione aperta in `ROADMAP.md`. I lint sulle conversioni numeriche
(cast) restano attivi di proposito: il codice audio e del tempo converte di continuo tra numero di
campioni, fotogrammi e numeri decimali, e ogni conversione con perdita deve essere visibile.

`unsafe_code` non è nella tabella del workspace. La radice di ogni crate comincia con
`#![forbid(unsafe_code)]`, tranne `audio-engine`, `plugin-host` e `io`, i tre crate che parleranno
con API in C. Un `forbid` a livello di workspace non si potrebbe allentare solo per quei tre, e
l'attributo rende l'eccezione visibile proprio in cima al crate.

`xtask` non eredita questa tabella: è uno strumento per sviluppatori che stampa messaggi ed esce
con codici di errore, quindi dichiara la sua (documentazione obbligatoria, pedantic attivo,
`unsafe` vietato).

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

Nella Fase 0 ogni crate contiene solo la sua documentazione di crate (scopo, responsabilità, cosa
*non* deve fare, dipendenze ammesse), tranne `orchestra-i18n`, che funziona già. Le regole sulle
dipendenze sono riassunte in `docs/ARCHITECTURE.md`; qui c'è a cosa serve ciascun crate.

### orchestra-core-time

Il tempo musicale esatto: posizioni e durate come frazioni di semibreve, l'unica mappa dei tempi,
le conversioni in secondi, campioni e fotogrammi video (ADR-0005). Le funzioni di conversione
verranno chiamate dal thread audio, quindi non devono allocare memoria. Nessuna dipendenza
all'interno del workspace.

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
plugin nel suo processo (ADR-0011, ancora Proposed). Il motore audio esegue il grafo in tempo
reale con le regole severe dell'ADR-0007 e offre un backend nullo/offline per i test. Entrambi
possono contenere codice `unsafe`, sempre con un commento `// SAFETY:`.

### orchestra-io, orchestra-ui, orchestra-video, orchestra-app

`io` gestisce il file di progetto versionato con migrazioni (ADR-0010), MIDI e MusicXML. `ui`
disegna le viste e trasforma l'input in comandi (toolkit nell'ADR-0012). `video` è vuoto di
proposito fino all'ultima fase. `app` produce l'eseguibile `orchestra` e si limita a collegare
gli altri.

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
`// i18n-ignore: <motivo>` sulla stessa riga.

## Automazione

### cargo xtask

`xtask/` è un piccolo programma Rust senza dipendenze, che si lancia con l'alias descritto sopra:

```sh
cargo xtask setup        # use the versioned git hooks (once per clone)
cargo xtask check        # everything CI checks
cargo xtask guides       # build dist/GUIDA.pdf and dist/GUIDE.pdf
cargo xtask screenshots  # not implemented yet (needs the GUI toolkit)
cargo xtask bench        # not implemented yet (no hot paths yet)
```

`check` esegue, in quest'ordine: `cargo fmt --check`, i controlli di parità di documenti e guide,
`cargo clippy -D warnings`, `cargo nextest` (che comprende i test di parità delle traduzioni), i
doctest (che nextest non esegue) e `cargo deny check`. Si ferma al primo errore.

I controlli di parità stanno in `xtask/src/parity.rs`. Per i due documenti per sviluppatori
calcolano la stessa firma strutturale dell'hook Stop: la sequenza dei livelli dei titoli più il
contenuto esatto dei blocchi di codice. Per le guide richiedono anche gli stessi file in
`docs/guide/it/` e `docs/guide/en/`, e in ogni coppia di file Typst gli stessi titoli, gli stessi
`#include` e gli stessi screenshot. Se falliscono, indicano la prima differenza.

`screenshots` e `bench` falliscono di proposito con una spiegazione invece di non fare niente, così
nessuno li scambia per passaggi funzionanti.

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
  `.toml`, `.ftl` sotto `crates/` o `xtask/`) e `DOCUMENTAZIONE.md` o `DOCUMENTATION.md` non sono
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

`.github/workflows/ci.yml` parte a ogni push su qualunque branch:

- **check**, su `ubuntu-latest`, `windows-latest` e `macos-latest`: installa Rust 1.99.1 (la stessa
  versione di `rust-version`), ripristina la cache di compilazione, installa `cargo-nextest` e
  `cargo-deny` già compilati ed esegue `cargo xtask check`. Le tre piattaforme girano in modo
  indipendente (`fail-fast: false`), così un errore su Windows non nasconde quello su macOS;
- **guides**, su Linux: installa Typst 0.15.1, esegue `cargo xtask guides` e carica `GUIDA.pdf` e
  `GUIDE.pdf` come artefatto `user-guides`, conservato per 30 giorni.

Usiamo solo i runner standard ospitati da GitHub: sono gratuiti per i repository pubblici, mentre
quelli più grandi si pagano anche lì. Ogni action di terzi è fissata con l'hash completo del commit,
con il tag in un commento, perché un tag può essere spostato da chi controlla il repository
dell'action:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
```

Il workflow ha solo accesso in lettura al repository, e un push più recente sullo stesso branch
annulla l'esecuzione precedente.

## Le guide per l'utente

Le guide per l'utente sono scritte in Typst. `docs/guide/template.typ` definisce tutto l'aspetto
una volta sola: una copertina blu inchiostro con un pentagramma disegnato con il font musicale
Bravura, intestazioni correnti, aperture di capitolo con un grande numero color ottone, blocchi di
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
due guide; finché il toolkit grafico non è scelto, i capitoli usano `screenshot-placeholder`, che
ha la stessa cornice e la stessa didascalia di uno screenshot vero. Le parole di Typst ("Figura",
"Indice") seguono l'impostazione `lang`, quindi il template non contiene testo in nessuna lingua.

`cargo xtask guides` compila entrambi i PDF in `dist/` con `--ignore-system-fonts`, così vengono
usati solo i font di `assets/fonts/` e il risultato è identico su ogni macchina. I PDF non si
committano mai: la CI li pubblica come artefatti.

### I font inclusi

`assets/fonts/` contiene quattro famiglie, tutte con licenza SIL Open Font License 1.1 (verificata
tramite l'API di GitHub e annotata con versioni e fonti in `assets/fonts/README.md`): Source Serif
4 per il testo, Inter per i titoli, JetBrains Mono per il codice e Bravura (con Bravura Text), il
font di riferimento dello standard SMuFL per i font musicali, che userà anche il motore di
notazione.

## Lezioni della Fase 0

Cose non ovvie che fanno perdere tempo la prima volta che le si incontra:

- **Il sandbox di Claude Code** filtra il traffico di rete con un proxy HTTP. SSH (`git push` sulla
  porta 22) e il token di `gh` nel portachiavi di sistema (raggiunto via D-Bus) lì dentro non
  funzionano, quindi `git push`, `git fetch`, `git pull`, `git ls-remote` e `gh` sono elencati in
  `sandbox.excludedCommands`. Vale solo per un comando eseguito *da solo*: `cd … && git push` resta
  nel sandbox e fallisce.
- Nel sandbox `~/.cargo`, `~/.rustup` e `.git/config` sono in sola lettura. Le nuove dipendenze si
  scaricano con un solo `cargo fetch` fuori dal sandbox (poi le compilazioni funzionano con
  `--offline`), gli aggiornamenti della toolchain li fa il proprietario con
  `rustup update stable`, e `cargo xtask setup` si esegue fuori dal sandbox.
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
