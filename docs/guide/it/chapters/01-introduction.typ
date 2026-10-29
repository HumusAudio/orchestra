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

== Stato di questa guida

Orchestra è in una fase di sviluppo molto iniziale. Questa edizione di anteprima spiega le idee
alla base del programma; ogni capitolo crescerà, con gli screenshot, man mano che le funzioni
corrispondenti diventeranno disponibili.

#tip("Suggerimento")[
  Questa guida esiste in due lingue, italiano (GUIDA.pdf) e inglese (GUIDE.pdf), con esattamente
  gli stessi capitoli e le stesse illustrazioni. L'interfaccia stessa è disponibile in inglese
  americano e britannico, italiano, francese, tedesco e spagnolo.
]
