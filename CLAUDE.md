# Progetto: Orchestra

Software unico che unisce notazione musicale, DAW e un editor video semplice. Open source (GPL-3.0-or-later), scritto in Rust. **Linux first**; Windows e macOS arrivano dopo, ma nessuna scelta fatta oggi deve ostacolarli.

Questo file contiene solo le REGOLE permanenti. Le decisioni di progetto stanno in `docs/adr/`, lo stato dei lavori in `ROADMAP.md`. Non duplicare qui il loro contenuto.


## 0. Inizio di OGNI sessione — fai sempre questo per primo

1. Leggi `ROADMAP.md` (stato attuale e prossimi passi) e l'indice `docs/adr/README.md`. Leggi per intero solo gli ADR rilevanti per il compito di oggi.

2. Esegui `uname -a`, `git status`, `git branch --show-current`, `git log --oneline -10`.

3. **Mai contraddire un ADR con stato "Accepted".** Se pensi che una decisione sia sbagliata, FERMATI, spiegami il problema e proponi un nuovo ADR che la sostituisca. Procedi solo dopo il mio ok. Questo è ciò che garantisce la coerenza tra sessioni diverse.

4. A fine sessione aggiorna `ROADMAP.md` (cosa è stato fatto, cosa resta, decisioni aperte).


## 1. Principio architetturale non negoziabile

Esiste UN SOLO documento di progetto (la "fonte di verità"). Notazione e DAW sono due *proiezioni* dello stesso modello: non due copie collegate, ma gli stessi identici dati letti e modificati in due modi. Il video è un terzo tipo di traccia nella stessa linea temporale.

- Mai creare dati duplicati tra le viste; mai sincronizzazioni "a mano" tra strutture parallele.

- Ogni modifica, da qualunque vista, è un **comando** applicato al modello (undo/redo unico).

- Ogni nota ha due strati: **intento scritto** (altezza con grafia, durata razionale, voce, articolazioni, dinamica) ed **esecuzione** (derivata dall'intento + scostamenti espliciti).

- Il tempo musicale è in **frazioni esatte**, mai in numeri decimali. Una sola mappa dei tempi.

- La vista DAW mostra SEMPRE la timeline "srotolata" come viene suonata (ritornelli e salti espansi), restando collegata alle posizioni dello spartito.

- Spostamenti di una nota dal lato DAW entro una **soglia di tolleranza** restano scostamenti di esecuzione (microtiming) e lasciano invariato lo spartito; oltre la soglia cambia la posizione scritta. La soglia è configurabile.

- Dettagli: ADR-0003, ADR-0004, ADR-0005.

## 2. Struttura del workspace

Cargo workspace; un crate per responsabilità, dipendenze solo "verso il basso". `core-time` · `core-model` · `playback` (da intento scritto a esecuzione) · `notation-layout` · `audio-engine` · `plugin-host` · `io` (MIDI, MusicXML, file di progetto versionato) · `i18n` · `ui` · `app` · `xtask` (automazione). `video` arriva per ultimo. L'elenco aggiornato è in `docs/ARCHITECTURE.md`. Codice specifico di piattaforma SOLO in moduli `platform/` dietro trait.

## 3. Comandi (tutto passa da `cargo xtask`)

- `cargo xtask check` — fmt --check, clippy, test, deny, parità documenti, parità traduzioni

- `cargo xtask bench` — benchmark dei percorsi critici (vedi §5)

- `cargo xtask screenshots` — genera gli screenshot dell'interfaccia in modalità headless

- `cargo xtask guides` — genera `GUIDA.pdf` e `GUIDE.pdf` in `dist/` (NON committati)

- `cargo xtask setup` — configura `core.hooksPath` su `.githooks`

I test NON devono mai richiedere un dispositivo audio né un display: usa backend audio "null/offline" e il rendering headless.

## 4. Stile del codice — commenti

- **Tutti i commenti in inglese (en-US)**, anche negli script. Mai in italiano nel codice.

- Commenta OGNI passaggio e ogni particolare, senza paura di essere prolisso. Vietati commenti stringati o criptici. Spiega: cosa fa, **perché** è fatto così, quali invarianti si assumono, unità di misura, casi limite, complessità computazionale, alternative scartate.

- Ogni elemento (anche privato) ha un doc comment: `missing_docs` è un errore del compilatore.

- Ogni `unsafe` ha un commento `// SAFETY:` che dimostra perché è corretto (`undocumented_unsafe_blocks` è un errore). `unsafe` ammesso solo nei crate designati (FFI/audio).

- Niente `unwrap`/`expect`/`panic!` in codice di libreria; errori tipizzati e propagati.

- `rustfmt` è applicato automaticamente da un hook dopo ogni Edit/Write.

Esempio del livello di dettaglio richiesto:

```
/// Converts a musical position into an absolute sample index.
///
/// # Why rational arithmetic?
/// Positions are exact fractions of a whole note. Floating point would accumulate rounding
/// errors over long scores (e.g. triplets of triplets), so two notes that must line up exactly
/// could drift by a sample. We only leave exact arithmetic at the very last step, here.
///
/// # Complexity
/// O(log n) in the number of tempo changes (binary search on the cumulative table).
pub fn position_to_sample(&self, pos: Rational) -> u64 {
    // Find the tempo segment containing `pos` with a binary search: the table is sorted by
    // start position, so `partition_point` gives us the first segment starting AFTER `pos`.
    let idx = self.segments.partition_point(|s| s.start <= pos);
    // ...
}
```

## 5. Efficienza e tempo reale (priorità numero uno)

**Hardware di riferimento:** portatile di fascia media di circa 5 anni fa (4 core / 8 thread, classe Core i5-1135G7 o Ryzen 5 5500U, 8 GB di RAM, GPU integrata, SSD, schermo 1080p). Ogni funzionalità deve girare bene lì. Target iniziali (da calibrare dopo le prime misure; cambiarli richiede un ADR):

- Callback audio ≤ 50% del periodo del buffer (256 frame a 48 kHz ≈ 5,3 ms → ≤ 2,6 ms) con il "progetto di riferimento" (16 tracce audio + 16 MIDI, solo strumenti interni); zero xrun in 10 min.

- Interfaccia: da input a fotogramma ≤ 50 ms; scorrimento/zoom di 100 battute × 10 righi a 60 fps con GPU e ≥ 30 fps con rendering software; re-layout dopo la modifica di una nota ≤ 16 ms.

- Avvio fino a interattivo ≤ 3 s; apertura di una partitura da 100 battute ≤ 1 s; RAM a riposo con progetto vuoto ≤ 250 MB.

Regole:

- **Thread audio:** niente allocazioni, niente lock, niente I/O, niente attese non limitate. Comunicazione solo con strutture lock-free a capacità fissa. Un test fallisce se il thread audio alloca.

- Scegli l'algoritmo e la struttura dati migliori FIN DALL'INIZIO e annota la complessità nel commento. Prima si sceglie bene, poi si misura: mai ottimizzare a caso.

- Ogni percorso caldo ha un benchmark. Preferisci misure deterministiche (conteggio di istruzioni o allocazioni) ai tempi in secondi, che sui runner CI condivisi sono rumorosi.

- Layout e rendering **incrementali**: mai ricalcolare l'intera partitura per una nota.

- Rendering con fallback software; video con file proxy a bassa risoluzione.

- Ogni nuova dipendenza si valuta anche per dimensione binaria e tempo di compilazione.

## 6. Cross-platform

- Ogni scelta si valuta PRIMA per la compatibilità Linux/Windows/macOS.

- Identità visiva e comportamentale uguale ovunque (interfaccia disegnata da noi, font inclusi nel programma), ma rispetto delle convenzioni di sistema: "modificatore primario" astratto (Ctrl/Cmd), menu globale su macOS, finestre di dialogo file native.

- Percorsi con `std::path`, mai con stringhe incollate; mai assunzioni su `\n`/`\r\n`, maiuscole nei nomi file, separatori di percorso, codifica locale.

- La CI gira su Linux, Windows e macOS: se una piattaforma diventa rossa, il lavoro non è concluso.

- Librerie C/C++ SOLO quando non esiste alternativa ragionevole; ogni eccezione va registrata in `docs/native-deps.md` con motivazione, licenza e piano di sostituzione.

- Plugin: solo CLAP, VST3, LV2 (e AU su macOS); niente VST2. Ogni plugin gira in un processo separato; le interfacce dei plugin si aprono in finestre separate, non incorporate.

## 7. Internazionalizzazione — 6 lingue

Lingue: **en-US** (sorgente), **en-GB**, **it**, **fr**, **de**, **es**. en-US e en-GB sono cataloghi SEPARATI (es. *quarter note*/*crotchet*, *measure*/*bar*, *staff*/*stave*).

- Mai testo visibile all'utente scritto nel codice: solo chiavi in cataloghi Fluent (`crates/i18n/locales/<lingua>/`).

- Per OGNI testo nuovo o modificato: prima controlla `crates/i18n/glossary.toml` (procedura: `docs/i18n-process.md`). Se il termine non c'è, **fai una ricerca** (dizionari musicali, manuali localizzati di software di notazione e DAW, glossari accademici) per trovare il termine corretto in ciascuna lingua: i termini musicali NON si traducono alla lettera. Aggiungi la voce al glossario con fonti e motivazione.

- Plurali, genere e accordi si gestiscono con Fluent, mai concatenando stringhe.

- Un test fallisce se una chiave manca in una qualunque delle 6 lingue.

## 8. Documentazione

- Dopo OGNI modifica a file sorgente aggiorna **`DOCUMENTAZIONE.md` (it) e `DOCUMENTATION.md`** **(en-US)** nella root, nello STESSO passaggio. Stesso identico contenuto e stessa struttura (stessi titoli, stessi blocchi di codice), lingua diversa.

- Spiega ogni file/modulo in modo colloquiale, molto approfondito, con frammenti di codice reali dove servono. Aggiorna solo le parti toccate in questa sessione.

- Un hook **Stop** blocca la fine del turno se i sorgenti cambiano senza i due documenti aggiornati e strutturalmente identici.

- **Guide per l'utente**: i sorgenti Typst stanno in `docs/guide/it/` e `docs/guide/en/` (stessa struttura, stessi file). I PDF `GUIDA.pdf` e `GUIDE.pdf` NON sono committati: si generano con `cargo xtask guides` e la CI li pubblica come artefatti.

- Ogni commit ha un trailer `User-visible: yes` oppure `User-visible: no`. Con `yes` DEVONO cambiare anche i sorgenti di ENTRAMBE le guide (lo impone l'hook `commit-msg`). `yes` significa che cambia qualcosa di visibile o di approccio per l'utente.

- Gli screenshot sono generati in modalità headless dall'app stessa, sempre in en-US, e condivisi da entrambe le guide: cambia solo il testo.

- Le guide devono essere dettagliate, ogni spiegazione accompagnata da screenshot, esteticamente curate (font inclusi nel repository con licenza aperta, così i PDF sono identici ovunque).

## 9. Coerenza tra sessioni

- Il file di progetto ha **numero di versione e migrazioni dal primo giorno**; ogni cambio di formato richiede migrazione + test su file "golden" delle versioni precedenti.

- Le API pubbliche dei crate si cambiano in modo deliberato: se rompi un'API, aggiorna tutti i chiamanti nello stesso commit.

- Decisioni nuove → nuovo ADR (`docs/adr/NNNN-titolo.md`, stati: Proposed / Accepted / Superseded).

- Mai lasciare lavoro a metà senza annotarlo in `ROADMAP.md`.

## 10. Licenza e dipendenze

- Licenza del progetto: **GPL-3.0-or-later**. Ogni file sorgente inizia con `// SPDX-License-Identifier: GPL-3.0-or-later`.

- Prima di aggiungere una dipendenza verifica: licenza compatibile con la GPL, manutenzione attiva, dimensione, necessità reale. `cargo deny` blocca le licenze non ammesse.

- Verifica sempre versioni e stato di un crate su crates.io: non fidarti della memoria.

- Mai committare certificati di firma, chiavi o credenziali.

## 11. Git (GitHub)

- Mai commit diretti su `main` (protetto): ogni lavoro su un branch (`feature/…`, `fix/…`) e poi pull request. La CI deve essere verde prima del merge.

- Commit atomici e frequenti, messaggi descrittivi, trailer `User-visible` obbligatorio.

- Push dopo ogni commit significativo (backup). Mai force push, mai `git reset --hard` / `git clean -fd` senza mia conferma esplicita.

- Prima di refactoring corposi: tag di snapshot (`git tag pre-refactor-AAAA-MM-GG`).

## 12. Checklist — cosa significa "lavoro concluso"

1. Compila senza warning nuovi; `cargo xtask check` passa.

2. Commenti in inglese completi e dettagliati su tutto il codice nuovo o modificato.

3. Testi dell'interfaccia presenti in tutte e 6 le lingue, con ricerca terminologica fatta.

4. Percorsi caldi con benchmark e rispetto dei budget di §5.

5. `DOCUMENTAZIONE.md` e `DOCUMENTATION.md` aggiornati e identici nella struttura.

6. Commit con trailer `User-visible`; se `yes`, sorgenti delle due guide aggiornati.

7. Nessun dato duplicato tra le viste; nessun ADR contraddetto.

8. `ROADMAP.md` aggiornato.

9. Branch pubblicato su GitHub e CI verde su Linux, Windows e macOS.

