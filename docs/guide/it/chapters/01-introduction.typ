// SPDX-License-Identifier: GPL-3.0-or-later
// Chapter 1 — Introduction (Italian). English counterpart: docs/guide/en/chapters/01-introduction.typ

#import "/docs/guide/template.typ": music, note, screenshot-placeholder, smufl, tip

= Introduzione

Orchestra è un unico programma per _scrivere_, _produrre_ e _sonorizzare_ la musica per le
immagini. Unisce un editor di notazione musicale, una workstation audio digitale (DAW) e un
semplice editor video e, a differenza di un insieme di strumenti separati, mantiene *un solo*
progetto: la partitura che scrivi e le tracce che produci sono la stessa musica.

== Un progetto, più viste

Quando scrivi una #music(smufl.text-quarter-note) semiminima su un rigo, la stessa identica nota
compare nella vista DAW, dove puoi suonarla, spostarla leggermente per darle un andamento più
umano o cambiarne l'intensità d'attacco. Niente viene copiato e niente va sincronizzato: la vista
di notazione e la vista DAW sono due modi di guardare le stesse note.

- La *vista di notazione* mostra la musica come è scritta: righi, battute, articolazioni,
  dinamiche, ritornelli.
- La *vista DAW* mostra la musica come viene suonata: i ritornelli sono srotolati, quindi un
  passaggio suonato due volte compare due volte sulla linea temporale, e ogni ripetizione resta
  collegata alla battuta da cui proviene.
- Il *video* è semplicemente un altro tipo di traccia sulla stessa linea temporale, così musica e
  immagini non possono mai andare fuori sincrono.

#screenshot-placeholder(
  [Screenshot: notation view and DAW view of the same project],
  [Le stesse note viste nella vista di notazione (in alto) e nella vista DAW (in basso).],
)

#note("Nota")[
  I piccoli spostamenti di tempo fatti nella vista DAW, quelli che rendono umana un'esecuzione,
  non modificano la partitura. Solo uno spostamento più grande di una tolleranza configurabile
  cambia il punto in cui la nota è scritta. Sei tu a decidere quanto è severa questa tolleranza.
]

== Aspetto chiaro e scuro

Orchestra avrà un aspetto chiaro e uno scuro. In modo predefinito segue l'impostazione del tuo
sistema operativo e cambia insieme a lei; puoi anche scegliere tu chiaro o scuro. La partitura può
seguire l'aspetto scuro oppure restare sempre su carta bianca, come preferisci.

== Regolare i valori

Ogni cursore e ogni manopola di Orchestra funziona allo stesso modo, ovunque si trovi:

- *Trascinalo*, oppure fai clic dove vuoi il valore.
- *Gira la rotella del mouse* con il puntatore sopra: non serve fare clic prima. Ogni scatto
  sposta il valore di poco; sul touchpad, scorri in su o in giù.
- *Tieni premuto Ctrl* (⌘ su macOS) mentre trascini, giri la rotella o premi le frecce per una
  regolazione più fine: il trascinamento va dieci volte più piano, uno scatto o una freccia
  spostano del passo più piccolo.
- *Fai doppio clic* per tornare al valore predefinito.
- Con la tastiera, quando è selezionato: le frecce, Pagina su e Pagina giù, Inizio e Fine.

#tip("Suggerimento")[
  Tieni premuto Maiusc mentre giri la rotella per far scorrere il pannello invece di cambiare il
  controllo sotto il puntatore.
]

== Scorciatoie da tastiera

I comandi di Orchestra hanno scorciatoie da tastiera che seguono le convenzioni del tuo sistema:
su Linux e Windows usano Ctrl, su macOS ⌘. Queste funzionano già:

- *F11* (⌃⌘F su macOS) attiva e disattiva lo schermo intero.
- *Ctrl+Q* (⌘Q su macOS) chiude Orchestra; su Windows chiudi la finestra con Alt+F4, come
  sempre.
- In una casella di testo, *Ctrl+Z* annulla quello che hai scritto, *Ctrl+Maiusc+Z* (e Ctrl+Y su
  Windows) lo ripristina, e *Ctrl+X*, *Ctrl+C*, *Ctrl+V* e *Ctrl+A* tagliano, copiano, incollano e
  selezionano tutto. Mentre scrivi in una casella, Annulla annulla solo ciò che hai scritto lì,
  mai una modifica al progetto.

Le scorciatoie seguono le lettere stampate sui tasti, qualunque sia il layout della tua tastiera.
Con un layout che non scrive lettere latine, come il russo o il greco, usano il tasto nella
posizione della lettera US, quindi Ctrl+Z resta il tasto dove si trova la Z su una tastiera US. I
tasti dei numeri funzionano per posizione su ogni layout, così su una tastiera francese non serve
Maiusc per usarli.

#note("Nota")[
  Potrai cambiare ogni scorciatoia nelle Preferenze. Alcuni tasti restano uguali su ogni sistema:
  Tab, le frecce, Invio, Esc e Spazio servono a muoversi nella finestra, e F10 apre i menu.
]

== La barra dei menu

Su Linux e Windows i menu sono in cima alla finestra: File, Modifica, Vista e Aiuto. Su macOS
saranno nella barra dei menu in cima allo schermo, come in ogni programma per Mac.

- *Con il mouse*, fai clic sul nome di un menu per aprirlo e poi su un comando. Puoi anche tenere
  premuto il tasto, spostarti su un comando e rilasciarlo lì. Mentre un menu è aperto, passando con
  il puntatore sul nome di un altro menu si apre quello; un nuovo clic sul nome del menu aperto lo
  chiude.
- *Con la tastiera*, premi *F10*, oppure premi e rilascia *Alt* da solo: il nome del primo menu si
  evidenzia. Spostati con le frecce a sinistra e a destra, apri il menu con la freccia in giù o con
  Invio, e scegli un comando con le frecce in su e in giù e con Invio. In un menu aperto le frecce a
  sinistra e a destra passano al menu vicino. Esc torna indietro di un passo; premilo ancora per
  lasciare i menu.
- *Più in fretta*: tieni premuto *Alt* e la lettera sottolineata del nome di un menu per aprirlo
  (Alt+M apre Modifica), poi premi la lettera sottolineata di un comando per eseguirlo. Le lettere
  sono sottolineate mentre tieni premuto Alt o usi i menu dalla tastiera. Sono scelte per ogni
  lingua, così in inglese Alt+E apre _Edit_.

#tip("Suggerimento")[
  I comandi che non sono ancora disponibili, o che in quel momento non possono fare nulla, sono in
  grigio. Le loro scorciatoie sono comunque scritte accanto, così i menu sono anche un elenco delle
  scorciatoie.
]

#screenshot-placeholder(
  [Screenshot: il menu File aperto dalla barra dei menu],
  [Il menu File aperto dalla tastiera, con le lettere sottolineate.],
)

== Menu contestuali

Fai clic con il tasto destro del mouse su una casella di testo per aprire il suo *menu
contestuale*: Annulla e Ripeti, Taglia, Copia e Incolla, Seleziona tutto. Puoi aprirlo anche dalla
tastiera, con il tasto Menu oppure con Maiusc+F10: allora si apre al cursore del testo, con il primo
comando disponibile già evidenziato.

- Ogni comando mostra a destra la sua scorciatoia da tastiera, scritta come la scrive il tuo
  sistema (Ctrl+C su Linux e Windows, ⌘C su macOS): così il menu ti insegna anche le scorciatoie.
- I comandi che in quel momento non possono fare nulla sono in grigio: Copia, per esempio, finché
  non hai selezionato del testo.
- Spostati con i tasti freccia, Home e Fine; premi Invio o Spazio per eseguire il comando
  evidenziato; premi Esc per chiudere il menu.
- Con il mouse puoi premere il tasto destro, spostarti su un comando e rilasciare lì il tasto: il
  comando parte subito.
- Un comando con una freccia sulla destra apre un sottomenu: fermati un attimo con il puntatore,
  fai clic, oppure premi la freccia a destra; la freccia a sinistra richiude il sottomenu.

#note("Nota")[
  I comandi del menu agiscono sulla casella di testo in cui stavi lavorando: la casella tiene la
  tastiera mentre il menu è aperto, così Copia copia proprio il testo che avevi selezionato.
]

#screenshot-placeholder(
  [Screenshot: il menu contestuale di una casella di testo],
  [Il menu contestuale di una casella di testo, con la scorciatoia di ogni comando a destra.],
)

== Stato di questa guida

Orchestra è in una fase di sviluppo molto iniziale. Questa edizione di anteprima spiega le idee
alla base del programma; ogni capitolo crescerà, con gli screenshot, man mano che le funzioni
corrispondenti diventeranno disponibili.

#tip("Suggerimento")[
  Questa guida esiste in due lingue, italiano (GUIDA.pdf) e inglese (GUIDE.pdf), con esattamente
  gli stessi capitoli e le stesse illustrazioni. L'interfaccia stessa è disponibile in inglese
  americano e britannico, italiano, francese, tedesco e spagnolo.
]
