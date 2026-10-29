# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — en-US (SOURCE language).
# Every message here must exist in all six catalogs (tests/parity.rs) with the same variables.
# Before adding or changing a message, check crates/i18n/glossary.toml (docs/i18n-process.md).

## Application

app-name = Orchestra

## Main menu (glossary: menu_file, menu_edit, menu_view, undo, redo, score, track, mixer)

menu-file = File
menu-edit = Edit
menu-edit-undo = Undo
menu-edit-redo = Redo
menu-view = View
menu-view-score = Score
menu-view-tracks = Tracks
menu-view-mixer = Mixer

## Status bar (glossary: measure). $count is the number of selected measures.

status-measures-selected =
    { $count ->
        [one] { $count } measure selected
       *[other] { $count } measures selected
    }
