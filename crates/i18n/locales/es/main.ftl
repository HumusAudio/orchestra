# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — Spanish. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Archivo
menu-edit = Editar
menu-edit-undo = Deshacer
menu-edit-redo = Rehacer
menu-view = Vista
menu-view-score = Partitura
menu-view-tracks = Pistas
menu-view-mixer = Mezclador

## Status bar (glossary: measure → "compás", masculine, plural "compases")

status-measures-selected =
    { $count ->
        [one] { $count } compás seleccionado
       *[other] { $count } compases seleccionados
    }
