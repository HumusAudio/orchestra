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
