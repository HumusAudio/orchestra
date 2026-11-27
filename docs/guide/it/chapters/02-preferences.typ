// SPDX-License-Identifier: GPL-3.0-or-later
// Chapter 2 — Preferences (Italian). English counterpart: docs/guide/en/chapters/02-preferences.typ

#import "/docs/guide/template.typ": note, screenshot-placeholder, tip

= Preferenze

Le Preferenze raccolgono le scelte che valgono per Orchestra, non per un singolo progetto: l'aspetto
della finestra, la lingua, il modo di suonare, la tastiera. Aprile con *Modifica → Preferenze…* oppure con Ctrl+, (su
macOS con *Orchestra → Impostazioni…* oppure ⌘,).

== Come funzionano

A sinistra c'è l'elenco delle sezioni, a destra le impostazioni della sezione scelta. Spostati tra le
sezioni con il mouse oppure con le frecce in su e in giù; Tab passa alle impostazioni.

- Ogni modifica si applica *subito*: vedi il risultato mentre scegli, senza confermare. Non ci sono
  pulsanti OK o Annulla.
- Orchestra ricorda le tue scelte e le ritrova alla prossima apertura.
- Chiudi la finestra con *Chiudi*, con Esc oppure di nuovo con Ctrl+, (⌘, su macOS).

#screenshot-placeholder(
  [Screenshot: le Preferenze aperte sulla sezione Aspetto (preferences-dialog.png)],
  [Le Preferenze: l'elenco delle sezioni a sinistra, la sezione Aspetto a destra.],
)

== Aspetto

- *Tema* sceglie una delle dieci coppie di colori di Orchestra (Studio, Ardesia, Foresta, Neve e le
  altre), ognuna con una metà chiara e una scura.
- *Modalità* sceglie quale metà usare: *Sistema* segue l'impostazione chiara o scura del tuo
  sistema e cambia insieme a lei, *Chiaro* e *Scuro* la fissano.
- *Cursore lampeggiante* spegne il lampeggio del cursore nelle caselle di testo: il cursore resta
  fisso.

=== I tuoi temi

I dieci temi integrati non si possono cambiare, ma puoi fare tutti i temi tuoi che vuoi, partendo da
uno qualunque di loro.

- *Duplica* fa una copia del tema che stai usando, che si chiama per esempio "Studio (copia)", e la
  sceglie subito. I tuoi temi compaiono nell'elenco *Tema* dopo quelli integrati.
- *Nome* rinomina il tema: scrivi il nuovo nome e premi Invio, oppure passa a un altro controllo. Se
  un altro tuo tema ha già quel nome, Orchestra ci aggiunge un numero.
- *Elimina* toglie il tema dopo avertelo chiesto, e torna al tema integrato da cui era nato.
- *Esporta…* salva il tema che stai usando in un file suo (`.orchestra-theme`), per portarlo su un
  altro computer o darlo a qualcuno. *Importa…* legge un file così e aggiunge il suo tema ai tuoi,
  senza sostituirne nessuno: se il nome è già usato, ci aggiunge un numero. Il tema importato viene
  scelto subito. Se il file contiene qualcosa che questa versione di Orchestra non sa usare, un breve
  messaggio dice quante voci sono state lasciate fuori.

Sotto *Testo* scegli quanto è grande tutto il testo dell'interfaccia, dall'85% al 150%, e il
carattere dell'interfaccia, dei numeri e dei percorsi dei file. Sotto *Linee, angoli e ombre* scegli
quanto sono arrotondati gli angoli, quanto sono spesse le linee, la decorazione del focus e la
cornice dei dialoghi, e quanto sono morbide e lontane le ombre. La *Densità* può essere
*Comoda*, con spazio intorno ai controlli, o *Compatta*, con i controlli più vicini tra loro. Ogni valore ha il suo *Ripristina*,
e la finestra cambia mentre muovi il controllo. Queste scelte sono uguali nella metà chiara e in
quella scura; ogni cursore si ferma dove l'interfaccia diventerebbe difficile da usare. Anche con il
testo più grande niente esce dalla finestra: le impostazioni si risistemano per starci, e un nome
troppo lungo per il suo posto va su una seconda riga.

#screenshot-placeholder(
  [Screenshot: testo, linee e densità di un tema tuo (theme-text-lines.png)],
  [Testo al 110% e angoli più arrotondati: tutta la finestra li usa già.],
)

Sotto *Colori* ogni colore dell'interfaccia ha una riga: il suo nome, un campione del colore, il suo
valore e *Ripristina*. Scrivi il valore come sul web, `#` seguito da sei cifre esadecimali
(`#5b9bf0`), oppure otto cifre per aggiungere la trasparenza (`#00000080` è nero a metà intensità).
Appena il valore è un colore, tutta la finestra lo prende: le Preferenze stesse ti mostrano il
risultato. *Ripristina* ridà al colore il valore del tema da cui era nato.

Non serve conoscere i codici dei colori: fai clic sul campione di un colore e sotto la sua riga si
apre un selettore. Nel quadrato grande, verso destra il colore è più intenso e verso l'alto più
luminoso; la striscia sotto sceglie la tinta, dal rosso attraverso il verde e il blu fino di nuovo al
rosso; per un colore trasparente, come l'ombra, una terza striscia regola la trasparenza. I due
campioni in fondo mostrano il colore di prima e quello di adesso. La finestra cambia mentre ti muovi.
Premi Invio, o fai di nuovo clic sul campione, per tenere il colore; Esc ridà il colore da cui eri
partito. Il codice nella casella segue il selettore, e il selettore segue un codice che scrivi.

#screenshot-placeholder(
  [Screenshot: il selettore dei colori aperto sotto il colore d'accento (theme-picker.png)],
  [Il selettore dei colori: il quadrato, la striscia della tinta, e il colore di prima e di adesso.],
)

#screenshot-placeholder(
  [Screenshot: un tema tuo con il colore d'accento cambiato (theme-editor.png)],
  [Una copia di Studio con l'accento blu: tutta la finestra lo usa già.],
)

#note("Nota")[
  Ogni tema ha una metà chiara e una scura. Le righe mostrano i colori della metà che la finestra sta
  usando, come dice la riga sopra di loro: per cambiare l'altra metà, sceglila in *Modalità*, e le
  righe la seguono.
]

Sotto il nome, *Leggibilità* ti dice se tutto si legge ancora. Se un colore è troppo vicino a quello
che ha dietro, qualche frase semplice dice quanto si vede male ("quasi invisibile", "si legge a
fatica", oppure si legge "ma non abbastanza bene per chi vede poco"), su quale colore, e cosa fare.
Il consiglio riguarda il colore che hai cambiato, e Orchestra controlla che funzioni davvero: ti
dice di schiarire un colore solo se un colore più chiaro può risolvere il problema, e lo stesso per
scurirlo. Quando c'è di mezzo una sola coppia di colori, ti dice anche come cambiare l'altro. Se
nessun colore più chiaro o più scuro può aiutare, ti dice di premere *Ripristina*. Le righe dei
colori coinvolti sono segnate con un pallino, così le trovi subito. È solo un consiglio: il colore
resta il tuo. Quando va tutto bene, una riga breve lo dice.

#screenshot-placeholder(
  [Screenshot: un avviso di leggibilità in cima alla sezione (theme-contrast.png)],
  [Il testo secondario reso troppo scuro: quasi invisibile su quattro sfondi; il consiglio è di schiarirlo.],
)

#tip("Suggerimento")[
  I tuoi temi sono conservati con le tue preferenze, così li ritrovi alla prossima apertura di
  Orchestra.
]

== Lingua

Scegli la lingua dell'interfaccia: inglese americano o britannico, italiano, francese, tedesco o
spagnolo. Ogni lingua è scritta nella lingua stessa, così la ritrovi anche se l'interfaccia è in una
lingua che non conosci. *Lingua del sistema* usa quella del tuo sistema.

#note("Nota")[
  Per ora la nuova lingua vale dal prossimo avvio di Orchestra, come dice la nota sotto la scelta.
  In una versione futura cambierà subito, e *Lingua del sistema* riconoscerà la lingua del tuo
  sistema (oggi usa l'inglese americano).
]

#screenshot-placeholder(
  [Screenshot: la sezione Lingua (preferences-language.png)],
  [La sezione Lingua, con la nota sul riavvio.],
)

== Riproduzione

Qui scegli come Orchestra suona gli accenti e i segni dinamici: di quanto un accento è più forte e
in quanto tempo il suono torna al livello di prima, quanto durano la tenuta e il passaggio di *fp*,
*sfp* e *pf*. I valori di partenza sono stati tarati a orecchio. Ogni valore ha un cursore, con il
valore scritto accanto; *Ripristina predefiniti* li riporta tutti ai valori di partenza.

#note("Nota")[
  Questi valori sono tuoi, non del progetto: valgono per tutti i progetti che apri, e chi apre lo
  stesso progetto con le sue preferenze può sentirlo suonare in modo un po' diverso.
]

#screenshot-placeholder(
  [Screenshot: la sezione Riproduzione (preferences-playback.png)],
  [La sezione Riproduzione: gli accenti, gli accenti dinamici e i cambi di livello.],
)

== Tastiera

*Regolazione fine* sceglie il tasto che, tenuto premuto, rende più precisi i cursori e i divisori:
la rotella e le frecce si muovono di un passo alla volta e il trascinamento va dieci volte più
piano. Normalmente è Ctrl (⌘ su macOS); puoi scegliere Maiusc o Alt (⌥ su macOS) se ti è più comodo.

#tip("Suggerimento")[
  Se scegli Maiusc, Maiusc con la rotella su un cursore lo regola fine invece di far scorrere la
  finestra: per scorrere, usa la rotella fuori dal cursore.
]

=== Scorciatoie

Sotto c'è l'elenco di tutti i comandi con le loro scorciatoie da tastiera, nell'ordine dei menu.
Puoi cambiarle come preferisci: i menu mostrano subito le scorciatoie nuove, e Orchestra se le
ricorda al prossimo avvio.

#screenshot-placeholder(
  [Screenshot: la sezione Tastiera (preferences-keyboard.png)],
  [La sezione Tastiera: il tasto della regolazione fine e i comandi con le loro scorciatoie.],
)

- *Cerca comandi* restringe l'elenco mentre scrivi. Trova un comando dal nome, oppure dalla
  scorciatoia: scrivi "ctrl+s" per vedere che cosa fa Ctrl+S.
- Seleziona un comando per vedere le sue scorciatoie sotto l'elenco. Un comando può averne più di
  una, oppure nessuna.
- *Aggiungi…* aspetta la scorciatoia nuova: tieni premuti i tasti modificatori che vuoi (Ctrl,
  Maiusc, Alt…) e premi il tasto. Esc annulla, e anche Tab, che come sempre passa oltre.
- *Rimuovi* toglie al comando la scorciatoia selezionata.
- *Ripristina predefiniti* restituisce al comando le scorciatoie che aveva all'inizio; *Ripristina
  tutte le scorciatoie* lo fa per tutti i comandi.

Alcuni tasti non possono diventare scorciatoie, perché servono a Orchestra e al sistema. Da soli o
con Maiusc: Tab e le frecce per spostarsi tra i controlli, Invio, Spazio ed Esc, F10; e sempre: Alt
e il tasto Menu, che aprono i menu, Alt con
una lettera per le lettere sottolineate dei menu (su Linux e Windows), e su macOS quelle del
sistema, come ⌘H. Se ne premi uno, Orchestra ti dice perché accanto al riquadro, e aspetta un
altro tasto.

Se la scorciatoia che premi appartiene già a un altro comando, Orchestra chiede prima di
spostarla:

#screenshot-placeholder(
  [Screenshot: la domanda su una scorciatoia già usata (shortcut-conflict.png)],
  [Ctrl+N appartiene a Nuovo: Riassegna la dà invece ad Apri.],
)

*Riassegna* (o Invio) la dà al comando nuovo, e l'altro non ce l'ha più; *Annulla* (o Esc) lascia
tutto com'era. *Ripristina predefiniti* chiede allo stesso modo quando altri comandi hanno preso
nel frattempo alcune delle scorciatoie originali del comando, e le elenca.

=== Portare con sé le scorciatoie

*Esporta…* salva tutte le tue scorciatoie in un file che finisce con `.orchestra-shortcuts`: ogni
comando con le sue scorciatoie, anche quelle che non hai mai cambiato. Copialo su un altro
computer, condividilo con un collega o con una classe, oppure tienilo da parte prima di provare
qualcosa di nuovo. Il file vale allo stesso modo su Linux, Windows e macOS: Ctrl su uno diventa ⌘
su un Mac.

*Importa…* legge un file così e mette le sue scorciatoie al posto delle tue. Se avevi cambiato
qualche scorciatoia, Orchestra chiede prima, perché le tue andrebbero perse; *Sostituisci* va
avanti, *Annulla* (o Esc) le lascia come sono. I comandi che il file non nomina ricevono le loro
scorciatoie di sempre.

#note("Nota")[
  Un file fatto con un'altra versione di Orchestra può nominare comandi che questa non ha, oppure
  tasti che il tuo sistema tiene per sé. Orchestra tiene quello che può usare e ti dice quante voci
  non ha potuto usare. Un file fatto da una versione più recente di Orchestra non si può
  importare: installa prima l'ultima versione.
]
