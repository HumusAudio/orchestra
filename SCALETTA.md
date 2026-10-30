# Scaletta di lavoro — Orchestra

> **Stato di questo documento: v0.4, approvata dal proprietario il 2026-10-30.**
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
| F1.02 | **Infrastruttura di test e benchmark**: `cargo xtask bench`, test di proprietà, contatore di allocazioni | ⬜ | 1–2 |
| F1.03 | **Implementazione del tipo razionale** + test di proprietà + benchmark | ⬜ | 1–2 |
| F1.04 | **Mappa delle misure (metri)**: battute/movimenti ↔ posizione razionale | ⬜ | 2 |
| F1.05 | **Mappa dei tempi (tempo map)**: posizione ↔ secondi ↔ campioni ↔ fotogrammi, in entrambe le direzioni | ⬜ | 2–3 |

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
- **F1.04** Casi da coprire con test: battuta in levare (anacrusi), cambi di metro, metri
  composti e additivi (3+2+2/8), metri non binari (es. 4/3, se li vogliamo: ❓), numerazione
  delle battute con eccezioni. Ricerca O(log n).
- **F1.05** Segmenti a tempo costante (esatti) e cambi graduali (accelerando/rallentando)
  integrati analiticamente; regola di arrotondamento ai campioni **documentata**; frame rate
  razionali (24/1, 25/1, 30000/1001); conversione inversa secondi → posizione (servirà a
  registrazione e DAW). Nessuna allocazione nelle conversioni usate dal thread audio (test).
  Benchmark: ricerca O(log n) misurata. ❓ **Decisione:** in quale grandezza è lineare un cambio
  graduale (BPM lineare nel tempo musicale o nel tempo reale): cambia le formule e il
  comportamento percepito. **Nota (da F1.01):** integrare un cambio graduale dà in generale un
  logaritmo, quindi i secondi dentro quei segmenti non sono razionali: F1.05 deve dichiarare fin
  dove arriva l'esattezza, prima dell'arrotondamento finale ai campioni (ADR-0015, Conseguenze).

#### F1-B — Il modello (`core-model`)

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F1.06 | **ADR: architettura del modello** — identità degli oggetti, struttura in memoria, come sono organizzate voci e tracce | ⬜ | 2 |
| F1.07 | **Altezza con grafia, intervalli, trasposizione** (tipi puri, molto testati) | ⬜ | 1–2 |
| F1.08 | **Scheletro del documento**: progetto, tracce/strumenti/righi, voci, mappe del tempo condivise | ⬜ | 2 |
| F1.09 | **Note a due strati** (ADR-0004): intento scritto + scostamenti; gruppi irregolari, legature di valore, abbellimenti, articolazioni, dinamiche e forcelle | ⬜ | 3–4 |
| F1.10 | **Forma**: ritornelli, volte, segno/coda, D.C./D.S. — come marcature scritte | ⬜ | 1–2 |
| F1.11 | **ADR + implementazione: comandi, undo/redo unico, notifiche di modifica a grana fine** | ⬜ | 3–4 |

Dettagli:

- **F1.06** È la decisione più importante della fase. Domande: ID stabili (indici generazionali
  o identificatori a 64 bit) che sopravvivono a undo/redo e al salvataggio; eventi di una voce in
  una struttura ordinata per posizione razionale (albero ordinato? vettore ordinato a blocchi per
  battuta?) con la complessità di inserimento/ricerca/scansione annotata; le battute sono oggetti
  memorizzati oppure derivate dalla mappa dei metri? (lo spartito ne ha bisogno, la DAW no:
  ADR-0003 vieta di duplicare); dove vivono le marcature che coprono un intervallo (forcelle,
  legature di portamento, ottave). Claude preparerà 2–3 alternative con un mini-prototipo e
  misure (inserimento in una partitura di riferimento da 100 battute × 10 righi). ❓
- **F1.07** Grafia (nome della nota, alterazione, ottava), enarmonie, intervalli con qualità,
  trasposizione di strumenti traspositori con grafia corretta (ADR-0004), altezza sonora MIDI
  derivata. Microtoni: ❓ **Decisione:** previsti nel tipo da subito (anche se non usati) o
  rimandati? Cambiarlo dopo tocca il formato del file.
- **F1.09** Tutti i "casi difficili" della tabella di ADR-0004 devono avere un test.
  Gli scostamenti sono **delta**, mai copie assolute.
- **F1.11** Ogni comando reversibile; raggruppamento (una "transazione" = un passo di undo,
  es. incollare 200 note); notifiche che dicono **cosa** è cambiato, per l'aggiornamento
  incrementale di layout e playback (ADR-0003 §6, ADR-0007). Test di proprietà: per ogni
  sequenza di comandi, fare-poi-annullare riporta al documento identico. ❓ **Decisioni:** limite
  della cronologia di undo (numero di passi o memoria), comandi "fusi" (trascinamento continuo =
  un solo passo).

#### F1-C — Ciò che legge il modello (`playback`, `io`)

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F1.12 | **Srotolamento della forma** in passaggi (written note id, pass index) | ⬜ | 2 |
| F1.13 | **Regole di esecuzione v0**: dall'intento agli eventi suonati (legature di valore fuse, articolazioni, dinamiche → velocity, abbellimenti) + scostamenti; aggiornamento incrementale | ⬜ | 3 |
| F1.14 | **Soglia di tolleranza**: il comando "sposta nota dal lato DAW" (ADR-0004) | ⬜ | 1–2 |
| F1.15 | **ADR: formato concreto del file** (completa ADR-0010) + versione 1 + primo file golden | ⬜ | 3 |
| F1.16 | **Generatore della partitura di riferimento** e benchmark di fase (apertura ≤ 1 s, modifica, srotolamento) | ⬜ | 1 |
| F1.18 | **Esportazione MIDI file** degli eventi suonati (`io`): primo ascolto del modello | ⬜ | 1–2 |
| F1.17 | **Chiusura fase**: documentazione, ROADMAP, revisione della scaletta, PR | ⬜ | 1 |

Dettagli:

- **F1.13** È il punto in cui il principio "intento + scostamenti" diventa concreto. L'uscita è
  un buffer di eventi pre-calcolato: è proprio ciò che il motore audio (F3) riceverà
  (ARCHITECTURE: `audio-engine` non vede `core-model`).
- **F1.14** ❓ **Decisione:** valore di default della soglia (proposta di ADR-0004: 1/32) e se
  offrire da subito anche la variante in millisecondi.
- **F1.15** Candidati da confrontare con i tipi veri: testo strutturato (JSON, RON o simili,
  leggibile e confrontabile con git) eventualmente dentro un contenitore (zip o cartella) per i
  media. Criteri ADR-0010: robustezza, tempo di caricamento, confrontabilità, dimensione,
  facilità di migrazione, percorsi multipiattaforma. Rifiuto dei file "più nuovi" con errore
  tipizzato; politica sui campi sconosciuti. ❓ **Decisioni:** formato, contenitore, uso di
  `serde` (dipendenza da valutare), estensione del file (`.orchestra`? ❓).
- **F1.16** La partitura di riferimento (100 battute × 10 righi) sarà riusata in F2 per il
  layout e in F3 per l'audio: si genera da codice, non si scrive a mano.

**"Primo ascolto" anticipato (approvato il 2026-10-30, D11):**
**F1.18:** esportazione **MIDI file** (crate `io`) degli eventi
di F1.13, così il proprietario può *ascoltare* il risultato del modello con qualunque lettore
già alla fine della Fase 1, senza aspettare il motore audio. Costo basso, verifica molto utile
delle regole di esecuzione. Si fa dopo F1.13 e prima della chiusura F1.17 (la tabella elenca i
passi per argomento, non in ordine stretto di esecuzione).

**Tappa M1** (§7): "motore musicale senza interfaccia".

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
| F2.01 | **Verifica e conferma dei mattoni** (ADR-0012 tabella): versioni, licenze, dimensioni di `winit`, `softbuffer`, `tiny-skia`, `skrifa`, `harfrust`, `accesskit`, `rfd`, `muda`; emendamento all'ADR | ⬜ | 1 |
| F2.02 | **Sistema dei temi** (ADR-0014): token semantici, istanze chiara e scura, test WCAG 2.2 AA, rilevamento del tema di sistema in `platform/` | ⬜ | 2–3 |
| F2.03 | **Astrazione di disegno**: lista di comandi di disegno, backend `tiny-skia`, ridisegno delle sole aree cambiate; rendering headless su immagine | ⬜ | 2–3 |
| F2.04 | **Finestra, ciclo eventi, input**: `winit` + `softbuffer`, "modificatore primario" astratto, DPI/scala, ridimensionamento | ⬜ | 2 |
| F2.05 | **`cargo xtask screenshots`** (prima versione, tema chiaro, en-US) | ⬜ | 1 |
| F2.06 | **Misura del percorso GPU** (`wgpu` + `vello`) fuori dalla sandbox, con il proprietario; ADR sul renderer GPU | ⬜ | 2–3 |
| F2.07 | **Testo**: font inclusi, shaping, cache dei glifi, misure; Bravura per i simboli musicali | ⬜ | 3 |
| F2.08 | **ADR: architettura dei widget** — modello ritenuto (retained) o immediato, algoritmo di layout, instradamento degli eventi, focus; nodo AccessKit per ogni widget fin dall'inizio | ⬜ | 2 |
| F2.09 | **Widget di base**: etichetta, pulsante, casella, interruttore, cursore, area scorrevole, divisore di pannelli | ⬜ | 3–4 |
| F2.10 | **Campo di testo**: modifica, selezione, appunti, IME, undo interno al campo | ⬜ | 3 |
| F2.11 | **Menu**: barra dei menu disegnata (Linux/Windows), menu contestuali, scorciatoie; `muda` su macOS | ⬜ | 2–3 |
| F2.12 | **Dialoghi**: file nativi (`rfd`), finestre modali nostre, finestra **Preferenze** (tema, lingua) | ⬜ | 2 |
| F2.13 | **i18n a runtime**: cambio lingua senza riavvio, rilevamento della lingua di sistema | ⬜ | 1 |
| F2.14 | **Salvataggio automatico e recupero dopo un crash** (collegato al file di progetto) | ⬜ | 1–2 |
| F2.15 | **Chiusura fase**: guide (primi capitoli con screenshot), documentazione, PR | ⬜ | 1–2 |

- **F2.06** viene presto apposta: è il rischio aperto più grosso della UI (la sandbox non ha GPU).
  Richiede che il proprietario esegua i test sul proprio schermo.
- **F2.08** ❓ **Decisioni:** retained vs immediato; layout nostro o crate esistente (es.
  `taffy`, da verificare); crate per gli appunti (es. `arboard`, da verificare) in F2.10.
- **F2.14** ❓ Proposta di Claude: anticiparlo qui perché un musicista che perde lavoro smette di
  usare il programma. Da confermare.

**Tappa M2A.**

### F2B — Notazione (20–30 sessioni)

**Obiettivo:** vedere e scrivere musica. Incisione incrementale (ADR-0007: mai ricalcolare tutta
la partitura), vista notazione, inserimento note da tastiera, tutto via comandi.

| ID | Passo | Stato | Stima |
|---|---|---|---|
| F2.16 | **ADR: architettura dell'incisione** — cache per battuta/sistema, invalidazione dalle notifiche del modello, cosa produce `notation-layout` (glifi posizionati, nessun disegno) | ⬜ | 2 |
| F2.17 | **Metadati SMuFL di Bravura** e primitive (righi, chiavi, armature, indicazioni di metro) | ⬜ | 2 |
| F2.18 | **Un rigo, una voce**: spaziatura orizzontale, gambi, travature, alterazioni, punti, pause | ⬜ | 4–5 |
| F2.19 | **Legature (di valore e di portamento), gruppi irregolari, abbellimenti, articolazioni, dinamiche, forcelle** | ⬜ | 3–4 |
| F2.20 | **Più voci, più righi, accollature, sistemi e interruzioni di riga** (incrementali) | ⬜ | 3–4 |
| F2.21 | **Vista notazione**: scorrimento, zoom, selezione, pagina bianca opzionale (ADR-0014) | ⬜ | 2–3 |
| F2.22 | **Inserimento note da tastiera** (step input) e modifica → comandi, undo/redo | ⬜ | 3 |
| F2.23 | **Benchmark di fase**: re-layout ≤ 16 ms; 100 battute × 10 righi a ≥ 30 fps software / 60 fps GPU | ⬜ | 1 |
| F2.24 | **Chiusura fase**: guide, documentazione, PR | ⬜ | 1–2 |

- ❓ **Decisione F2.18:** algoritmo di spaziatura (esistono approcci noti in letteratura, es.
  quelli basati sulle durate e sulle "molle"): Claude fa una ricerca e propone.
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

- ❓ **Decisione F3.03:** quali strumenti interni (sintetizzatore semplice? campionatore SFZ/SF2?)
  e **quali suoni**, con licenze compatibili: è una scelta che pesa sull'esperienza d'uso.
- ❓ **Decisione F3.04:** crate esistente (es. `cpal`, da verificare) o backend nostri (ALSA /
  PipeWire / JACK, WASAPI, CoreAudio); su Linux il kernel del proprietario è PREEMPT_RT, utile
  per le misure ma non rappresentativo dell'utente medio.
- ❓ **Decisione F3.07:** decodifica dei formati audio (es. `symphonia`, da verificare).
- La scelta del termine "region"/"clip" (glossario) va chiusa **prima di F3.06**.

**Tappa M3** = primo programma realmente usabile per un piccolo brano.

---

## 5. Filoni trasversali (si agganciano alle fasi, non hanno una fase propria)

| Filone | Quando (proposta) | Note |
|---|---|---|
| **Esportazione MIDI** | F1.18 (approvato) | Primo ascolto |
| **Importazione MIDI** | dopo F3.06 | ADR-0004: va *dedotto* l'intento (quantizzazione, grafia, voci): difficile, merita un ADR |
| **MusicXML import/export** | dopo F2B | Fondamentale per l'interoperabilità con gli altri software di notazione |
| **Stampa ed esportazione PDF** | dopo F2B | Sempre in colori chiari (ADR-0014) |
| **Parti staccate e strumenti traspositori in vista** | dopo F2B | Nessun dato duplicato (ADR-0004) |
| **Accessibilità** | ogni passo UI | Nodi AccessKit fin dal primo widget; test con lettore di schermo a ogni tappa |
| **Guide utente** | ogni tappa | Capitoli e screenshot crescono con le funzioni visibili |
| **Glossario e traduzioni** | ogni testo nuovo | Revisione dei madrelingua quando possibile |
| **Windows e macOS** | sempre in CI; prove su macchine reali a ogni tappa | ❓ Il proprietario ha accesso a macchine Windows/macOS per le prove manuali? |
| **Prestazioni** | ogni percorso caldo | Benchmark in CI, regressioni visibili nelle PR |
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
| D03 | Strumenti di benchmark e di test di proprietà | F1.02 | aperta |
| D04 | Metri non binari (es. 4/3): sì o no | F1.04 | aperta |
| D05 | Cambi di tempo graduali: lineari in quale grandezza | F1.05 | aperta |
| D06 | Architettura del modello (ID, strutture, battute memorizzate o derivate) | F1.06 | aperta |
| D07 | Microtoni nel tipo di altezza fin da subito | F1.07 | aperta |
| D08 | Limite della cronologia di undo; fusione dei comandi continui | F1.11 | aperta |
| D09 | Soglia di tolleranza di default; variante in ms | F1.14 | aperta (esiste già nella ROADMAP) |
| D10 | Formato concreto del file, contenitore, `serde`, estensione | F1.15 | aperta |
| D11 | Esportazione MIDI anticipata (F1.18) | fine F1-B | **chiusa** 2026-10-30: sì |
| D12 | Widget retained o immediato; layout; appunti | F2.08 | aperta |
| D13 | Salvataggio automatico già in F2A | F2.14 | aperta |
| D14 | Algoritmo di spaziatura orizzontale | F2.18 | aperta |
| D15 | Strumenti interni e suoni | F3.03 | aperta |
| D16 | Backend audio: crate esistente o nostri | F3.04 | aperta |
| D17 | "Region" o "clip" | F3.06 | aperta (esiste già nella ROADMAP) |
| D18 | Accesso a macchine Windows/macOS per prove manuali | F2A | aperta |
| D19 | Lingua della scaletta: italiano (attuale) o inglese come gli altri documenti di progetto | F0.17 | **chiusa** 2026-10-30: italiano |
| D20 | Ordine alternativo: incisione headless (F2B in PNG) **prima** delle fondamenta UI (F2A) | F0.17 | **chiusa** 2026-10-30: F2A poi F2B |

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
