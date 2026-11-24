// SPDX-License-Identifier: GPL-3.0-or-later
// Chapter 2 — Preferences (Italian). English counterpart: docs/guide/en/chapters/02-preferences.typ

#import "/docs/guide/template.typ": note, screenshot-placeholder, tip

= Preferenze

Le Preferenze raccolgono le scelte che valgono per Orchestra, non per un singolo progetto: l'aspetto
della finestra, la lingua, la tastiera. Aprile con *Modifica → Preferenze…* oppure con Ctrl+, (su
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

== Tastiera

*Regolazione fine* sceglie il tasto che, tenuto premuto, rende più precisi i cursori e i divisori:
la rotella e le frecce si muovono di un passo alla volta e il trascinamento va dieci volte più
piano. Normalmente è Ctrl (⌘ su macOS); puoi scegliere Maiusc o Alt (⌥ su macOS) se ti è più comodo.

#tip("Suggerimento")[
  Se scegli Maiusc, Maiusc con la rotella su un cursore lo regola fine invece di far scorrere la
  finestra: per scorrere, usa la rotella fuori dal cursore.
]

#screenshot-placeholder(
  [Screenshot: la sezione Tastiera (preferences-keyboard.png)],
  [La sezione Tastiera: il tasto della regolazione fine.],
)

Qui arriverà anche l'elenco delle scorciatoie da tastiera, da cambiare a piacere.
