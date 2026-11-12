# Scaletta di lavoro — Orchestra

> **Stato di questo documento: v0.63, approvata dal proprietario il 2026-11-12.**
> L'impianto è approvato; le voci marcate **❓ Decisione** restano domande aperte che Claude non
> risolve da solo, e ogni passo si avvia solo con il protocollo di §0.2.

## 0. A cosa serve e come si usa

### 0.1 Il ruolo dei documenti

| Documento | Cosa contiene | Chi lo cambia |
|---|---|---|
| `SCALETTA.md` (questo) | **Il piano**: fasi, passi, ordine, criteri di "fatto", decisioni da prendere e quando | Claude propone, il proprietario approva; il proprietario può modificarlo a mano in ogni momento |
| `ROADMAP.md` | **Lo stato**: cosa è stato fatto, dove siamo, cose aperte (CLAUDE.md §0) | Claude a fine sessione |
| `docs/adr/` | **Le decisioni** vincolanti, con motivazioni | Nuovo ADR solo dopo l'ok del proprietario |

La scaletta **non ripete** il contenuto degli ADR: li cita. Se una voce della scaletta e un ADR
"Accepted" sembrano in conflitto, vince l'ADR e Claude lo segnala.

### 0.2 Protocollo di lavoro (concordato il 2026-10-30)

1. **"briefing"** → Claude rilegge **sempre** questo file (può essere stato modificato a mano),
   `ROADMAP.md` e lo stato git, poi scrive un punto della situazione **brevissimo** e propone il
   lavoro della sessione (di norma: il prossimo passo ⬜ della scaletta, oppure una sua parte).
2. Il proprietario dà il **prompt** della sessione (o approva la proposta così com'è).
3. Claude **non parte**: analizza il prompt, segnala problemi, rischi e difficoltà, fa domande se
   mancano informazioni, propone miglioramenti o aggiunte utili. Se non c'è nulla da aggiungere lo
   dice esplicitamente ("va già tutto bene così") e **chiede l'ok**.
4. Solo dopo l'**ok** esplicito Claude lavora.
5. A fine passo Claude valuta se la scaletta va aggiornata (passi da dividere, aggiungere,
   spostare, stime sbagliate, decisioni emerse) e **propone** le modifiche; le applica solo dopo
   l'ok. Ogni modifica approvata finisce nel registro in fondo (§9).
6. Nessuna decisione progettuale autonoma: dove c'è una scelta, Claude presenta le opzioni con una
   raccomandazione motivata e aspetta.

### 0.3 Legenda

- Stato: ⬜ da fare · 🔄 in corso · ✅ fatto · ⏸ sospeso · ✂️ eliminato (resta scritto, barrato nel
  registro)
- **ID stabili** (es. `F1.04`): non si rinumerano mai, così briefing, commit e ROADMAP possono
  citarli. Un passo nuovo inserito in mezzo prende un suffisso (`F1.04b`).
- **Stima** in *sessioni* (una sessione = un blocco di lavoro con Claude che si chiude con commit,
  documentazione e ROADMAP aggiornati). Le stime sono volutamente prudenti e verranno corrette.
- **❓ Decisione** = va presa con il proprietario prima o durante quel passo. **ADR** = il passo
  produce o modifica un ADR.

### 0.4 Cosa vale per OGNI passo (non lo ripeto più sotto)

Ogni passo è "fatto" solo se rispetta la checklist di CLAUDE.md §12: `cargo xtask check` verde,
commenti inglesi completi, testi in 6 lingue con ricerca terminologica, benchmark sui percorsi
caldi, `DOCUMENTAZIONE.md`/`DOCUMENTATION.md` aggiornati e identici nella struttura, trailer
`User-visible` (e guide se `yes`), nessun ADR contraddetto, `ROADMAP.md` aggiornato, branch su
GitHub con CI verde su Linux, Windows e macOS. Un passo = un branch e una PR (passi piccoli
possono condividere una PR, se il proprietario è d'accordo).

---

## 1. Filosofia della scaletta

1. **Fare bene, non veloce.** Nessuna scadenza. Un passo si chiude quando è giusto, non quando è
   "abbastanza". Meglio dividere un passo in due che chiuderlo a metà.
2. **Dal basso verso l'alto, come le dipendenze dei crate.** Prima il tempo, poi il modello, poi
   ciò che lo legge (playback, layout, file), infine viste e audio. Ogni strato è testato da solo,
   senza display e senza audio, prima che qualcosa lo usi.
3. **Le strutture dati sono la prestazione** (ADR-0007). Le scelte che sono difficili da cambiare
   dopo (rappresentazione del tempo, identità degli oggetti nel modello, struttura delle voci,
   comandi e notifiche, formato del file) si fanno con un ADR, con alternative misurate quando
   serve, **prima** di scrivere il codice che ne dipende.
4. **Rischio prima, rifinitura dopo.** In ogni fase si affronta per primo il pezzo che può
   costringerci a cambiare idea (es. le misure GPU in Fase 2, la latenza dei bridge in Fase 4).
5. **Ogni fase finisce con una tappa dimostrabile** (§7), verificabile dal proprietario, non solo
   con i test.
6. **Niente lavoro "di riserva".** Non si scrive codice per funzioni future "perché poi servirà":
   si lascia lo spazio nell'architettura (trait, versioni del file) e basta.
7. **La CI su tre piattaforme è parte del lavoro, non un controllo finale.**

---

## 2. Visione d'insieme

| Orizzonte | Fasi | Obiettivo | Stima grezza |
|---|---|---|---|
| **Breve** | F0+ raccordo, **F1** | Il "motore" senza interfaccia: tempo esatto, modello unico a due strati, forma, comandi e undo, playback derivato, file di progetto v1 | 25–35 sessioni |
| **Medio** | **F2A** fondamenta UI, **F2B** notazione, **F3** audio e timeline DAW | Il primo programma usabile: si scrive una partitura, la si vede come spartito e come DAW, la si ascolta, la si salva | 70–110 sessioni |
| **Lungo** | **F4** plugin, **F5** video, **F6** prima alpha pubblica, oltre | Programma completo nelle tre anime, distribuito sulle tre piattaforme | da stimare a fine F3 |

Le stime del medio e lungo termine sono **ordini di grandezza**: verranno ricalcolate alla
chiusura di ogni fase, con i dati veri delle fasi precedenti.

---

## 3. BREVE TERMINE

### F0+ — Raccordo dopo la Fase 0 (1–2 sessioni)

Chiudere le code della Fase 0 prima di iniziare, così la Fase 1 parte pulita.

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F0.15 | **Verifica CI macOS**: la ROADMAP segnala il run macOS come punto aperto; verificare che sia verde e che lo resti | ✅ tutti i run su `main` verdi; run settimanale aggiunto | 0,5 |
| F0.16 | **Revisione dei lint "pedantic"** in `Cargo.toml` (proposta in attesa di revisione del proprietario) | ✅ pedantic confermato + lint `restriction` scelti | 0,5 |
| F0.17 | **Approvazione di questa scaletta** e collegamento da `ROADMAP.md` e `CLAUDE.md` §0 | ✅ | 0,5 |

- **Decisione F0.16 (D01), chiusa il 2026-10-30:** pedantic confermato con le tre eccezioni;
  aggiunti lint `restriction` scelti uno per uno; `indexing_slicing` in `core-time` e
  `core-model`, `arithmetic_side_effects` in `core-time`; scartati `nursery` e `as_conversions`.
  Dettagli in `Cargo.toml` e nella documentazione.
- Restano aperti e **non bloccano** la Fase 1: revisione del glossario da parte di madrelingua
  (voci `review = true`), "region" o "clip", soglia di tolleranza di default.

### F1 — `core-time`, `core-model`, playback di base, file di progetto (24–33 sessioni)

**Obiettivo:** tutto il cuore musicale esiste ed è testato **senza interfaccia e senza audio**.
Alla fine si può costruire da codice (o da un piccolo strumento di prova) una partitura con
ritornelli, salvarla, riaprirla, annullare e ripetere ogni modifica, ottenere la sua esecuzione
"srotolata" come lista di eventi.

**Perché in questo ordine:** ogni altra parte del programma legge questi dati. Un errore qui si
paga ovunque; per questo la fase è lunga e piena di ADR.

#### F1-A — Il tempo (`core-time`)

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F1.01 | **ADR: il tipo razionale** — rappresentazione, politica di overflow, normalizzazione | ✅ ADR-0015 | 1 |
| F1.02 | **Infrastruttura di test e benchmark**: `cargo xtask bench`, test di proprietà, contatore di allocazioni | ✅ ADR-0016 | 1–2 |
| F1.03 | **Implementazione del tipo razionale** + test di proprietà (con oracolo) + test "non alloca" | ✅ PR #8 | 1–2 |
| F1.03b | **Benchmark del tipo razionale + confronto con `main` in CI** (soglie di regressione) | ✅ ADR-0017, PR #9 | 1 |
| F1.04 | **ADR della mappa delle misure + tipi posizione e durata** | ✅ ADR-0018 | 1 |
| F1.04b | **Mappa delle misure (metri)**: battute/movimenti ↔ posizione razionale, battute irregolari e libere, numerazione, benchmark | ✅ ADR-0019, PR #11 | 1–2 |
| F1.05 | **ADR della mappa dei tempi + unità del tempo reale** (tempo, secondi, campioni, fotogrammi) | ✅ ADR-0020, PR #12 | 1 |
| F1.05b | **Mappa dei tempi (tempo map)**: posizione ↔ secondi ↔ campioni ↔ fotogrammi, in entrambe le direzioni, con i cambi graduali | ✅ ADR-0021, PR #13 | 2 |

Dettagli:

- **F1.01** (ADR-0005 §6 rimanda qui la politica di overflow). Punti da decidere:
  numeratore/denominatore `i64` sempre ridotti ai minimi termini; confronti senza overflow
  (prodotti incrociati in `i128`); operazioni `checked_*` con errore tipizzato; quando e se
  passare a interi grandi. ❓ **Decisione:** tipo nostro oppure un crate esistente (es.
  `num-rational`: da verificare su crates.io — licenza, manutenzione, dimensione, controllo
  dell'overflow). Raccomandazione preliminare di Claude: tipo nostro, piccolo e con overflow
  esplicito, ma la decisione è del proprietario dopo il confronto.
  **Decisione D02, chiusa il 2026-10-30:** tipo nostro, `i64` ai minimi termini con intermedi
  `i128`, errore tipizzato, solo metodi `checked_*` (niente operatori), niente interi grandi per
  ora. Dettagli e crate esaminati in ADR-0015.
- **F1.02** È il primo vero uso di `cargo xtask bench`. ❓ **Decisioni:** (a) strumento per
  misure deterministiche: conteggio istruzioni (Callgrind, disponibile solo su Linux) e/o tempi
  con un framework classico, e cosa gira in CI su quali piattaforme; (b) crate per i test di
  proprietà (es. `proptest`), da verificare; (c) contatore di allocazioni globale nei test
  (ADR-0007 regola 1), che servirà anche per il thread audio in F3. Ogni dipendenza nuova va
  valutata per licenza, dimensione e tempo di compilazione (CLAUDE.md §5, §10).
  **Decisione D03, chiusa il 2026-10-31:** benchmark solo con Gungraun (conteggio istruzioni
  sotto Valgrind; in CI su Linux, solo compilati su Windows e macOS), test di proprietà con
  `proptest`, contatore di allocazioni nostro, per thread, nel nuovo crate `test-support` (solo
  `[dev-dependencies]`). Confronto con `main` e soglie rimandati a F1.03b. Dettagli in ADR-0016.
- **F1.03** Implementa `Rational` secondo ADR-0015. **Oracolo** dei test di proprietà (ADR-0016
  §4): proposta di Claude, approvata il 2026-10-31, un calcolo di riferimento in `i128` scritto nei
  test stessi, senza crate nuovi; ogni operazione è confrontata con l'oracolo, compresi i casi
  limite di ADR-0015 (`i64::MAX`, zero, negativi, risultati che entrano in `i64` solo dopo la
  riduzione). Test "non alloca" con `orchestra-test-support`.
  **Fatto il 2026-10-31.** Scelte d'interfaccia approvate dal proprietario lo stesso giorno: solo il
  tipo base (i tipi specifici in F1.04); solo `Debug`, niente `Display`/`FromStr` (forma testuale
  in F1.15); `Display` dell'errore in inglese per gli sviluppatori (l'interfaccia tradurrà le
  varianti con chiavi Fluent); costruttori e aritmetica `const fn`.
- **F1.03b** Benchmark Gungraun di somma, prodotto, confronto e riduzione; nella CI il confronto
  di una pull request con `main` (baseline di Gungraun) e soglie di fallimento, tarate sui primi
  numeri reali (ADR-0016 §3). Diviso da F1.03 il 2026-10-31 perché insieme superavano la stima.
  Sono qui anche i benchmark che ADR-0015 §9 attribuiva a F1.03. Serve almeno un caso con valori
  oltre i 64 bit, per misurare il percorso a 128 bit del massimo comun divisore (non solo quello
  veloce a 64 bit).
  **Fatto il 2026-10-31 (decisioni in ADR-0017):** confronto nello stesso job con la base comune
  con `main`, soglia +5% sulle istruzioni, trailer `Bench-regression: accepted` per i peggioramenti
  voluti, job `bench` obbligatorio nella regola di `main`. Regole per tutti i benchmark futuri:
  ogni benchmark esegue un **lotto** di operazioni (conteggi nell'ordine delle migliaia), e un
  benchmark che cambia significato prende un **nome nuovo**.
- **F1.04** Casi da coprire con test: battuta in levare (anacrusi), cambi di metro, metri
  composti e additivi (3+2+2/8), metri non binari (es. 4/3, se li vogliamo: ❓), numerazione
  delle battute con eccezioni. Ricerca O(log n). Qui si progettano anche i **tipi specifici**
  sopra `Rational` (posizione e durata, ADR-0015 §3: posizione − posizione = durata, posizione +
  posizione non compila) e, se la mappa delle misure ne ha bisogno, un'operazione di resto
  (`rem`).
  **Fatto il 2026-10-31 (ADR-0018 Accepted):** diviso in F1.04 (ADR e tipi `Position`/`Duration`)
  e F1.04b (la mappa). Decisione D04 chiusa: metri non binari ammessi nel tipo (incisione in F2B,
  esportazione MIDI in F1.18). Scelte approvate: `Duration` con segno, al massimo 8 gruppi
  additivi, eccezioni di numerazione di base adesso (quelle legate alle volte con la forma),
  nessun `rem` per ora. Su richiesta del proprietario, le **cadenze libere** con sospensione del
  metro (battute `Free`: nessun movimento, il metro riprende dopo), con lunghezza esplicita e
  contate di default.
- **F1.04b** Implementa `Meter`, le battute irregolari (allineate all'inizio, alla fine, libere),
  la mappa a segmenti con ricerche O(log n) e la numerazione mostrata, come da ADR-0018. Casi da
  coprire con test: anacrusi (movimento 4 in 4/4), cambi di metro, composti (6/8 in due),
  additivi (3+2+2/8), non binari (4/3), cadenza libera su una o più battute con ripresa del
  metro, numerazione con battute non contate e ripartenze, errori tipizzati. Test di proprietà
  (posizione → battuta → posizione è l'identità), test "non alloca" sulle ricerche, benchmark a
  lotti (ADR-0017 §4).
  **Fatto il 2026-10-31 (ADR-0019 Accepted):** prima di partire il proprietario ha scelto che una
  battuta più lunga del suo metro ripeta lo schema dei movimenti con la numerazione che prosegue,
  e ha chiesto che la **suddivisione dei movimenti** sia separata dal metro scritto e modificabile
  per sezione o per singola battuta (9/8 contato 3+2+2+2; unità propria, quindi anche l'emiolia).
  Regola predefinita invariata: 7/8 conta sette crome. Requisito per l'interfaccia in F2.22b.
- **F1.05** Segmenti a tempo costante (esatti) e cambi graduali (accelerando/rallentando)
  integrati analiticamente; regola di arrotondamento ai campioni **documentata**; frame rate
  razionali (24/1, 25/1, 30000/1001); conversione inversa secondi → posizione (servirà a
  registrazione e DAW). Nessuna allocazione nelle conversioni usate dal thread audio (test).
  Benchmark: ricerca O(log n) misurata. ❓ **Decisione:** in quale grandezza è lineare un cambio
  graduale (BPM lineare nel tempo musicale o nel tempo reale): cambia le formule e il
  comportamento percepito. **Nota (da F1.01):** integrare un cambio graduale dà in generale un
  logaritmo, quindi i secondi dentro quei segmenti non sono razionali: F1.05 deve dichiarare fin
  dove arriva l'esattezza, prima dell'arrotondamento finale ai campioni (ADR-0015, Conseguenze).
  **Fatto il 2026-10-31 (ADR-0020 Accepted):** decisione D05 chiusa, i cambi graduali sono
  lineari nel **tempo reale** (accelerazione costante): le loro durate sono razionali, quindi i
  confini restano esatti. Verificato su Friberg e Sundberg (1999): q = 2 è "lineare nel tempo" ed
  è stata, con q = 3, la curva più apprezzata all'ascolto. Tempo come unità + BPM razionali; mappa
  sulle posizioni scritte (l'orologio srotolato è di `playback`); campione più vicino (pareggi al
  successivo) e fotogramma che contiene l'istante. Implementati `Tempo`, `Seconds`, `SampleRate`,
  `FrameRate` con gli arrotondamenti.
- **F1.05b** Implementa la mappa dei tempi di ADR-0020: segmenti costanti e graduali con istanti
  iniziali esatti cumulati, ricerca O(log n) nei due sensi, indice di campione e di fotogramma
  esatto anche dentro i cambi graduali (stima in `f64` + correzione esatta con l'inversa; ripiego
  `f64` in caso di overflow, da misurare), modifiche tutto-o-niente. Test contro un oracolo,
  test "non alloca" sulle conversioni, benchmark a lotti.
  **ADR-0021 Accepted il 2026-11-01:** una simulazione ha mostrato che gli istanti esatti cumulati
  escono dai limiti di `i64` dopo 8–13 cambi graduali (di `i128` dopo 31–53). Gli istanti di
  confine stanno quindi su una griglia di 1/705 600 000 s (al massimo mezzo tick di errore per
  confine), le posizioni dentro i cambi graduali si arrotondano alla stessa griglia in semibrevi,
  entrambe le conversioni si fermano ai bordi del segmento; un cambio graduale può avere un tempo
  di partenza ("Meno mosso, poco a poco accel."). Diviso in due parti. **Prima parte fatta il
  2026-11-01:** `TempoMap`, posizioni ⇄ secondi, modifiche, oracolo, test "non alloca". **Seconda
  parte:** indici esatti di campione e di fotogramma dentro i cambi graduali (controllo esatto in
  `i128`), misura di quanto spesso serve il ripiego `f64`, benchmark a lotti, PR.
  **Fatto il 2026-11-01 (PR #13):** `sample_at`/`frame_at` restituiscono `IndexAt::Exact` o
  `Estimated` (scelta del proprietario: il tipo non nasconde mai una stima); una partitura di prova
  di 2 h 16 min con 500 cambi graduali non ha avuto bisogno di nessuna stima a 44,1–384 kHz né a
  24/25/29,97 fps. Benchmark: ≈ 4500 istruzioni per un indice dentro un cambio graduale, ≈ 0,2 ms
  per una modifica. F1-A completata.

#### F1-B — Il modello (`core-model`)

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F1.06 | **ADR: architettura del modello** — identità degli oggetti, struttura in memoria, come sono organizzate voci e tracce | ✅ | 2 |
| F1.07 | **Altezza con grafia, intervalli, trasposizione** (tipi puri, molto testati) | ✅ | 1–2 |
| F1.08 | **Scheletro del documento**: progetto, tracce/strumenti/righi, voci, mappe del tempo condivise | ✅ PR #16 | 2 |
| F1.09 | **ADR del contenuto di una voce** (note a due strati, ADR-0004) | ✅ ADR-0024 | 1 |
| F1.09b | **Eventi, figure, gruppi irregolari, legature di valore, chiavi, armature**, `validate` | ✅ | 2 |
| F1.09c | **Abbellimenti, articolazioni, ornamenti, corone, legature di portamento, dinamiche, forcelle, indicazioni di tempo, scostamenti**, benchmark | ✅ PR #17 | 2 |
| F1.10 | **ADR della forma**: ritornelli, volte, segno/coda, D.C./D.S. — come marcature scritte | ✅ ADR-0025 | 1 |
| F1.10b | **Ritornelli, volte, segni e salti, ripetizioni di battuta, regole di passaggio, stile di numerazione**, `validate` | ✅ PR #18 | 2 |
| F1.11 | **ADR dei comandi, dell'undo/redo unico e delle notifiche di modifica a grana fine** | ✅ ADR-0026 | 1 |
| F1.11b | **Il motore dei comandi**: modifiche elementari con inversa, transazioni, cronologia, gesti, insiemi delle modifiche; inserimento e cancellazione di battute in `core-time`; comandi su eventi, marcature, forma e battute | ✅ PR #19 | 2–3 |
| F1.11c | **Cambio di metro** nelle due modalità (riscrittura di default, battute mantenute) | ✅ PR #20 | 1–2 |

Dettagli:

- **F1.06** È la decisione più importante della fase. Domande: ID stabili (indici generazionali
  o identificatori a 64 bit) che sopravvivono a undo/redo e al salvataggio; eventi di una voce in
  una struttura ordinata per posizione razionale (albero ordinato? vettore ordinato a blocchi per
  battuta?) con la complessità di inserimento/ricerca/scansione annotata; le battute sono oggetti
  memorizzati oppure derivate dalla mappa dei metri? (lo spartito ne ha bisogno, la DAW no:
  ADR-0003 vieta di duplicare); dove vivono le marcature che coprono un intervallo (forcelle,
  legature di portamento, ottave). Claude preparerà 2–3 alternative con un mini-prototipo e
  misure (inserimento in una partitura di riferimento da 100 battute × 10 righi). Da ADR-0018:
  decidere se le note memorizzano una posizione assoluta o relativa alla battuta (cambiare un
  metro non deve spostare per sbaglio tutte le note successive), e i comandi che allungano una
  battuta libera quando vi si inseriscono note. ❓
  **Fatto il 2026-11-01:** ADR-0022 Accepted (candidato P3, voci per strumento); prototipo e
  misure in `spikes/model/`. Sulla partitura pesante nessuna modifica misurata cresce con la
  lunghezza della partitura.
- **F1.07** Grafia (nome della nota, alterazione, ottava), enarmonie, intervalli con qualità,
  trasposizione di strumenti traspositori con grafia corretta (ADR-0004), altezza sonora MIDI
  derivata. Microtoni: ❓ **Decisione:** previsti nel tipo da subito (anche se non usati) o
  rimandati? Cambiarlo dopo tocca il formato del file.
  **Fatto il 2026-11-01:** ADR-0023 Accepted, modulo `pitch` di `core-model` (alterazione
  razionale in semitoni, armature tradizionali fino a ±14 quinte, grafia da MIDI, trasposizione).
- **F1.08** Implementa ADR-0022 §1–3: contatore unico e ID tipizzati, tracce → strumento → righi
  e voci (le voci appartengono allo strumento, ogni evento ha il suo rigo), elenco delle battute
  con `BarId`.
  **Fatto il 2026-11-02 (PR #16).** Decisioni del proprietario dello stesso giorno: i documenti
  nuovi si creano con un `DocumentBuilder` pubblico (file, generatore, test, benchmark), mentre un
  documento esistente cambierà solo con i comandi di F1.11 (ADR-0003 §4); chiave e armatura vanno
  in F1.09; uno strumento nuovo ha una voce per rigo; al massimo 16 righi e 256 voci per strumento
  (limiti tecnici contro i file corrotti, non musicali).
- **F1.09** Tutti i "casi difficili" della tabella di ADR-0004 devono avere un test.
  Gli scostamenti sono **delta**, mai copie assolute. Vincoli di ADR-0022 §4 e §6: blocchi per
  battuta con offset, un ID per ogni testa di nota, la battuta troppo piena è uno stato valido,
  le note che attraversano una stanghetta diventano catene di note legate, marcature estese legate
  alle note oppure a (battuta, offset). Da F1.08: la **chiave** iniziale di ogni rigo e i cambi di
  chiave e di armatura.
  **Fatto il 2026-11-02 (ADR-0024 Accepted):** il proprietario ha fissato l'ambito (corone e
  ornamenti dentro, testo cantato in un passo suo) e ha approvato tutte le raccomandazioni C1–C9:
  figura + punti salvati, durata ricavata; appartenenza ai gruppi irregolari sull'evento, al massimo
  4 livelli e termini fino a 255; gruppi irregolari a cavallo della stanghetta ammessi; legature di
  valore come collegamento in avanti; abbellimenti a durata metrica zero; chiavi per rigo, armature
  per documento con eccezioni per strumento; scostamenti nella nota, in tempo musicale; indicazioni
  di tempo a (battuta, offset) come fonte, mappa dei tempi ricavata. Emenda ADR-0022 §5 e §6.
- **F1.09b** Implementa ADR-0024 §1–§4 e §9: `Event`, `Chord`, `Rest`, `Note`, `NoteValue`,
  pausa di battuta intera, tabella dei gruppi irregolari per voce, legature di valore, chiavi per
  rigo, armature; `validate`; test dei casi difficili di ADR-0024 §13 che li riguardano.
- **F1.09c** Implementa ADR-0024 §5–§8, §10–§11. Il `DocumentBuilder` riceve le indicazioni di
  tempo invece della mappa già fatta (cambio di API, chiamanti aggiornati nello stesso commit).
  Misura della memoria per nota sulla partitura pesante (ADR-0007).
  **Fatto il 2026-11-03 (PR #17), in due sessioni.** Scelte del proprietario: E1–E7 (2026-11-02) e,
  il 2026-11-03, l'indicazione di tempo iniziale come campo del documento con un suo ID; nel ricavare
  la mappa dei tempi un offset oltre la stanghetta di una battuta troppo piena vale come la
  stanghetta, nella stessa posizione vince l'indicazione successiva, un graduale ridotto a niente è
  immediato; la mappa si ricalcola a ogni indicazione aggiunta, tutto o niente; una dinamica per
  ambito in un punto, forcelle dello stesso ambito senza sovrapposizioni; tetto di memoria di 285
  byte per nota (misurati 257 su 221 169 note).
- **F1.10** **Fatto il 2026-11-03 (ADR-0025 Accepted).** Ambito fissato dal proprietario: il nucleo
  (stanghette di ritornello, volte, segno, coda, Fine, D.C./D.S. e varianti) più le ripetizioni di
  battuta (%), la musica suonata solo in alcuni passaggi e i salti scritti come testo libero.
  Scelte approvate (F1–F10): marcature solo sulle stanghette; ritornello senza inizio che torna
  all'inizio della partitura; niente ritornelli annidati; per ogni salto la scelta se ripetere i
  ritornelli dopo (di default no, con l'ultima volta); un salto dentro un ritornello all'ultimo
  passaggio o al passaggio indicato; battute % senza note, suonate dall'origine, per rigo; regole
  di passaggio come intervalli; al massimo 64 esecuzioni. Per F9 il proprietario ha scelto **uno
  stile di numerazione per documento** (continua, stessi numeri con lettere, stessi numeri senza
  lettere; dopo le volte si riparte dalla più lunga).
- **F1.10b** Implementa ADR-0025 §2–§8 (ID nuovi, elenchi della forma, `PassSet`, metodi del
  builder, regole 8–11 di `validate`, stile di numerazione) con i test dei casi difficili di §10,
  un oracolo e ricerche senza allocazioni.
  **Fatto il 2026-11-03 (PR #18), in due sessioni.** Scelte del proprietario: le volte successive
  più lunghe della prima proseguono la numerazione e dopo le volte si riparte dalla più lunga; lo
  stile si applica sopra la numerazione di `core-time` e una ripartenza dentro le volte vince; con
  le volte `times` vale 2 oppure n. Correzione di ADR-0025 §3: ogni volta tranne l'ultima finisce
  con un ritornello ("1. :‖ 2. :‖ 3."). Precisazioni annotate in ADR-0025 §8: il "fino a" di un
  salto sta fra il bersaglio e il salto; un segno d'inizio, uno di fine e un salto per battuta;
  `OnPass(n)` solo dentro una sezione ripetuta; nessuna marcatura di forma dentro una ripetizione
  di battuta; regole di passaggio entro le esecuzioni della sezione e senza sovrapposizioni.
- **F1.11** Ogni comando reversibile; raggruppamento (una "transazione" = un passo di undo,
  es. incollare 200 note); notifiche che dicono **cosa** è cambiato, per l'aggiornamento
  incrementale di layout e playback (ADR-0003 §6, ADR-0007). Test di proprietà: per ogni
  sequenza di comandi, fare-poi-annullare riporta al documento identico. ❓ **Decisioni:** limite
  della cronologia di undo (numero di passi o memoria), comandi "fusi" (trascinamento continuo =
  un solo passo). ❓ **Decisione D21:** comportamento predefinito di un cambio di metro
  (mantenere il contenuto delle battute, oppure riscrivere la musica fino al cambio di metro
  successivo). In `core-time` servono l'inserimento e la rimozione di battute nella mappa dei
  metri (ADR-0022 §8). Da ADR-0024 §11: i cambi di tempo sono indicazioni a (battuta, offset) e
  seguono le battute da soli, quindi non servono più lo spostamento dei cambi di tempo né il suo
  test di proprietà; dopo ogni comando che li tocca la mappa dei tempi si ricalcola.
  Da F1.09c: la mappa dei tempi si ricalcola anche dopo ogni comando che cambia dove cominciano le
  battute (inserire o togliere battute, cambi di metro). Da ADR-0025: cancellare una battuta che
  porta una marcatura di forma cancella la marcatura nello stesso comando, e ogni comando lascia la
  forma valida.
  **Fatto il 2026-11-04 (ADR-0026 Accepted):** il proprietario ha approvato tutte le
  raccomandazioni U1–U11 e la divisione del passo. Modifiche elementari che restituiscono la propria
  inversa, comandi come dati, transazioni tutto-o-niente; cronologia lineare limitata in memoria
  (128 MB di default, l'ultimo passo sempre tenuto, non salvata nel file: D08); gesti espliciti per
  i trascinamenti; un insieme delle modifiche restituito da ogni transazione, undo e redo, più un
  numero di revisione. Battute inserite: prendono ciò che vale prima di loro; rifiutato dentro una
  ripetizione di battuta. Battute cancellate: metro, chiave, armatura e tempo passano alla battuta
  successiva, le dinamiche si cancellano, gli intervalli si accorciano, le marcature di forma rese
  non valide si cancellano a cascata, resta almeno una battuta. **D21:** il cambio di metro
  **riscrive** la musica fino al cambio di metro successivo (la musica resta dove suona, si spostano
  le stanghette), con "battute mantenute" come modalità esplicita.
- **F1.11b** Implementa ADR-0026 §1–§6, §8 e §9 (tutto tranne il cambio di metro). Test di
  proprietà: dopo ogni transazione `validate` passa; fare-poi-annullare dà il documento identico
  tranne il contatore degli ID, indici derivati compresi; fare-annullare-ripetere dà fare; gli indici
  aggiornati incrementalmente coincidono con quelli ricostruiti da zero; una transazione fallita non
  cambia nulla. Benchmark a lotti sulla partitura normale e pesante (una nota ≤ 1 ms), memoria per
  passo della cronologia.
  **Parte 1 fatta il 2026-11-04:** in `core-time` inserimento e cancellazione di battute
  (`BarSpan`, l'una l'inversa esatta dell'altra); in `core-model` le modifiche elementari con
  inversa, l'insieme delle modifiche, l'`Editor` (transazioni, cronologia, undo/redo, revisione) e
  il test di proprietà sulle fotografie. Scelte d'interfaccia confermate dal proprietario lo stesso
  giorno: `Edit`, `Object` e `Transaction::apply` pubblici (chi usa le modifiche elementari a mano
  risponde delle regole musicali); `Transaction::new_id` restituisce un `NonZeroU64` (i costruttori
  degli ID tipizzati restano interni); i test dell'editor dentro il crate (`src/editor_tests.rs`).
  La modifica in blocco della mappa dei metri passa a F1.11c, l'unico che la usa. La parte 2 è
  divisa in 2a e 2b (proprietario, 2026-11-04).
  **Parte 2a fatta il 2026-11-04:** prima di scrivere il codice il proprietario ha approvato le
  precisazioni V1–V9 e W1–W6, ora ADR-0026 §11: si inserisce solo nello spazio libero; oltre la
  stanghetta una catena di note legate scritta con le figure più lunghe per prime; una nota di
  gruppo irregolare a cavallo della stanghetta è rifiutata fino a F1.11c; la battuta libera si
  allunga; cancellare lascia un buco e porta via abbellimenti, legature di portamento e gruppi
  irregolari vuoti; spostare non divide mai; una nuova altezza segue la catena di note legate; niente
  oltre l'ultima battuta; cancellare battute accorcia i cambi di tempo graduali, porta il tempo come
  indicazione immediata e cancella in cascata la forma resa non valida; niente inserimenti fra
  l'origine e la fine di una ripetizione di battuta. Se si cancellano le prime battute nel mezzo di
  un cambio graduale, la partitura parte dal suo **tempo d'arrivo** (scelta del proprietario: così
  dalla fine del cambio in poi la musica suona come prima). Implementati i comandi (`Command`),
  `Transaction::execute` ed `Editor::execute` con comandi atomici, i gesti con la compattazione e la
  mappa dei tempi ricalcolata dal vivo, la verifica locale sulle celle toccate (`check_cells`) con un
  indice inverso delle legature di valore; 19 casi scritti a mano e un test di proprietà con il
  `validate` completo dopo ogni comando (sette bug iniettati presi).
  **Parte 2b:** i comandi su marcature e forma (aggiungere, spostare e togliere ogni indicazione,
  dinamica, forcella, legatura di portamento, gruppo irregolare e marcatura di forma, lasciando la
  forma valida), i benchmark dell'editor sulla partitura normale e pesante (una nota ≤ 1 ms,
  l'annullamento di un incolla di 200 note, inserire e cancellare una battuta, un gesto di 1 000
  passi) e la memoria per passo della cronologia, poi la PR con ADR-0026 e tutto F1.11b.
  **Parte 2b fatta il 2026-11-05 (PR #19):** prima del codice il proprietario ha approvato le
  precisioni X1–X10, ora ADR-0026 §11: volte aggiunte per gruppo intero e ritornelli a coppie;
  togliere cancella in cascata, aggiungere, spostare e cambiare non cancellano mai nulla; una
  ripetizione di battuta su battute con musica è rifiutata; cambiare una marcatura = la nuova
  descrizione intera, stesso ID e stesso tipo; chiavi e armature non cambiano le altezze; gruppi
  irregolari aggiunti vuoti, cambiati sul posto, tolti con le loro note; valori iniziali impostati,
  mai tolti; i dati per battuta che non spostano le stanghette qui, il resto in F1.11c; verifica
  locale anche sulle marcature. Il test di proprietà ha poi trovato che le battute inserite fra due
  volte cadevano fuori da entrambe: **X11** (scelta del proprietario), entrano nella volta
  precedente e il ritornello si sposta con loro. Benchmark sulla partitura pesante: una nota ≈ 8 700
  istruzioni, cancellare una battuta ≈ 978 000, un passo di gesto ≈ 14 500. La stima della memoria
  della cronologia coincide con la misura (passi ristretti alla loro lunghezza).
- **F1.11c** Implementa ADR-0026 §7: `Rewrite` (catene di note legate, battuta irregolare finale,
  divisione delle battute sotto le marcature di forma, pause di battuta intera) e `KeepBars`, con
  casi scritti a mano, test di proprietà e benchmark; le regole di §7 si possono precisare con
  l'ok del proprietario. Diviso in due parti (proprietario, 2026-11-05).
  **Parte 1 fatta il 2026-11-05:** prima del codice il proprietario ha approvato Y1–Y6, ora
  ADR-0026 §11: `ChangeMeter` e `SetIrregularBar` con un `MeterMode`; `KeepBars` tiene il contenuto
  di ogni battuta; i raggruppamenti per battuta che non tornano si tolgono; levate troppo lunghe e
  raggruppamenti sbagliati sono rifiutati; niente cambi dentro una ripetizione di battuta; la
  modifica in blocco della mappa dei metri passa alla parte 2. Il test di proprietà ha poi trovato
  due casi, decisi dal proprietario: **Y7**, rifiutato un cambio che lascia oltre una stanghetta
  accorciata una nota con legatura (di valore o di portamento) verso la battuta dopo; **Y8**,
  correzione di `InsertBars` (F1.11b): forcelle, regole di passaggio e cambi graduali che finiscono
  proprio sul punto d'inserimento restano prima delle battute nuove. Benchmark sulla partitura
  pesante: un 3/4 a metà ≈ 5,7 milioni di istruzioni (60 battute troppo piene verificate), una
  battuta irregolare ≈ 122 000.
  La parte 2 è divisa in 2a e 2b (proprietario, 2026-11-05).
  **Parte 2a fatta il 2026-11-05:** prima del codice il proprietario ha approvato Z1–Z5, ora
  ADR-0026 §11: la modifica in blocco della mappa dei metri (`MeterMap::replace_span`, modifica
  elementare `MeterEdit::Span`; 60 battute ≈ 77 000 istruzioni); nessuna per la mappa dei tempi,
  che si ricava già una volta per transazione; le note di gruppo irregolare a cavallo della
  stanghetta si dividono in note legate dentro lo stesso gruppo, scritte nei suoi valori, così
  `EnterEvent` non le rifiuta più (cade V4); una sola funzione di divisione per `EnterEvent` e
  `Rewrite`.
  **Parte 2b:** `Rewrite` (regole 1–5 di §7: la musica resta dove suona e si spostano le
  stanghette fino al prossimo cambio di metro; catene di note legate, battuta irregolare finale,
  divisione delle battute sotto le marcature di forma, pause di battuta intera), con casi scritti a
  mano, test di proprietà e benchmark, poi la PR di tutto F1.11c.
  **Parte 2b fatta il 2026-11-05 (PR #20):** prima del codice il proprietario ha approvato R1–R11,
  ora ADR-0026 §11: la regione fino al prossimo cambio di metro o battuta libera; una nuova divisione
  con un'ultima battuta irregolare, così nulla dopo si sposta; levate e battute libere tenute; lo
  stesso algoritmo per `SetIrregularBar` e per le rimozioni; eventi e marcature messi dove suonano,
  catene di note legate anche nei gruppi irregolari, legature e fini di legatura di portamento
  sull'ultima parte; pause di battuta intera scritte; battute divise sotto le marcature di forma;
  rifiutato se tocca una ripetizione di battuta. Costo misurato: 60 battute × 41 voci della
  partitura pesante ≈ 118 milioni di istruzioni (cronologia 2,9 MB), forse oltre i 16 ms: accettato
  dal proprietario per ora, da misurare in tempo reale in F2.23.

#### F1-C — Ciò che legge il modello (`playback`, `io`)

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F1.12 | **Srotolamento della forma** in passaggi (written note id, pass index) | ✅ PR #21 | 2 |
| F1.13 | **ADR delle regole di esecuzione v0**: dall'intento alle note suonate | ✅ ADR-0027 | 1 |
| F1.13b | **Note suonate**: legature di valore fuse, articolazioni, abbellimenti, arpeggi, scostamenti e limiti, regole di passaggio e segni % | ✅ | 1–2 |
| F1.13c | **Dinamiche e forcelle** → velocity, aggiornamento incrementale per cella, benchmark (una nota ≤ 1 ms sulla partitura pesante) | ✅ | 2 |
| F1.14 | **ADR della soglia di tolleranza**: il comando "sposta nota dal lato DAW" (ADR-0004) | ✅ ADR-0028 | 1 |
| F1.14b | **Spostamento dell'attacco dalla DAW**: soglia nel documento, modo "solo esecuzione", tabella degli scostamenti per passaggio, aggiornamento delle note suonate | ✅ | 1–2 |
| F1.14c | **Durate dalla DAW** oltre la soglia e "sposta comunque" (divisione in legature) | ✅ PR #23 | 1–2 |
| F1.15 | **ADR: formato concreto del file** (completa ADR-0010) | ✅ ADR-0029 | 1 |
| F1.15b | **Salvataggio e caricamento della versione 1**: schema del file in `io`, `Document::from_parts`, forma testuale di `Rational`, regola 13 e limiti, salvataggio sicuro, test di andata e ritorno e di robustezza, benchmark | ✅ | 2 |
| F1.15c | **File golden della versione 1 e meccanismo delle migrazioni**, documentazione, PR di tutto F1.15 | ✅ PR #25 | 1 |
| F1.16 | **Crate `test-scores` (ADR-0031) con il preset "plain"**: l'attuale generatore spostato senza cambiare i documenti, i 7 file che lo includono migrati | ✅ PR #27 | 1 |
| F1.16b | **Preset "full"**: la partitura di riferimento plausibile con tutto ciò che il modello contiene (forma, gruppi irregolari, legature, dinamiche, tempi…), organico vero, normale e pesante; test di copertura, andata e ritorno, impronte | ✅ PR #29 | 1–2 |
| F1.16c | **Benchmark di fase** sul preset "full" (`crates/test-scores/benches/phase.rs`): apertura (byte → documento → srotolamento → note suonate), modifica di una nota in un ritornello con aggiornamento incrementale, srotolamento, salvataggio | ✅ PR #31 | 1 |
| F1.18 | **Esportazione MIDI file** degli eventi suonati (`io`): primo ascolto del modello | ✅ PR #33, ADR-0032, ADR-0027 §15 (taratura a orecchio, giri 1–2); ADR-0033 (tasto a metà strada, PR #37) | 1–2 |
| F1.17 | **Chiusura fase**: documentazione, ROADMAP, revisione della scaletta, PR | ✅ PR #35, tappa M1 | 1 |

Dettagli:

- **F1.13** È il punto in cui il principio "intento + scostamenti" diventa concreto. L'uscita è
  un buffer di eventi pre-calcolato: è proprio ciò che il motore audio (F3) riceverà
  (ARCHITECTURE: `audio-engine` non vede `core-model`). Restano fuori dalla v0 l'**intonazione**
  (temperamenti, intonazione naturale) e i segni di comma (Helmholtz-Ellis, Sagittal): ADR-0023
  §2–§3 usa il 12-TET come riferimento. ❓ **Decisione D22:** in quale passo si fa l'intonazione.
  **Fatto il 2026-11-05 (ADR-0027 Accepted):** ricerca su Dorico, MuseScore, Sibelius e sui formati
  di scambio (MusicXML 4.0, file MIDI, MIDI 2.0, MIDI Tuning Standard). Il proprietario ha
  approvato G1–G14 cambiandone tre: le dinamiche vanno da *pppp* a *ffff* (più *p* o *f* si
  scrivono ma suonano come gli estremi; dieci livelli a passo uniforme: 10, 23, 36, 49, 62, 75, 88,
  101, 114, 127 su 7 bit); le **acciaccature prima del tempo**, rubando alla nota precedente (la
  principale parte all'istante scritto), le appoggiature sul tempo; le **corone rimandate**, e poi
  un suono allungato con tempo inserito, **mai** un tempo più lento. Uscita in tempo musicale
  esatto con altezza esatta in semitoni e velocity a 16 bit; una nota per catena di legature;
  ornamenti suonati come scritti; aggiornamento per cella. **D22:** 12-TET con microtoni esatti ora,
  intonazioni dopo F3.04. Diviso in F1.13b e F1.13c, con una PR unica alla fine (proprietario).
- **F1.13b** **Fatto il 2026-11-05:** prima del codice il proprietario ha approvato N1–N10, ora
  ADR-0027 §13: una legatura si segue solo dove la nota d'arrivo suona subito dopo lungo il
  percorso; ogni testa di un accordo è una nota; accorciamenti sull'ultima testa della catena,
  accenti sulla prima; tenuto con staccato suona come portato; l'acciaccatura ruba alla nota
  precedente della voce lungo il percorso, dopo gli abbellimenti "dopo"; arpeggi sul primo accordo;
  scostamenti della catena; *mf* per tutte fino alle dinamiche. Modulo `perform` di
  `orchestra-playback`; limite noto: su una nota molto corta abbellimenti "dopo" e acciaccature
  insieme possono superarne la metà (la nota scende alla durata minima).
- **F1.13c** Diviso in due parti (proprietario, 2026-11-05). **Parte 1 fatta il 2026-11-05:** prima
  del codice il proprietario ha approvato H1–H13 e, chiedendo che un accento sia più forte solo
  all'attacco e poi torni alla sua dinamica a velocità fissa (più lentamente dopo *sf* e simili),
  l'**emendamento H14–H17**, ora ADR-0027 §14: la dinamica in vigore si segue lungo il percorso;
  *rf* +1 livello, *sf* +2, *sffz* +3, solo sulle note che cominciano lì; *n* = velocity 1;
  l'arrivo di una forcella dentro la battuta della sua fine e nel verso giusto, altrimenti un
  livello; le dinamiche dentro una forcella la dividono; vince la voce sullo strumento; ogni nota
  suonata ha un **profilo d'attacco** (attacco, tenuta, ritorno in secondi, livello d'arrivo); gli
  accenti di articolazione sono gradini fissi (accento +15 in 0,2 s); *fp* è *f* tenuto 0,3 s poi
  *p* in 0,1 s, *pf* il contrario. Modulo `loudness` di `orchestra-playback`. **Parte 2:** H11–H13,
  cioè la lista degli strumenti con dinamiche cambiate nel `ChangeSet`, `Performance::update` per
  cella, i benchmark sulla partitura pesante; poi la PR di tutto F1.13.
  **Parte 2 fatta il 2026-11-05:** `ChangeSet::dynamics_changed` (anche per una dinamica tolta);
  `Performance::update`, identico al ricalcolo completo: dopo un cambio di dinamiche solo le velocity
  del tratto in cui la linea di una voce cambia davvero, dopo un cambio di cella una finestra di
  battute suonate ricalcolata e rimessa al suo posto; `PlayedBar::written_start`. Sulla partitura
  pesante: esecuzione completa ≈ 1,67 miliardi di istruzioni, una nota ≈ 1,24 milioni, una dinamica
  ≈ 2,11 milioni; `add_dynamic` dell'editor +8–9% per la lista nuova, accettato dal proprietario
  (`Bench-regression: accepted`). F1.13 completo.
- **F1.12** Da ADR-0024 §10: la tabella degli scostamenti per singolo passaggio (`NoteId`,
  indice del passaggio). Da ADR-0025: lo srotolamento segue le regole di §5; l'indice di passaggio
  di una battuta suonata è il numero di volte che la stessa battuta scritta è già stata suonata
  prima, da 0; le battute % si suonano dalle battute d'origine e le regole di passaggio filtrano le
  note.
  **Fatto il 2026-11-05 (PR #21):** prima del codice il proprietario ha approvato P1–P8, ora
  ADR-0025 §12: il percorso nel modulo `unroll` di `orchestra-playback`, dato ricavato; per ogni
  battuta suonata la battuta scritta, l'indice di passaggio, l'esecuzione della sezione, l'inizio
  esatto e uno spostamento nel tempo (esatto a meno di salti dentro un cambio graduale); dove si
  suona ogni battuta scritta; la battuta d'origine delle note per ogni rigo con %; le regole di
  passaggio come domanda per F1.13; ricostruzione intera quando cambiano forma, battute o mappe.
  La tabella degli scostamenti per passaggio passa a F1.14 (P6). Benchmark: ≈ 2,5 milioni di
  istruzioni per 900 battute suonate.
- **F1.13** (seguito) Da ADR-0024: pedale e glissando entrano con le regole di esecuzione che li
  usano. Da F1.09c: quale dinamica (o forcella) prevale per una voce quando valgono insieme una di
  strumento e una di voce; gli scostamenti di esecuzione sono salvati senza limiti, quindi le regole
  limitano i risultati (inizio ≥ 0, durata minima, scala della velocity).
- **F1.14** **Fatto il 2026-11-06 (ADR-0028 Accepted):** il proprietario ha approvato Q1–Q11 (soglia
  1/32 in tempo musicale con variante in ms; modo "solo esecuzione" come scelta consapevole; per la
  velocity il contrario: di default solo esecuzione, a scelta anche lo spartito). Diviso in F1.14b e
  F1.14c, con una PR unica alla fine.
- **F1.14b** **Fatto il 2026-11-06:** prima del codice il proprietario ha approvato T1–T7, ora
  ADR-0028 §10 (rifiutati oltre la soglia le teste in cui arriva una legatura e le note di gruppo
  irregolare; voci per passaggio lungo le catene come N8; la chiave distingue già le copie %; le
  voci delle note cancellate tolte dall'editor dopo ogni comando; millisecondi esatti tranne nei
  cambi graduali; una voce cambiata segna la cella della nota). Moduli `tolerance`, `pass_offsets`,
  `daw_command` di `core-model`; voci per passaggio nelle note suonate. Sulla partitura pesante un
  aggiornamento del trascinamento DAW ≈ 23 000 istruzioni; esecuzione completa +1,0%, aggiornamento
  di una nota +1,9%.
- **F1.14c** **Fatto il 2026-11-06:** prima del codice il proprietario ha approvato U1–U9, ora
  ADR-0028 §11 (il fattore delle regole passato da chi chiama, così il residuo è esatto; la stessa
  soglia per la durata, minimo un passo di griglia; la catena riscritta riusa gli ID dei pezzi;
  durate oltre la stanghetta ammesse; "sposta comunque" come `MoveMode::SplitAtBarlines`; T2
  confermato; una testa legata da prima si può ridimensionare; legature di portamento e
  abbellimenti `After` seguono la fine). `ResizePlayed` ≈ 74 000 istruzioni per aggiornamento sulla
  partitura pesante. F1.14 completo (PR #23).
- **F1.14** (testo originale): ❓ **Decisione:** valore di default della soglia (proposta di ADR-0004: 1/32) e se
  offrire da subito anche la variante in millisecondi. Da F1.12 (ADR-0025 §12 P6): qui entra nel
  modello la tabella degli scostamenti per singolo passaggio (ADR-0024 §10), con chiave (nota,
  indice di passaggio), con le sue modifiche elementari e il comando che la scrive; cancellare una
  nota ne cancella le voci.
- **F1.15** Candidati da confrontare con i tipi veri: testo strutturato (JSON, RON o simili,
  leggibile e confrontabile con git) eventualmente dentro un contenitore (zip o cartella) per i
  media. Criteri ADR-0010: robustezza, tempo di caricamento, confrontabilità, dimensione,
  facilità di migrazione, percorsi multipiattaforma. Rifiuto dei file "più nuovi" con errore
  tipizzato; politica sui campi sconosciuti. ❓ **Decisioni:** formato, contenitore, uso di
  `serde` (dipendenza da valutare), estensione del file (`.orchestra`? ❓). Da F1.08: un tetto al
  numero di tracce all'apertura di un file (righi e voci ce l'hanno già), e se rifiutare i cambi di
  metro e di tempo oltre l'ultima battuta (oggi nessuno lo controlla; vale anche per i comandi di
  F1.11).
  **Fatto il 2026-11-06 (ADR-0029 Accepted, D10 chiusa):** una prova in `spikes/fileformat/`
  sulle vere partiture normale e pesante (JSON compatto: 100 battute lette in ≈ 3 ms, la pesante in
  ≈ 100 ms; RON 3–5 volte più lento e senza migrazioni possibili sul suo albero; binario non
  confrontabile). Il proprietario ha approvato S1–S14: JSON con `serde`/`serde_json` solo in `io`;
  nella versione 1 un solo file JSON non compresso (la compressione potrà arrivare come versione
  nuova; audio e video accanto al file, mai dentro); un record per riga, scrittura deterministica;
  schema del file separato dal modello, caricamento con `Document::from_parts` che conserva gli ID e
  verifica tutto; versione come primo campo; frazioni canoniche, niente decimali; caricamento
  rigido (campi sconosciuti rifiutati); al massimo 1 024 tracce e 1 GiB; regola 13 contro i cambi di
  metro oltre l'ultima battuta; migrazioni sull'albero di `serde_json`; estensione `.orchestra`.
  Due precisazioni del proprietario: il lettore della versione 1 riconosce già zip e gzip come
  "fatto con un Orchestra più nuovo" (S5); si omettono solo le assenze strutturali, le impostazioni
  dell'utente si scrivono sempre (S6). Diviso in F1.15b e F1.15c, con una PR unica alla fine.
- **F1.15b** Implementa ADR-0029 S1–S10 e S12–S13 tranne i file golden: schema `io::project::v1`
  con il formattatore "un record per riga", API di lettura e `Document::from_parts` in
  `core-model`, `Display`/`FromStr` di `Rational` in `core-time`, regola 13 di `validate` e la
  verifica nel test di proprietà dei comandi, limiti, salvataggio sicuro (da provare anche su
  Windows in CI), test di andata e ritorno su documenti da sequenze casuali di comandi, test di
  robustezza (mai un panic), benchmark di salvataggio e caricamento, misura della dimensione del
  binario. Diviso in due parti (proprietario, 2026-11-06).
  **Parte 1 fatta il 2026-11-06:** prima del codice il proprietario ha approvato L1–L8, ora in
  ADR-0029 ("Precisions L1–L8"): `from_raw` su ogni tipo di ID e costruttori pubblici dei valori
  foglia; `DocumentParts` con `to_parts` e `from_parts`, che ripete la descrizione attraverso il
  builder con gli ID salvati (stesse verifiche del builder), controlla ID unici ed emessi e
  rifiuta gli elenchi fuori ordine; `MAX_TRACKS = 1024`; regola 13; `Display`/`FromStr` di
  `Rational`. Andata e ritorno verificata dopo ogni passo dei test di proprietà; 17 bug iniettati
  presi. Sulla partitura pesante `from_parts` ≈ 882 milioni di istruzioni, sulla normale ≈ 29
  milioni. Il test di proprietà dei comandi ha trovato un bug già presente del cambio di metro in
  `Rewrite` dopo una battuta libera (F1.11c): il proprietario ha scelto di ripristinare il metro
  precedente dopo la battuta libera, su un branch di correzione separato.
  **Parte 2:** il lato `io`: schema `io::project::v1` con il formattatore "un record per riga",
  lettura della versione prima del resto (con le firme zip e gzip), caricamento rigido, limite di
  1 GiB, salvataggio sicuro (provato anche su Windows in CI), test di andata e ritorno e di
  robustezza, benchmark di salvataggio e caricamento, misura della dimensione del binario.
  **Parte 2 fatta il 2026-11-06:** prima del codice il proprietario ha approvato I1–I10, ora in
  ADR-0029 ("Precisions I1–I10"): file diviso per proprietario, stringhe compatte solo per ciò che
  compare per nota, battute per ID anche nella mappa dei metri, API `save`/`load`/`to_bytes`/
  `from_bytes`, errori con riga e colonna, un record per riga. Modulo `project` di `orchestra-io`
  (codici di testo rigidi, schema serde, conversione, formattatore, intestazione), salvataggio
  sicuro con il primo modulo `platform/`; `serde` e `serde_json` nel workspace. Andata e ritorno
  byte per byte su un documento con ogni tipo di oggetto e sulle partiture normale e pesante,
  troncamenti e corruzioni senza panico, comandi casuali; 7 bug iniettati presi. Sulla macchina di
  sviluppo: 100 battute salvate in 5,5 ms e caricate in 11 ms (0,84 MB); la partitura pesante in
  168 e 396 ms (25,4 MB). Binario +0,6 MB. F1.15b completo.
- **F1.15c** Implementa ADR-0029 S11–S12: i file golden della versione 1 (uno per tipo di oggetto e
  uno con tutto), mai rigenerati; la catena delle migrazioni sull'albero, provata con una
  migrazione `v1 → v2` solo di test; poi la PR di tutto F1.15.
  **Fatto il 2026-11-06 (PR #25):** 15 file golden in `crates/io/tests/golden/v1/` (uno per tipo di
  oggetto salvato, un contatore esaurito, uno con tutto), costruiti anche da codice e confrontati
  con il documento e byte per byte; scritti una volta da un test ignorato che non sovrascrive mai;
  ogni troncamento rifiutato, JSON casuale senza panico. Catena delle migrazioni sull'albero JSON
  (vuota, provata con una catena di prova), `LoadError::Migration`. **Fatto dal proprietario il
  2026-11-07:** revisione a occhio dei file golden (ADR-0029 S12), nessun problema. F1.15 completo.
- **F1.16** La partitura di riferimento (100 battute × 10 righi) sarà riusata in F2 per il
  layout e in F3 per l'audio: si genera da codice, non si scrive a mano. Si parte dal generatore
  deterministico di `spikes/model/src/lib.rs` (partitura normale: 8 690 note; pesante: 255 776).
  Da F1.11b: il generatore riusabile sostituirà `crates/core-model/tests/common/score.rs`, oggi
  condiviso dal test della memoria, dal test della cronologia e dai benchmark dell'editor.
  **Deciso il 2026-11-07:** il generatore va in un crate nuovo solo per i test,
  `orchestra-test-scores` (ADR-0031, opzione B: ADR-0016 §6 vieta a `test-support` di dipendere da
  crate del workspace), con due preset: "plain" (l'attuale, documenti identici, così i benchmark
  esistenti restano confrontabili) in F1.16 e "full" in F1.16b. I benchmark di fase contano
  istruzioni; la misura in secondi dell'apertura resta in F2.23. Il "comando di prova" della tappa
  M1 si fa in F1.17.
  **F1.16 fatto il 2026-11-07 (PR #27):** il preset "plain" produce documenti identici a quelli
  di prima (impronte dei byte salvati uguali a quelle di `main`, fissate in
  `crates/test-scores/tests/plain_fingerprint.rs`; benchmark senza regressioni). **Per F1.16b:**
  un buon punto di partenza per il preset "full" è `crates/io/tests/common/everything.rs`, il
  documento con ogni tipo di oggetto usato dai file golden.
  **Scelte del 2026-11-07 per F1.16b/F1.16c:** partitura **plausibile** (sezioni con forma, frequenze
  realistiche) e non copertura densa; un test verifica che ogni tipo di oggetto di `everything.rs`
  compaia almeno una volta. Organico vero: 10 righi (pianoforte, flauto, oboe, clarinetto in Si♭,
  corno in Fa, violini I e II, viola, violoncello) e un'orchestra da 30 righi per la taglia pesante.
  Il preset "full" non è un file golden (può cambiare di proposito, ADR-0031 §7). I benchmark di
  fase stanno in `crates/test-scores/benches/phase.rs`; se il job dei benchmark si allunga troppo,
  la taglia pesante resta solo per l'apertura.
  **F1.16b fatto il 2026-11-07 (PR #29):** preset "full" in `crates/test-scores/src/full/` (piano del
  movimento, organico, note, segni); copertura di ogni tipo di oggetto di `everything.rs` (meno le tre
  eccezioni approvate: numerazione con lettere, ripresa della numerazione, ultima battuta corta), di
  ogni articolazione e ogni ornamento; srotolamento esatto (133 battute suonate per movimento, 1 330
  nella pesante); andata e ritorno; impronte fissate. **Densità realistica** (scelta A del
  proprietario): 3 939 teste di nota nella normale, 77 225 nella pesante, circa metà del "plain", che
  resta il caso denso. Il D.S. al Coda usa `RepeatsAfterJump::Play` perché con `Skip` i movimenti
  successivi perderebbero i ritornelli: osservazione annotata in ROADMAP per i documenti con più
  movimenti.
  **F1.16c fatto il 2026-11-07 (PR #31):** `crates/test-scores/benches/phase.rs` misura in istruzioni
  percorsi completi sul preset "full". Regola approvata dal proprietario: **1 istruzione per
  nanosecondo** sul portatile di riferimento (volutamente pessimista; i tempi veri in F2.23).
  Apertura di 100 battute (byte → documento → srotolamento → note suonate) 86,6 M ≈ 87 ms (budget
  1 s); pesante 2,03 G ≈ 2 s; modifica di una nota del violino I nel ritornello, dalla notazione o
  dalla DAW entro la soglia (comando + aggiornamenti) 0,38–0,49 M ≈ 0,5 ms (budget 50 ms);
  srotolamento 0,38 M; salvataggio 24 M. Una nota del pianoforte costa circa 1,6 M (finestra
  d'aggiornamento più grande). Il job dei benchmark in CI cresce di circa 2,5 minuti. F1.16
  completo; il prossimo passo è F1.18.

**"Primo ascolto" anticipato (approvato il 2026-10-30, D11):**
**F1.18:** esportazione **MIDI file** (crate `io`) degli eventi
di F1.13, così il proprietario può *ascoltare* il risultato del modello con qualunque lettore
già alla fine della Fase 1, senza aspettare il motore audio. Costo basso, verifica molto utile
delle regole di esecuzione. Si fa dopo F1.13 e prima della chiusura F1.17 (la tabella elenca i
passi per argomento, non in ordine stretto di esecuzione).

Da F1.13c (ADR-0027 §14): in **F1.18** si sceglie come scrivere il profilo d'attacco nel file MIDI
(MIDI 1.0 ha solo il CC11 per canale, che ne approssima la curva; MIDI 2.0 ha i controller per
nota), ed è il primo momento in cui Claude ricorda al proprietario di **tarare a orecchio** tutti i
tempi e i gradini di §14 (di nuovo con gli strumenti interni di F3).

**Tappa M1** (§7): "motore musicale senza interfaccia".

**F1.17 fatto il 2026-11-08 (PR #35): Fase 1 chiusa, tappa M1 raggiunta.** Il comando di prova è
un esempio di `orchestra-io`: `cargo run -p orchestra-io --example m1 -- CARTELLA [--heavy]
[--notes]`. Crea la partitura di riferimento "full" (normale, o pesante con `--heavy`), la salva
come `m1.orchestra`, la riapre e verifica che sia identica (musica, descrizione, byte), stampa
l'esecuzione srotolata (battute suonate con passaggio, sezione e secondi; note per strumento; ogni
nota con `--notes`) ed esporta `m1.mid`. Strumento per sviluppatori, guide invariate. Prossimo
passo: F2.01.

---

## 4. MEDIO TERMINE

### F2A — Fondamenta dell'interfaccia (25–35 sessioni)

**Obiettivo:** il nostro strato UI (ADR-0012) con temi (ADR-0014), testo, widget accessibili,
menu, dialoghi, screenshot headless. Alla fine c'è una finestra vera con menu, preferenze (tema,
lingua) e un documento vuoto, in 6 lingue, chiara e scura, navigabile da tastiera e da lettore di
schermo.

**Perché è così lunga:** abbiamo scelto di non usare un toolkit. È il prezzo (consapevole) del
controllo completo. Va fatta bene una volta sola.

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F2.01 | **Verifica e conferma dei mattoni** (ADR-0012 tabella): versioni, licenze, dimensioni di `winit`, `softbuffer`, `tiny-skia`, `skrifa`, `harfrust`, `accesskit`, `rfd`, `muda`; emendamento all'ADR | ✅ ADR-0036 | 1 |
| F2.02a | **Sistema dei temi** (ADR-0014): token semantici, dieci coppie chiaro/scuro incluse, temi personalizzati come modifiche, test WCAG 2.2 AA | ✅ ADR-0037 | 2 |
| F2.02b | **Rilevamento del tema di sistema** in `platform/` dietro un trait (ADR-0014 §5), con i cambi osservati a programma aperto | ✅ ADR-0038 | 1 |
| F2.03 | **Astrazione di disegno**: lista di comandi di disegno, backend `tiny-skia`, ridisegno delle sole aree cambiate; rendering headless su immagine | ✅ ADR-0039 | 2–3 |
| F2.04a | **Finestra, ciclo eventi, input**: `winit` + `softbuffer`, "modificatore primario" astratto, DPI/scala, ridimensionamento | ✅ ADR-0040 | 2 |
| F2.04b | **Tema di sistema su Windows e macOS** con `winit` (ADR-0038 S3) e **cambi di tema seguiti** a programma aperto: la callback del portale Linux portata al ciclo eventi con `EventLoopProxy` (ADR-0040 W3) | ✅ | 1 |
| F2.05 | **`cargo xtask screenshots`** (prima versione, tema scuro, en-US) | ✅ ADR-0041 | 1 |
| F2.06 | **Misura del percorso GPU** (`wgpu` + `vello`) fuori dalla sandbox, con il proprietario; ADR sul renderer GPU | ✅ ADR-0042 | 2–3 |
| F2.06b | **Backend `vello_cpu`** (ADR-0042): `Renderer` su `vello_cpu` con ridisegno delle sole aree cambiate e thread configurabili; collaudo con i test di pixel; benchmark e confronto con `tiny-skia`; poi `tiny-skia` esce e il PNG passa a `png` | ✅ ADR-0043 | 2 |
| F2.07 | **ADR del testo**: font inclusi, shaping, cache dei glifi, misure, a capo, token del testo; Bravura per i simboli musicali | ✅ ADR-0044 | 1 |
| F2.07b | **Testo, realizzazione** (ADR-0044): crate `orchestra-text` (font, shaping, `TextLayout`, a capo, metriche, profili), comando `GlyphRun`, cache dei glifi per `tiny-skia`, scelta della gamma a occhio, scelta fra `icu_segmenter` e il sottoinsieme nostro per l'a capo | ✅ | 2 |
| F2.07c | **Token del testo** (ADR-0044 TX11) nel tema, testo nella scena di riferimento e nel catalogo degli screenshot, budget del fotogramma intero (33 M istruzioni) | ✅ | 1 |
| F2.08 | **ADR: architettura dei widget** — modello ritenuto (retained) o immediato, algoritmo di layout, instradamento degli eventi, focus; nodo AccessKit per ogni widget fin dall'inizio | ✅ ADR-0045 | 2 |
| F2.09 | **Widget di base**: etichetta, pulsante, casella, interruttore, cursore, area scorrevole, divisore di pannelli | ⬜ | 3–4 |
| F2.10 | **Campo di testo**: modifica, selezione, appunti, IME, undo interno al campo | ⬜ | 3 |
| F2.11 | **Menu**: barra dei menu disegnata (Linux/Windows), menu contestuali, scorciatoie; `muda` su macOS | ⬜ | 2–3 |
| F2.11b | **Barra del titolo nostra** al posto delle decorazioni di `winit` su Wayland (ADR-0036 B6): ADR su dove usarla (solo Linux, o ovunque con i pulsanti nativi su macOS), trascinamento, ridimensionamento, doppio clic, temi, accessibilità | ⬜ | 1–2 |
| F2.12 | **Dialoghi**: file nativi (`rfd`), finestre modali nostre, finestra **Preferenze** (tema, lingua, opzioni di riproduzione) | ⬜ | 2 |
| F2.12b | **Editor dei temi** (ADR-0037 T7–T8): ogni token di colore e di forma, anteprima dal vivo, avvisi di contrasto (mai blocchi), ripristino del valore del tema di partenza, importazione ed esportazione di un tema | ⬜ | 2 |
| F2.13 | **i18n a runtime**: cambio lingua senza riavvio, rilevamento della lingua di sistema | ⬜ | 1 |
| F2.14 | **Salvataggio automatico e recupero dopo un crash** (collegato al file di progetto) | ⬜ | 1–2 |
| F2.15 | **Chiusura fase**: guide (primi capitoli con screenshot), documentazione, PR | ⬜ | 1–2 |

- **F2.01** **Fatto il 2026-11-09 (ADR-0036 Accepted):** prima di partire il proprietario ha
  approvato un ADR nuovo invece di modificare ADR-0012, solo `skrifa` (niente `ttf-parser`), appunti
  e tema di sistema rimandati ai loro passi, librerie native registrate subito. La prova
  `spikes/gui-blocks` mette tutti i mattoni in un binario: 9,6 MB senza simboli (1,5 MB di font),
  163 crate su Linux, 213 s di compilazione da zero; AccessKit da solo +3,2 MB, +69 crate, +76 s.
  `rfd` 0.17 (libdbus-1 caricata con dlopen) preferito alla 0.16 (`ashpd`/`zbus`, +0,9 MB); `muda`
  solo su macOS; le versioni doppie (quasi tutte da `winit` 0.30) vanno in `deny.toml` con la loro
  uscita, nel passo che le porta; prime voci in `docs/native-deps.md`. Il proprietario ha provato
  la finestra sul suo desktop. Decorazioni di `winit` su Wayland per ora, poi la nostra barra del
  titolo in F2.11b (proprietario).
- **F2.02a** **Fatto il 2026-11-10 (ADR-0037 Accepted, emenda ADR-0014 §1, §3, §7):** il
  proprietario ha visto dieci proposte (pagina di confronto "Temi di Orchestra": tre in bianco e
  nero puri, sette diverse) e ha scelto di **tenerle tutte** come scelta dell'utente, con **Studio**
  predefinito e la personalizzazione di ogni dettaglio. L'impostazione è coppia + modo (*Sistema*
  predefinito); senza risposta dal sistema si usa lo **scuro** (T9, proprietario). 24 token di
  colore e 5 di forma (angoli e linee per coppia, anello di focus 2 px, ombra), 31 coppie WCAG;
  temi personalizzati come tema di partenza + modifiche, con avviso e mai blocco sotto le soglie.
  Modulo `theme` di `orchestra-ui`, nessuna dipendenza nuova. Nello stesso passo, con l'ok del
  proprietario, corretti nelle guide i testi in ambra e in verde sotto WCAG (numeri dei capitoli,
  "Figura N", etichette dei riquadri).
- **F2.02b** **Fatto il 2026-11-10 (ADR-0038 Accepted):** prima di partire il proprietario ha
  chiesto di verificare i candidati, misurati nello spike `gui-blocks`. Su Linux il nostro codice
  su `zbus` 5.19 (la versione di AccessKit: nessun crate in più accanto a lui); libdbus-1 caricata a
  runtime scartata (circa 600 righe di `unsafe`), `dark-light` scartato (su macOS controlla ogni
  secondo). Windows e macOS con `winit` in F2.04, fino ad allora scuro (T9). Il proprietario ha
  provato la lettura sul suo desktop (il portale manda ogni cambio tre volte: si scartano le
  ripetizioni) e ha scelto: lo 0 del portale ("nessuna preferenza", lo stile chiaro predefinito di
  GNOME) vale **chiaro**; la prima lettura aspetta **al massimo 250 ms**. Durante il lavoro
  `cargo deny` ha segnalato tre crate di zbus 5 come non più mantenuti (RUSTSEC-2026-0370/0371/0372,
  non vulnerabilità): ignorati come fa AccessKit, finché AccessKit non passa a zbus 6 (S7,
  proprietario); `syn` 2 in `skip` come prevedeva ADR-0036 B7.
- **F2.03** **Fatto il 2026-11-10 (ADR-0039 Accepted):** il proprietario ha scelto una lista piatta
  divisa in segmenti, i token del tema nei comandi, l'ombra analitica subito e niente PNG prima di
  F2.05. Modulo `draw` di `orchestra-ui`, `tiny-skia` 0.12 entra nel workspace (7 crate piccoli).
  Misurando, l'ombra ha avuto bisogno di 12 campioni invece di 4 (errore massimo 0,84 gradini a
  8 bit invece di 3,5), più la formula esatta per i rettangoli: D6 emendato con l'ok del
  proprietario. Benchmark sulla scena di riferimento: modificare una nota 0,55 M istruzioni,
  fotogramma intero 84,6 M contro un budget di 33 M, l'86 % per le teste di nota disegnate come
  tracciati al posto dei glifi.
- **F2.04a** **Fatto il 2026-11-10 (ADR-0040 Accepted):** il proprietario ha diviso F2.04 in due,
  ha approvato la struttura (fotogrammi su richiesta, il nostro fotogramma completo, i nostri eventi
  di input con il modificatore primario astratto), l'ordine di ADR-0036 B9 provato ora con un punto
  d'aggancio vuoto e una finestra di prova (`cargo run -p orchestra-app`). Provandola su KDE Plasma
  (Wayland e X11) ha trovato tre errori, tutti corretti con un test: pixel di bordo sbagliati nel
  ridisegno parziale (l'area d'appoggio ora contiene i tracciati interi), la lentezza che quella
  prima correzione aveva causato (ombre calcolate solo dove serve, formula esatta lontano dagli
  angoli: 0,5 ms a movimento del puntatore), e la finestra nera su X11 dopo averla riaperta (copia
  intera dopo `Occluded(false)`). `tiny-skia` riportato al requisito caret (ADR-0036 B1).
- **F2.05** Da ADR-0039 D8: scrittura PNG con la feature `png-format` di `tiny-skia`, e il
  doppione di `miniz_oxide` in `skip` di `deny.toml` come prevede ADR-0036 B7.
  **Fatto il 2026-11-11 (PR #61, ADR-0041 Accepted):** il proprietario ha approvato S1–S8 e ha
  scelto la **metà scura di Studio** per gli screenshot (emenda ADR-0014 §7 e ADR-0037 T10). Un
  binario per sviluppatori, `orchestra-screenshots`, mai distribuito, disegna il catalogo di scene
  `SCENES` della libreria di `orchestra-app` a scala 2; i PNG vanno in `dist/screenshots/`, non si
  committano mai e `cargo xtask guides` li ricrea ogni volta (artefatto `screenshots` nella CI).
  Generati su Linux, controllati in memoria sulle tre piattaforme, niente immagini di riferimento.
  La finestra di prova non entra nelle guide (S7).
- **F2.04b** **Fatto il 2026-11-10 (PR #60, nessun ADR nuovo):** come previsto da ADR-0038 S3 e
  ADR-0040 W3. Il proprietario ha approvato **una sola strada** per i cambi su tutte le
  piattaforme: su Windows e macOS una sorgente **alimentata dal ciclo eventi** (`system_theme()`
  prima della finestra, il tema della finestra subito dopo, poi ogni `ThemeChanged`) con lo stesso
  `Reporter` del portale; la callback manda il nostro evento con `EventLoopProxy` e la finestra
  ridisegna tutto solo quando la metà della coppia cambia. API: `WindowOptions` riceve coppia e
  modo, `system_appearance_source()` diventa privata del crate. AccessKit usa ancora zbus 5.19:
  restano le eccezioni di ADR-0038 S7. Il proprietario l'ha provato su KDE Plasma (Wayland e X11):
  un rallentamento percepito, confrontato con `main` sulla stessa build, è risultato identico.
  Windows e macOS solo nella CI (ADR-0040 W9). Il colore della barra del titolo con un modo
  forzato è rimandato (nota in F2.12).
- **F2.06** viene presto apposta: è il rischio aperto più grosso della UI (la sandbox non ha GPU).
  Richiede che il proprietario esegua i test sul proprio schermo.
  **Fatto il 2026-11-11 (ADR-0042 Accepted):** il proprietario ha approvato G1–G8 con tutti e
  quattro i candidati e ha chiesto anche la misura a 4K. Lo spike `spikes/gpu-render` disegna la
  scena di riferimento con `tiny-skia` (il nostro `CpuRenderer`), `vello_cpu`, `vello` e
  `vello_hybrid`, a 1920 × 1080 e 3840 × 2160. La macchina di sviluppo è un Intel N150, più debole
  del riferimento; le misure GPU vanno prese fuori dalla sandbox (dentro c'è solo Vulkan
  software). A 1080p la GPU non batte `vello_cpu` su 3 thread (7 ms a fotogramma intero); a 4K
  `tiny-skia` scende a 68 ms, `vello_cpu` resta a 16 ms; `vello` su GL è il più veloce a 4K ma parte
  in 4,4 s a cache fredda. Prova a occhio del proprietario: `vello_cpu` il più fluido (≈ 105 fps),
  `vello_hybrid` un po' a scatti. Scelta **B** con i punti 1–6: niente GPU per ora, `vello_cpu`
  dietro la nostra lista di disegno con segmenti e ridisegno parziale (F2.06b), un core sempre
  libero per l'audio, GPU da rimisurare con lo stesso spike alle condizioni di ADR-0042 R2.
- **F2.06b** **Fatto il 2026-11-11 (PR #63, ADR-0043 Accepted, sostituisce ADR-0042 R1, R3, R5,
  R6):** il backend `vello_cpu` è stato scritto e ha passato i test di pixel, ma misurandolo nel
  prodotto si è visto che il confronto dello spike era falsato: `tiny-skia` ricalcolava l'ombra del
  pannello a ogni fotogramma. A parità di condizioni `tiny-skia` è veloce quanto `vello_cpu` con la
  precisione delle curve scelta dal proprietario, su un solo thread: **`tiny-skia` resta**,
  `vello_cpu` e i suoi thread escono, niente GPU resta valido (R2), e una regola per confrontare i
  renderer a parità di condizioni (K4). La copertura delle ombre va in cache (32, LRU). Chiudendo il
  passo il proprietario ha visto gli screenshot con e senza ombra e l'ha tenuta (nei temi chiari un
  menu sopra lo spartito si legge peggio senza); ha scartato l'ombra già colorata (13,0 ms, 1,57
  gradini dalla formula) e scelto di riempire l'interno a copertura piena con una tabella per
  canale, miscelando solo la fascia sfumata: fotogramma a 1080p con un dialogo 1400 × 800 da 31,6 a
  7,3 ms (3,8 senza ombra), `draw_shadowed_panel_frame` 17,7 M istruzioni. Il PNG resta con
  `tiny-skia` (ADR-0039 D8).
- **F2.08** **Fatto il 2026-11-12 (ADR-0045 Accepted, D12 chiusa):** dopo lo spike `spikes/widgets`
  (un clic su 198 widget: 0,17 M istruzioni ritenuto contro 27–33 M immediato), widget ritenuti in
  un'arena, layout nostro a due passate (pila, riga, colonna, griglia), eventi con cattura e
  risalita, focus con trappola modale, un nodo AccessKit e un segmento per widget, messaggi tipizzati
  all'applicazione. AccessKit resta su zbus 5.19: le eccezioni di `deny.toml` restano.
- **F2.08** (testo originale) ❓ **Decisioni:** retained vs immediato; layout nostro o crate esistente (es.
  `taffy`, da verificare); crate per gli appunti (es. `arboard`, da verificare) in F2.10.
- **F2.08** Da ADR-0038 S7 (era una nota di F2.04b): quando entra AccessKit, ricontrollare se è
  passato a zbus 6; in quel caso ci passiamo anche noi nello stesso passo e togliamo da
  `deny.toml` le tre eccezioni RustSec e `syn` 2.
- **F2.08** Da ADR-0040 W7: l'adapter di AccessKit si aggancia al punto di preparazione già pronto
  (finestra creata invisibile, preparata, poi mostrata), provato da un test in F2.04a.
- **F2.07b** **Parte 1 fatta il 2026-11-12** (branch `feature/f2-07-text`): il crate
  `orchestra-text` (cinque font inclusi, shaping con `harfrust`, a capo con `icu_segmenter` scelto
  dal proprietario dopo la misura, metriche, profili). Resta la parte 2: il comando `GlyphRun`, la
  cache dei glifi per `tiny-skia`, i test di pixel e di allocazione, gli screenshot per la scelta
  della gamma.
  **Fatto il 2026-11-12 (parte 2):** comando `GlyphRun` con archivio dei glifi per segmento, cache
  dei glifi da 8 MB per `tiny-skia`, ridisegno parziale identico al fotogramma intero byte per byte,
  nessuna allocazione con la cache calda. Contrasto del testo scelto dal proprietario a occhio:
  γ = 1,45 (ADR-0044, precisazioni). Costo misurato: 200 etichette a 1080p ≈ 15,5 M istruzioni con
  la cache calda, quasi tutte nella fusione dei pixel.
- **F2.07b** Da ADR-0043 (che sostituisce ADR-0042 R6): la cache dei glifi si costruisce per
  `tiny-skia`, come previsto prima di ADR-0042.
- **F2.07c** **Fatto il 2026-11-12** (scelte TD1–TD6, ADR-0044 precisazioni): token del testo nel
  tema (famiglie, dimensioni 11–22 px, pesi), `Faces::glyph_box`, scena di riferimento con teste di
  nota di Bravura, nomi degli strumenti e numeri di battuta: registrazione + fotogramma intero
  **18,3 M istruzioni** (budget 33 M rispettato; 84,7 M con i tracciati), modifica di una nota
  0,25 M. La fusione dei pixel non è stata toccata (TD6). La finestra dimostrativa mostra titolo,
  numeri di battuta e teste di nota di Bravura.
- **F2.07c** Da F2.07b (2026-11-12): se il fotogramma della scena di riferimento supera i 33 M
  istruzioni, il primo intervento da valutare è la fusione dei pixel dei glifi (oggi circa 25
  istruzioni a pixel, in virgola mobile), per esempio con l'aritmetica intera, **prima** di aprire
  un ADR sul budget.
- **F2.07c** Da ADR-0039 D10: con i glifi presi da una cache, il benchmark del fotogramma intero
  della scena di riferimento deve rientrare nei 33 M istruzioni (30 fps in software con la regola
  di F1.16c); se non ci rientra, si apre un ADR sul budget.
- **F2.07c** Da ADR-0037: qui si definiscono anche i **token del testo** (carattere fra i font
  inclusi, scala delle dimensioni, pesi), personalizzabili come gli altri.
- **F2.09** Diviso nel briefing del 2026-11-12 in quattro parti: **F2.09a** il nucleo (albero ritenuto,
  layout, instradamento, focus, modali, densità), **F2.09b** AccessKit con etichetta e pulsante,
  **F2.09c** casella, interruttore, cursore e la demo sostituita da un pannello vero, **F2.09d**
  area scorrevole, divisore e il benchmark sui 200 widget. **F2.09a fatto il 2026-11-12** (scelte
  WB1–WB11, precisazioni in ADR-0045: il puntatore risale come la tastiera, uno strato modale blocca
  il puntatore fuori, i valori di densità): un clic su 200 widget 63 000 istruzioni disegno
  compreso (obiettivo 0,5 M), un widget che cresce 116 000, un layout completo 0,86 M; 14 difetti
  iniettati, tutti presi.
- **F2.09d** Da F2.09a (scelta WB5): i rettangoli dei widget sono in pixel logici della finestra e i
  segmenti si registrano senza trasformazioni; l'area scorrevole deve decidere come spostare i
  figli (una trasformazione con clip per il suo contenuto, rettangoli e test del puntatore nello
  spazio del contenuto).
- **F2.09** Da ADR-0045: AccessKit (`accesskit` 0.25, `accesskit_winit` 0.34) entra nel workspace
  qui, agganciato al punto di preparazione della finestra (ADR-0040 W7). Obiettivo dai benchmark: un
  evento su un pannello di 200 widget sotto 0,5 M istruzioni, disegno compreso; la finestra
  dimostrativa viene sostituita dai widget veri.
- **F2.09** Da ADR-0037: qui si definisce la **densità** (spaziature compatte o comode) come token
  del tema.
- **F2.10** Da ADR-0045 WA8 (parte di D12): qui si sceglie il crate degli appunti; candidato
  `arboard` 3.7.0 (2026-10-15, MIT OR Apache-2.0), da misurare (dipendenze native, Wayland).
- **F2.10** Da F2.07b (scelta TB4): le funzioni per il cursore e il clic sul testo
  (`caret_x`, `hit_test`) si costruiscono sui cluster che `TextLayout` già registra (offset in byte
  UTF-8 per ogni glifo).
- **F2.12** Da ADR-0037: i nomi delle dieci coppie e le parole *Sistema / Chiaro / Scuro* nei sei
  cataloghi (con il glossario), e il **file delle impostazioni** con versione e migrazioni (CLAUDE.md
  §9) che salva coppia, modo e temi personalizzati.
- **F2.12** Approvato dal proprietario il 2026-11-08: nelle Preferenze anche le **opzioni di
  riproduzione**, cioè i valori oggi fissi di ADR-0027 §14–§15 (gradini degli accenti, tenute e
  ritorni dei profili d'attacco), con i valori tarati a orecchio come predefiniti.
- **F2.12** Rimandato dal proprietario in F2.04b (2026-11-10): con *Chiaro* o *Scuro* scelti a mano,
  la **barra del titolo di sistema** (Windows, e Wayland con le decorazioni di `winit`) segue
  ancora il sistema e può non accordarsi con la finestra. Si accorda con `Window::set_theme`, e si
  torna a `set_theme(None)` quando il modo ritorna *Sistema*, perché finché il tema è fissato su
  Windows e macOS non arrivano più gli avvisi di cambio. Se nel frattempo F2.11b porta la nostra
  barra del titolo, lì il problema sparisce su Linux.
- **F2.12b** Da F2.07c (2026-11-12): cambiare un token del testo (famiglia o dimensione) vuol dire
  impaginare di nuovo i testi e registrare di nuovo i loro segmenti, non solo ridisegnare
  (ADR-0044 TX9): l'editor dei temi deve farlo.
- **F2.15** Da ADR-0041: i primi screenshot veri nelle guide, una voce nuova in `SCENES` per ogni
  vista mostrata; con le viste vere si riconsiderano i confronti con immagini di riferimento
  (ADR-0041 S5).
- **F2.14** Proposta di Claude, **confermata dal proprietario il 2026-11-08** (D13 chiusa):
  anticiparlo qui perché un musicista che perde lavoro smette di usare il programma.

**Tappa M2A.**

### F2B — Notazione (20–30 sessioni)

**Obiettivo:** vedere e scrivere musica. Incisione incrementale (ADR-0007: mai ricalcolare tutta
la partitura), vista notazione, inserimento note da tastiera, tutto via comandi.

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F2.16 | **ADR: architettura dell'incisione** — cache per battuta/sistema, invalidazione dalle notifiche del modello, cosa produce `notation-layout` (glifi posizionati, nessun disegno) | ⬜ | 2 |
| F2.17 | **Metadati SMuFL di Bravura** e primitive (righi, chiavi, armature, indicazioni di metro) | ⬜ | 2 |
| F2.18 | **Un rigo, una voce**: spaziatura orizzontale, gambi, travature, alterazioni, punti, pause | ⬜ | 4–5 |
| F2.19 | **Legature (di valore e di portamento), gruppi irregolari, abbellimenti, articolazioni, dinamiche, forcelle, linee d'ottava** (ADR-0024 C1) | ⬜ | 3–4 |
| F2.20 | **Più voci, più righi, accollature, sistemi e interruzioni di riga** (incrementali) | ⬜ | 3–4 |
| F2.21 | **Vista notazione**: scorrimento, zoom, selezione, pagina bianca opzionale (ADR-0014) | ⬜ | 2–3 |
| F2.22 | **Inserimento note da tastiera** (step input) e modifica → comandi, undo/redo | ⬜ | 3 |
| F2.22b | **Inserimento di metri, cambi di metro, battute irregolari e libere, suddivisioni dei movimenti** → comandi | ⬜ | 1–2 |
| F2.23 | **Benchmark di fase**: re-layout ≤ 16 ms; 100 battute × 10 righi a ≥ 30 fps software / 60 fps GPU | ⬜ | 1 |
| F2.24 | **Chiusura fase**: guide, documentazione, PR | ⬜ | 1–2 |

- ❓ **Decisione F2.18:** algoritmo di spaziatura (esistono approcci noti in letteratura, es.
  quelli basati sulle durate e sulle "molle"): Claude fa una ricerca e propone.
- **F2.17** Da F2.07b (2026-11-12): i simboli SMuFL si cercano **solo** in Bravura
  (`Faces::glyph_for_char` con `FaceId::Bravura`), perché Inter usa gli stessi punti di codice
  dell'area a uso privato per glifi suoi.
- **F2.17** Da F2.07c (2026-11-12): i riquadri dei simboli vengono dai metadati SMuFL
  (`glyphBBoxes` di Bravura); `Faces::glyph_box` resta come riserva per i glifi che non li hanno. Mai
  il riquadro dell'intero font per i simboli: per Bravura è largo 2,75 em e alto 3,9 em.
- **F2.23** Da ADR-0042 R2: se un budget sfora anche con `vello_cpu`, si rimisura il percorso GPU
  con `spikes/gpu-render` sulle versioni del momento.
- **F2.23** Da F1.11c (ADR-0026 §11 R11): misurare in tempo reale sul portatile di riferimento il
  cambio di metro `Rewrite` di una sezione lunga della partitura pesante (60 battute × 41 voci,
  ≈ 118 milioni di istruzioni); se supera i 16 ms, ottimizzarlo (per esempio con uno spostamento in
  blocco degli eventi di un tratto di battute, che richiede un emendamento ad ADR-0026 §1).
  Da F1.13c: misurare lì anche `Performance::update` dopo una nota e dopo una dinamica sulla
  partitura pesante (≈ 1,2 e ≈ 2,1 milioni di istruzioni; sulla macchina di sviluppo ≈ 0,6–1 ms),
  con il budget di 1 ms di ADR-0027 §9.
  Da F1.15b: misurare lì l'apertura della partitura pesante (sulla macchina di sviluppo ≈ 0,4 s,
  2,2 miliardi di istruzioni; atteso ≈ 1 s), oltre alle 100 battute del budget di ADR-0007.
  Dalla revisione del codice (2026-11-08, PR #37–#42): i conteggi di istruzioni citati qui e nella
  chiusura di F1.16 sono cambiati (il caricamento e i comandi del modello sono più veloci) e non
  si riscrivono nel testo storico: si rimisurano qui. Si aggiunge un benchmark con molti
  abbellimenti e note lunghe, per decidere se le scansioni lineari di `playback` sugli abbellimenti
  (`previous_end`, `shorten_previous`) e sulle note che coprono più battute (`Summary::new`)
  vanno ottimizzate (punti D5 e D6 della revisione).
- **F2.22** Da ADR-0026 §11 (X4, X6, X7), comandi composti rimandati qui: svuotare le battute e
  mettere la ripetizione di battuta; ritrascrivere le note dopo un cambio di armatura; trasformare
  le note di un gruppo irregolare in valori normali.
- **F2.22b** Requisito del proprietario (2026-10-31, ADR-0019): quando si inserisce un metro misto
  (5/8, 7/8, 11/8…) l'interfaccia chiede **subito** quale raggruppamento usare, elencando le
  opzioni (5/8: 2+3, 3+2 o tenere 5; 7/8: 2+2+3, 3+2+2, 2+3+2 o tenere 7; ecc.), e ricorda che
  si potrà poi cambiare battuta per battuta. Comprende anche levare, battute non contate e
  cadenze libere (ADR-0018).
- Da ADR-0023: il simbolo da disegnare per le alterazioni microtonali (Stein-Zimmermann, frecce
  di Gould…); la semplificazione enarmonica delle armature trasposte (9 diesis mostrati come
  3 bemolle); i nomi delle note nelle sei lingue, con il Si tedesco «H» (glossario).
- Da ADR-0025: disegnare le lettere della numerazione nelle volte e i segni di ripetizione di
  battuta (%).
- Da F1.09c: l'interfaccia deve mostrare quando una battuta troppo piena porta un'indicazione di
  tempo sulla stanghetta (il modello la sposta lì senza segnalarlo).
- Fuori da questa fase (vedi §5): parti staccate, stampa/PDF, MusicXML.

**Tappa M2B.**

### F3 — Motore audio e timeline DAW (25–40 sessioni)

**Obiettivo:** ascoltare. Motore in tempo reale con le regole ferree di §5 di CLAUDE.md, vista
DAW srotolata collegata allo spartito, mixer essenziale, tracce audio.

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F3.01 | **ADR: architettura audio** — grafo, thread, code lock-free a capacità fissa, comunicazione UI ↔ motore tramite `app`, test "il thread audio non alloca" | ⬜ | 2 |
| F3.02 | **Backend null/offline** e rendering offline su file WAV | ⬜ | 2 |
| F3.03 | **Strumenti interni minimi** (servono al "progetto di riferimento" di ADR-0007) | ⬜ | 3–4 |
| F3.04 | **ADR + backend dei dispositivi reali** dietro trait (Linux prima; Windows e macOS compilati e testati in CI) | ⬜ | 3–4 |
| F3.05 | **Trasporto**: play/stop/loop, cursore di riproduzione, sincronia con le viste | ⬜ | 2 |
| F3.06 | **Vista DAW**: timeline srotolata, collegamento alle posizioni dello spartito, piano roll, soglia di tolleranza in azione, corsia della velocity | ⬜ | 4–5 |
| F3.07 | **Tracce audio**: importazione file, regioni/clip, cache delle forme d'onda, ancoraggio musicale o assoluto (ADR-0005 §5) | ⬜ | 3 |
| F3.08 | **Mixer essenziale**: volume, panorama, mute/solo, mandate, bus | ⬜ | 2–3 |
| F3.09 | **Ingresso MIDI** (tastiera per l'inserimento note) e **registrazione** audio/MIDI | ⬜ | 3 |
| F3.10 | **Progetto di riferimento + benchmark**: callback ≤ 2,6 ms, zero xrun in 10 minuti | ⬜ | 2 |
| F3.11 | **Chiusura fase** | ⬜ | 1–2 |

- **F3.01** **Requisito del proprietario (2026-11-08, ADR-0032):** **MIDI 2.0 di default
  all'interno del programma**: gli eventi tra la riproduzione e gli strumenti non scendono mai
  sotto la precisione del MIDI 2.0 (velocity a 16 bit, microtoni, controller per nota); verso i
  dispositivi MIDI esterni si usa UMP, con ripiego automatico su MIDI 1.0 se il dispositivo non
  lo supporta; per i plugin le espressioni per nota di CLAP e VST3. Il MIDI 1.0 resta solo
  nell'esportazione di file v0 (F1.18).
- **F3.03** Da ADR-0024 §10: i dati di controller per nota (ADR-0004) arrivano con il primo
  strumento che li usa.
- **F3.03** **Richiesta del proprietario (2026-11-08, F1.18):** con gli strumenti interni
  **rivedere a orecchio tutte le dinamiche** (livelli, accenti, profili d'attacco di ADR-0027 §14–§15),
  come nuovo giro di §15. Il file d'ascolto è quello dell'esempio
  `cargo run -p orchestra-io --example export_midi -- listening OUT.mid`.
- ❓ **Decisione F3.03:** quali strumenti interni (sintetizzatore semplice? campionatore SFZ/SF2?)
  e **quali suoni**, con licenze compatibili: è una scelta che pesa sull'esperienza d'uso.
- ❓ **Decisione F3.04:** crate esistente (es. `cpal`, da verificare) o backend nostri (ALSA /
  PipeWire / JACK, WASAPI, CoreAudio); su Linux il kernel del proprietario è PREEMPT_RT, utile
  per le misure ma non rappresentativo dell'utente medio.
- **F3.04** Da ADR-0042 R3: con il progetto di riferimento, zero xrun in 10 minuti **mentre la
  partitura scorre** con i thread di disegno di `vello_cpu` attivi; solo dopo questa misura il
  numero predefinito di thread di disegno è definitivo.
- ❓ **Decisione F3.07:** decodifica dei formati audio (es. `symphonia`, da verificare).
- La scelta del termine "region"/"clip" (glossario) va chiusa **prima di F3.06**.

**Tappa M3** = primo programma realmente usabile per un piccolo brano.

---

## 5. Filoni trasversali (si agganciano alle fasi, non hanno una fase propria)

| Filone | Quando (proposta) | Note |
|---|---|---|
| **Esportazione MIDI** | F1.18 (approvato) | Primo ascolto; SMF 1.0 (ADR-0032) |
| **Esportazione MIDI 2.0 su file** (MIDI Clip File) | con F3.04 | Tasto e bend di una nota da `MidiNote::nearest`, la regola unica di ADR-0033 (tasto inferiore a metà strada). Opzione accanto a SMF 1.0, che resta il default finché pochi programmi leggono il nuovo formato (proprietario, 2026-11-08; da riverificare lo stato dello standard) |
| **Importazione MIDI** | dopo F3.06 | ADR-0004: va *dedotto* l'intento (quantizzazione, grafia, voci): difficile, merita un ADR. Grafia secondo il contesto melodico (ADR-0023 §6) |
| **MusicXML import/export** | dopo F2B | Fondamentale per l'interoperabilità con gli altri software di notazione. **Da ricordare:** le armature non tradizionali (Bartók, klezmer, maqam e makam), rimandate dal proprietario, come secondo caso di `Key` con la loro regola di grafia (ADR-0023 §5); le armature diverse per rigo (ADR-0024 C6) |
| **Stampa ed esportazione PDF** | dopo F2B | Sempre in colori chiari (ADR-0014) |
| **Parti staccate e strumenti traspositori in vista** | dopo F2B | Nessun dato duplicato (ADR-0004) |
| **Accessibilità** | ogni passo UI | Nodi AccessKit fin dal primo widget; test con lettore di schermo a ogni tappa |
| **Guide utente** | ogni tappa | Capitoli e screenshot crescono con le funzioni visibili |
| **Glossario e traduzioni** | ogni testo nuovo | Revisione dei madrelingua quando possibile |
| **Windows e macOS** | sempre in CI; prove su macchine reali a ogni tappa | ❓ Il proprietario ha accesso a macchine Windows/macOS per le prove manuali? |
| **Prestazioni** | ogni percorso caldo | Benchmark in CI, regressioni visibili nelle PR. In F2.23: un benchmark con molti abbellimenti e note lunghe, per decidere D5/D6 della revisione del 2026-11-08 |
| **Formato del file** | ogni cambio del modello | Migrazione + golden file (ADR-0010) |

---

## 6. LUNGO TERMINE

Dettaglio volutamente minore: si espande alla chiusura di F3, con quello che avremo imparato.

### F4 — Plugin (ADR-0011) (stima da rifare a fine F3)

1. ADR: protocollo dei bridge, segnalazione in tempo reale con attesa limitata, politica di
   riavvio; benchmark del round trip contro il budget di ADR-0007 (il p99 dello spike, 1–2 ms,
   va abbassato).
2. Scansione in processo separato, cache dei risultati.
3. Host **CLAP** (primo: API più pulita) → **VST3** → **LV2** (con voce in
   `docs/native-deps.md` per `lilv`) → **AU** su macOS.
4. Editor dei plugin in finestre separate possedute dal bridge.
5. Parametri, automazione, stato dei plugin nel file di progetto (cambio di formato → migrazione).
6. Recupero dai crash e dai blocchi dei plugin, con interfaccia chiara per l'utente.
7. Riprendere i problemi dello spike: crash LSP CLAP/VST3, uscita VST3 silenziosa.

### F5 — Video

1. ❓ ADR sulla decodifica (probabile libreria C, es. FFmpeg → `docs/native-deps.md`, licenze da
   valutare con attenzione rispetto alla GPL).
2. Tracce video sulla stessa timeline, file proxy a bassa risoluzione (ADR-0007), sincronia al
   fotogramma con frame rate razionali (ADR-0005).
3. Montaggio semplice: tagli, spostamenti, dissolvenze base.
4. Esportazione audio+video.

### F6 — Prima alpha pubblica

1. Pacchetti: Linux (❓ Flatpak / AppImage / pacchetti di distribuzione), Windows (installer),
   macOS (app firmata e notarizzata: certificati **mai** nel repository).
2. Icone generate dal logo (ADR-0014), sostituibilità del logo per le distribuzioni.
3. Sito, guida per chi vuole contribuire, modelli per segnalare bug.
4. Politica di versionamento dell'applicazione (separata dalla versione del formato, ADR-0010).

### Oltre (idee da valutare, nessun impegno)

Da ADR-0027 (rimandi decisi, da collocare): la **realizzazione degli ornamenti** (trillo,
gruppetti, mordenti, tremolo: nota ausiliaria dall'armatura e dall'alterazione sopra il segno,
velocità); le **corone** come tempo inserito nella linea del tempo srotolata, con il suono delle
note allungato e mai un tempo più lento (emendamento di ADR-0025 §12); le **intonazioni**
(temperamenti, intonazione naturale, segni di comma) dopo F3.04; i valori di articolazioni e
dinamiche come opzioni modificabili nella finestra Preferenze (F2.12), compresi i tempi e i gradini
del profilo d'attacco di ADR-0027 §14.

Da collocare (ADR-0024 C9): tremolo tra due note, testo cantato (sillabe, strofe, allineamento),
sigle degli accordi, diteggiature e indicazioni di tecnica. Da ADR-0025: le ripetizioni di
movimento (un segno che ripete un movimento) e più movimenti in un solo file ("l'inizio della
partitura" dei salti diventerà allora "l'inizio del movimento").

Scripting, tema ad alto contrasto, notazioni speciali (tablature, percussioni, notazione antica),
collaborazione, supporto microtonale completo, internazionalizzazione oltre le 6 lingue.

---

## 7. Tappe (milestone) — cosa il proprietario può verificare con mano

| Tappa | Fine di | Verifica concreta |
|---|---|---|
| **M1** | F1 | Un comando di prova crea una partitura di riferimento con ritornelli, la salva, la riapre identica; si stampa l'esecuzione srotolata; si ascolta il file MIDI esportato (F1.18) |
| **M2A** | F2A | Si apre Orchestra: finestra, menu, Preferenze; tema chiaro/scuro/di sistema; 6 lingue senza riavvio; navigazione da tastiera; screenshot generati da `cargo xtask screenshots` |
| **M2B** | F2B | Si scrive da tastiera un brano per pianoforte a due voci con legature, gruppi irregolari e dinamiche, si annulla/ripete, si salva e si riapre; scorrimento fluido della partitura di riferimento |
| **M3** | F3 | Lo stesso brano si ascolta, si vede nella vista DAW srotolata, si sposta una nota con microtiming senza cambiare lo spartito, si aggiunge una traccia audio; zero xrun in 10 minuti |
| **M4** | F4 | Un plugin CLAP/VST3/LV2 suona e si apre il suo editor; un plugin che va in crash non chiude Orchestra |
| **M5** | F5 | Un video sulla timeline, sincronizzato al fotogramma, esportato con la musica |
| **M6** | F6 | Pacchetti installabili sulle tre piattaforme |

---

## 8. Decisioni aperte (indice)

Elenco unico delle ❓ della scaletta, con il passo entro cui vanno prese. Si aggiorna a ogni
modifica.

| # | Decisione | Entro | Stato |
|---|---|---|---|
| D01 | Lint pedantic da tenere | F0.16 | **chiusa** 2026-10-30: vedi F0.16 |
| D02 | Tipo razionale: nostro o crate esistente; politica di overflow | F1.01 | **chiusa** 2026-10-30: ADR-0015 |
| D03 | Strumenti di benchmark e di test di proprietà | F1.02 | **chiusa** 2026-10-31: ADR-0016 |
| D04 | Metri non binari (es. 4/3): sì o no | F1.04 | chiusa: sì nel tipo (ADR-0018) |
| D05 | Cambi di tempo graduali: lineari in quale grandezza | F1.05 | chiusa: nel tempo reale (ADR-0020) |
| D06 | Architettura del modello (ID, strutture, battute memorizzate o derivate) | F1.06 | **chiusa** 2026-11-01: ADR-0022 |
| D07 | Microtoni nel tipo di altezza fin da subito | F1.07 | **chiusa** 2026-11-01: ADR-0023 (alterazione razionale in semitoni; l'intonazione è rimandata) |
| D08 | Limite della cronologia di undo; fusione dei comandi continui | F1.11 | **chiusa** 2026-11-04: ADR-0026 (memoria, 128 MB; gesti espliciti) |
| D09 | Soglia di tolleranza di default; variante in ms | F1.14 | **chiusa** 2026-11-06: ADR-0028 (1/32, variante in ms) |
| D10 | Formato concreto del file, contenitore, `serde`, estensione | F1.15 | **chiusa** 2026-11-06: ADR-0029 (JSON semplice, `serde` solo in `io`, `.orchestra`) |
| D11 | Esportazione MIDI anticipata (F1.18) | fine F1-B | **chiusa** 2026-10-30: sì |
| D12 | Widget retained o immediato; layout; appunti | F2.08 | **chiusa** 2026-11-12: ADR-0045 (ritenuti, layout nostro); appunti in F2.10 |
| D13 | Salvataggio automatico già in F2A | F2.14 | **chiusa** 2026-11-08: sì |
| D14 | Algoritmo di spaziatura orizzontale | F2.18 | aperta |
| D15 | Strumenti interni e suoni | F3.03 | aperta |
| D16 | Backend audio: crate esistente o nostri | F3.04 | aperta |
| D17 | "Region" o "clip" | F3.06 | aperta (esiste già nella ROADMAP) |
| D18 | Accesso a macchine Windows/macOS per prove manuali | F2A | aperta |
| D19 | Lingua della scaletta: italiano (attuale) o inglese come gli altri documenti di progetto | F0.17 | **chiusa** 2026-10-30: italiano |
| D20 | Ordine alternativo: incisione headless (F2B in PNG) **prima** delle fondamenta UI (F2A) | F0.17 | **chiusa** 2026-10-30: F2A poi F2B |
| D21 | Cambio di metro: mantenere le battute o riscrivere la musica | F1.11 | **chiusa** 2026-11-04: ADR-0026 (riscrivere di default, mantenere come modalità) |
| D22 | Intonazione (temperamenti, intonazione naturale, segni di comma): in quale passo | F1.13 | **chiusa** 2026-11-05: ADR-0027 (12-TET con microtoni esatti ora, intonazioni dopo F3.04) |

Nota su **D20**: `notation-layout` non dipende da `ui`, quindi in teoria si potrebbe incidere la
musica su immagini PNG (headless) subito dopo la Fase 1, vedendo prima i risultati musicali.
Contro: servono comunque testo e font (F2.07), e ADR-0012/ADR-0014 parlano di "UI foundation
della Fase 2 che parte dai temi" — invertire l'ordine non cambia la sostanza di quegli ADR (temi
prima del primo widget), ma va approvato esplicitamente. Decisione del proprietario (2026-10-30): **F2A poi F2B**, come
raccomandato.

---

## 9. Registro delle modifiche alla scaletta

| Data | Versione | Modifica | Approvata da |
|---|---|---|---|
| 2026-10-30 | 0.1 | Prima proposta | — |
| 2026-10-30 | 0.2 | Scaletta approvata; D11 (sì a F1.18), D19 (italiano), D20 (F2A poi F2B) chiuse; F0.17 fatto; protocollo aggiunto a `CLAUDE.md` §0 | proprietario |
| 2026-10-30 | 0.3 | F0.15 e F0.16 fatti, D01 chiusa | proprietario |
| 2026-10-30 | 0.4 | F1.01 fatto (ADR-0015 Accepted), D02 chiusa; nota sull'esattezza dei cambi di tempo graduali in F1.05 | proprietario |
| 2026-10-31 | 0.5 | F1.02 fatto (ADR-0016 Accepted), D03 chiusa; F1.03 diviso in F1.03 (tipo + oracolo `i128`) e F1.03b (benchmark + confronto con `main`) | proprietario |
| 2026-10-31 | 0.6 | F1.03 fatto (PR #8) con le scelte d'interfaccia approvate; note su F1.03b (benchmark di ADR-0015 §9, caso oltre i 64 bit) e F1.04 (tipi specifici, eventuale `rem`) | proprietario |
| 2026-10-31 | 0.7 | F1.03b fatto (ADR-0017 Accepted, PR #9); regole dei benchmark: lotti e nome nuovo se cambia significato | proprietario |
| 2026-10-31 | 0.8 | F1.04 fatto (ADR-0018 Accepted, tipi `Position`/`Duration`), D04 chiusa; nuovo F1.04b (la mappa) con le cadenze libere richieste dal proprietario; note per F1.06 | proprietario |
| 2026-10-31 | 0.9 | F1.04b fatto (ADR-0019 Accepted, PR #11): suddivisione dei movimenti separata dal metro scritto; nuovo F2.22b (inserimento dei metri) con il requisito dei metri misti | proprietario |
| 2026-10-31 | 0.10 | F1.05 fatto (ADR-0020 Accepted, PR #12), D05 chiusa; nuovo F1.05b (la mappa dei tempi) | proprietario |
| 2026-11-01 | 0.11 | ADR-0021 Accepted (istanti di confine su una griglia fissa, posizioni arrotondate dentro i cambi graduali, tempo di partenza facoltativo); F1.05b in corso, prima parte fatta (posizioni ⇄ secondi) | proprietario |
| 2026-11-01 | 0.12 | F1.05b fatto (PR #13): indici esatti di campione e di fotogramma, nessun ripiego misurato; F1-A completata | proprietario |
| 2026-11-01 | 0.13 | F1.06 fatto (ADR-0022 Accepted), D06 chiusa; nuova D21 (comportamento del cambio di metro) per F1.11; vincoli di ADR-0022 annotati in F1.08, F1.09, F1.11, F1.16 | proprietario |
| 2026-11-01 | 0.14 | F1.07 fatto (ADR-0023 Accepted), D07 chiusa; nuova D22 (intonazione) per F1.13; armature non tradizionali da ricordare nel filone MusicXML, grafia dal contesto nel filone importazione MIDI, note di ADR-0023 in F2B | proprietario |
| 2026-11-02 | 0.15 | F1.08 fatto (PR #16): builder pubblico, una voce per rigo, tetti di 16 righi e 256 voci per strumento; chiave e armatura spostate in F1.09; tetto delle tracce e cambi oltre l'ultima battuta annotati in F1.15 | proprietario |
| 2026-11-02 | 0.16 | F1.09 fatto (ADR-0024 Accepted, emenda ADR-0022 §5 e §6); nuovi F1.09b e F1.09c; F1.11 non sposta più i cambi di tempo; rimandi annotati in F1.12, F1.13, F2.19, F3.03, filone MusicXML e "Oltre" | proprietario |
| 2026-11-02 | 0.17 | F1.09b fatto (eventi, gruppi irregolari, legature di valore, chiavi, armature, `validate`) | proprietario |
| 2026-11-03 | 0.18 | F1.09c fatto (PR #17): dinamiche, forcelle, indicazioni di tempo con la mappa ricavata, limite alla stanghetta, tetto di memoria; note per F1.11, F1.13 e F2B | proprietario |
| 2026-11-03 | 0.19 | F1.10 fatto (ADR-0025 Accepted, uno stile di numerazione per documento); nuovo F1.10b; note per F1.11, F1.12, F2B e "Oltre" | proprietario |
| 2026-11-03 | 0.20 | F1.10b fatto (PR #18): ritornelli, volte, stile di numerazione, segni e salti, ripetizioni di battuta, regole di passaggio; ADR-0025 corretto (§3) e precisato (§8); F1-B resta con F1.11 | proprietario |
| 2026-11-04 | 0.21 | F1.11 fatto (ADR-0026 Accepted), D08 e D21 chiuse; F1.11 diviso in F1.11b (il motore dei comandi) e F1.11c (il cambio di metro) | proprietario |
| 2026-11-04 | 0.22 | F1.11b in corso, parte 1 fatta (modifiche elementari, editor, cronologia); confermate le scelte d'interfaccia; benchmark dell'editor spostati nella parte 2, modifica in blocco della mappa dei metri in F1.11c | proprietario |
| 2026-11-04 | 0.23 | F1.11b: parte 2 divisa in 2a e 2b, parte 2a fatta (comandi su eventi e battute, gesti, verifica locale); precisazioni V1–V9 e W1–W6 in ADR-0026 §11, con la partenza dal tempo d'arrivo per un cambio graduale tagliato all'inizio; parte 2b definita | proprietario |
| 2026-11-05 | 0.24 | F1.11b fatto (PR #19): parte 2b con le precisioni X1–X11 in ADR-0026 §11; note per F1.16 (generatore condiviso) e F2.22 (comandi composti rimandati) | proprietario |
| 2026-11-05 | 0.25 | F1.11c diviso in due parti, parte 1 fatta (`KeepBars`, precisioni Y1–Y8 in ADR-0026 §11, Y8 corregge `InsertBars`); parte 2 definita | proprietario |
| 2026-11-05 | 0.26 | F1.11c: parte 2 divisa in 2a e 2b, parte 2a fatta (modifica in blocco della mappa dei metri, note di gruppo irregolare divise alla stanghetta; precisioni Z1–Z5 in ADR-0026 §11); parte 2b definita | proprietario |
| 2026-11-05 | 0.27 | F1.11c fatto (PR #20): parte 2b, `Rewrite`, con le precisioni R1–R11 in ADR-0026 §11 e il budget accettato da misurare in F2.23; F1-B completata, si passa a F1-C | proprietario |
| 2026-11-05 | 0.28 | F1.12 fatto (PR #21): srotolamento della forma, precisioni P1–P8 in ADR-0025 §12; la tabella degli scostamenti per passaggio passa a F1.14 | proprietario |
| 2026-11-05 | 0.29 | F1.13 fatto (ADR-0027 Accepted), D22 chiusa; nuovi F1.13b e F1.13c; rimandi di ADR-0027 (ornamenti, corone, intonazioni, opzioni di esecuzione) in "Oltre" | proprietario |
| 2026-11-05 | 0.30 | F1.13b fatto: le note suonate, precisioni N1–N10 in ADR-0027 §13 | proprietario |
| 2026-11-05 | 0.31 | F1.13c diviso in due parti, parte 1 fatta (precisioni H1–H13 ed emendamento H14–H17 in ADR-0027 §14: profilo d'attacco); note per F1.18 (codifica del profilo, taratura a orecchio) e F2.12 | proprietario |
| 2026-11-05 | 0.32 | F1.13c fatto (parte 2: aggiornamento incrementale, benchmark), F1.13 completo; misura in tempo reale dell'aggiornamento delle note suonate in F2.23 | proprietario |
| 2026-11-06 | 0.33 | F1.14 fatto (ADR-0028 Accepted), D09 chiusa; nuovi F1.14b e F1.14c, PR unica per F1.14 | proprietario |
| 2026-11-06 | 0.34 | F1.14b fatto (precisioni T1–T7 in ADR-0028 §10); note per F1.14c (T2, "sposta comunque") | proprietario |
| 2026-11-06 | 0.35 | F1.14c fatto (precisioni U1–U9 in ADR-0028 §11), F1.14 completo (PR #23) | proprietario |
| 2026-11-06 | 0.36 | F1.15 fatto (ADR-0029 Accepted, precisazioni S5 e S6 del proprietario), D10 chiusa; nuovi F1.15b e F1.15c, PR unica per F1.15 | proprietario |
| 2026-11-06 | 0.37 | F1.15b diviso in due parti, parte 1 fatta (precisioni L1–L8 in ADR-0029); bug del `Rewrite` dopo una battuta libera da correggere su un branch separato (opzione A) | proprietario |
| 2026-11-06 | 0.38 | F1.15b fatto (parte 2: precisioni I1–I10 in ADR-0029); misura dell'apertura della partitura pesante in F2.23 | proprietario |
| 2026-11-06 | 0.39 | F1.15c fatto, F1.15 completo (PR #25); revisione dei file golden da parte del proprietario | proprietario |
| 2026-11-07 | 0.40 | F1.16 diviso in F1.16 (crate `test-scores`, ADR-0031 opzione B, preset "plain") e F1.16b (preset "full", benchmark di fase); comando di prova di M1 in F1.17 | proprietario |
| 2026-11-07 | 0.41 | F1.16 fatto (PR #27, ADR-0031): crate `test-scores` con il preset "plain"; nota per F1.16b (`everything.rs` come punto di partenza); revisione dei file golden fatta dal proprietario, nessun problema | proprietario |
| 2026-11-07 | 0.42 | F1.16b diviso in F1.16b (preset "full" plausibile, organico vero) e F1.16c (benchmark di fase in `test-scores`) | proprietario |
| 2026-11-07 | 0.43 | F1.16b fatto (PR #29): preset "full" plausibile, densità realistica (scelta A), `Play` sul D.S.; osservazione sui ritornelli dopo un salto in ROADMAP | proprietario |
| 2026-11-07 | 0.44 | F1.16c fatto (PR #31): benchmark di fase sul preset "full", regola 1 istruzione/ns; F1.16 completo, prossimo passo F1.18 | proprietario |
| 2026-11-08 | 0.45 | F1.18 avviato (bozza di ADR-0032, scelte E1–E12 approvate); requisito "MIDI 2.0 di default all'interno" in F3.01; esportazione MIDI 2.0 su file come filone con F3.04 | proprietario |
| 2026-11-08 | 0.46 | F1.18 fatto (PR #33): esportazione MIDI 1.0, profili d'attacco tarati a orecchio (ADR-0027 §15, giri 1–2); in F3.03 la revisione a orecchio di tutte le dinamiche | proprietario |
| 2026-11-08 | 0.47 | F1.17 fatto (PR #35): Fase 1 chiusa, tappa M1 raggiunta (comando `m1`); D13 chiusa (F2.14 confermato); opzioni di riproduzione nelle Preferenze di F2.12; prossimo passo F2.01 | proprietario |
| 2026-11-08 | 0.48 | Revisione del codice dopo M1 (PR #37–#42): duplicazioni e codice inutile rimossi, ADR-0033 (tasto inferiore a metà strada, emenda ADR-0032 E9), indice delle voci nel documento; in F2.23 la rimisura dei conteggi e un benchmark con abbellimenti e note lunghe (D5, D6); prossimo passo resta F2.01 | proprietario |
| 2026-11-09 | 0.49 | Seconda revisione del codice (PR #44–#49): pratiche poco raccomandabili corrette (nomi temporanei unici nel salvataggio, conversioni "impossibili" controllate in debug, costanti razionali verificate in compilazione, errori con causa e posizione, `MigrationError` tipizzato, timeout della CI, permessi e link simbolici nel salvataggio); ADR-0034 (moduli privati, un solo percorso per elemento, errori esaustivi); nei commenti le sigle delle decisioni si scrivono con il loro ADR; funzioni lunghe divise nei loro passi; prossimo passo resta F2.01 | proprietario |
| 2026-11-09 | 0.50 | F2.01 fatto (ADR-0036 Accepted): mattoni dell'interfaccia verificati e misurati insieme; nuovo F2.11b (la nostra barra del titolo, con un suo ADR) | proprietario |
| 2026-11-10 | 0.51 | F2.02 diviso in F2.02a (fatto, ADR-0037: dieci coppie di temi, Studio predefinito, personalizzazione) e F2.02b (tema di sistema); nuovo F2.12b (editor dei temi); note in F2.07 (token del testo), F2.09 (densità), F2.12 (nomi dei temi, file delle impostazioni); ambra delle guide corretta | proprietario |
| 2026-11-10 | 0.52 | F2.02b fatto (ADR-0038 Accepted: portale XDG con `zbus` 5.19 su Linux, `winit` su Windows e macOS in F2.04, lo 0 del portale vale chiaro, prima lettura entro 250 ms, avvisi RustSec di zbus 5 ignorati con AccessKit); nota in F2.04 (tema di sistema con `winit`, passaggio a zbus 6 con AccessKit); prossimo passo F2.03 | proprietario |
| 2026-11-10 | 0.53 | F2.03 fatto (ADR-0039 Accepted, D6 emendato: ombra a 12 campioni misurati); note in F2.04 (presentazione con `softbuffer`), F2.05 (PNG e `miniz_oxide`), F2.07 (budget del fotogramma intero con la cache dei glifi); prossimo passo F2.04 | proprietario |
| 2026-11-10 | 0.54 | F2.04 diviso in F2.04a (fatto, ADR-0040: finestra, ciclo eventi, input; tre errori trovati dal proprietario e corretti) e F2.04b (tema di sistema su Windows e macOS, cambi di tema seguiti); note di F2.04 spostate in F2.04b; nota in F2.08 (AccessKit nel punto di preparazione); prossimo passo F2.04b | proprietario |
| 2026-11-10 | 0.55 | F2.04b fatto (PR #60): tema di sistema su Windows e macOS con `winit`, cambi seguiti a programma aperto su una sola strada (`EventLoopProxy`); nota di zbus 6 spostata in F2.08; nota in F2.12 (barra del titolo con un modo forzato); prossimo passo F2.05 | proprietario |
| 2026-11-11 | 0.56 | F2.05 fatto (PR #61, ADR-0041 Accepted: screenshot senza finestra da un binario per sviluppatori, catalogo di scene, metà scura di Studio che emenda ADR-0014 §7 e ADR-0037 T10, mai committati); nota in F2.15 (primi screenshot veri, immagini di riferimento); prossimo passo F2.06 | proprietario |
| 2026-11-11 | 0.57 | F2.06 fatto (ADR-0042 Accepted: GPU misurata a 1080p e 4K, niente GPU per ora, `vello_cpu` dietro la nostra lista di disegno); nuovo F2.06b (backend `vello_cpu`); note in F2.07 (glifi per `vello_cpu`), F2.23 (rimisura della GPU), F3.04 (xrun con i thread di disegno); prossimo passo F2.06b | proprietario |
| 2026-11-11 | 0.58 | F2.06b fatto (PR #63, ADR-0043 Accepted: `tiny-skia` resta, `vello_cpu` scartato dopo un confronto falsato, ombre in cache con l'interno riempito da tabella); nota in F2.07 (cache dei glifi per `tiny-skia`); prossimo passo F2.07 | proprietario |
| 2026-11-12 | 0.59 | F2.07 diventa l'ADR del testo (fatto, ADR-0044 Accepted: crate `orchestra-text`, cinque font inclusi, cache dei glifi da 8 MB, `GlyphRun`, a capo secondo UAX #14, token del testo); nuovi F2.07b (realizzazione) e F2.07c (token, scena di riferimento, budget); note di F2.07 spostate in F2.07b e F2.07c; un'unica PR per l'intero passo; prossimo passo F2.07b | proprietario |
| 2026-11-12 | 0.60 | F2.07b fatto (glyph run e cache dei glifi, contrasto del testo γ = 1,45 scelto dal proprietario); note in F2.07b (risultati), F2.10 (cursore sui cluster), F2.17 (SMuFL solo in Bravura), F2.07c (fusione dei pixel prima di un ADR sul budget); prossimo passo F2.07c | proprietario |
| 2026-11-12 | 0.61 | F2.07c fatto e con esso F2.07 (token del testo, `Faces::glyph_box`, scena di riferimento con glifi a 18,3 M istruzioni entro il budget di 33 M); note in F2.07c (risultati), F2.17 (riquadri SMuFL), F2.12b (cambiare un token del testo reimpagina); prossimo passo F2.08 | proprietario |
| 2026-11-12 | 0.62 | F2.08 fatto (ADR-0045 Accepted, D12 chiusa: widget ritenuti, layout nostro, eventi, focus, AccessKit per widget); note in F2.09 (AccessKit, obiettivo 0,5 M per evento) e F2.10 (appunti, `arboard`); prossimo passo F2.09 | proprietario |
| 2026-11-12 | 0.63 | F2.09 diviso in F2.09a–d; F2.09a fatto (nucleo dei widget, densità; un clic su 200 widget 63 000 istruzioni); nota in F2.09d (scorrimento e coordinate); prossimo passo F2.09b | proprietario |
