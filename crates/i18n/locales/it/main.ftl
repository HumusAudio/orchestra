# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — Italian. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = File
menu-edit = Modifica
menu-edit-undo = Annulla
menu-edit-redo = Ripristina
menu-view = Vista
menu-view-score = Partitura
menu-view-tracks = Tracce
menu-view-mixer = Mixer

## Status bar (glossary: measure → "battuta", feminine: the participle agrees)

status-measures-selected =
    { $count ->
        [one] { $count } battuta selezionata
       *[other] { $count } battute selezionate
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Riproduzione
playback-metronome = Metronomo
playback-count-in = Conteggio iniziale
playback-metronome-volume = Volume del metronomo
playback-track-name = Nome traccia
playback-track-name-placeholder = Senza nome

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Ripristina predefiniti
dialog-ok = OK
