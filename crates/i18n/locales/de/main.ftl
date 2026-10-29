# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — German. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Datei
menu-edit = Bearbeiten
menu-edit-undo = Rückgängig
menu-edit-redo = Wiederherstellen
menu-view = Ansicht
menu-view-score = Partitur
menu-view-tracks = Spuren
menu-view-mixer = Mixer

## Status bar (glossary: measure → "Takt", plural "Takte")

status-measures-selected =
    { $count ->
        [one] { $count } Takt ausgewählt
       *[other] { $count } Takte ausgewählt
    }
