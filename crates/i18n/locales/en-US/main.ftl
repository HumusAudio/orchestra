# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — en-US (SOURCE language).
# Every message here must exist in all six catalogs (tests/parity.rs) with the same variables.
# Before adding or changing a message, check crates/i18n/glossary.toml (docs/i18n-process.md).

## Application

app-name = Orchestra

## Main menu (glossary: menu_file, menu_edit, menu_view, undo, redo, score, track, mixer, new,
## open, save, save_as, quit, cut, copy, paste, select_all, preferences, full_screen, help).
## The items are also the actions' names (ADR-0048 AM10).

menu-file = File
menu-file-new = New
menu-file-open = Open
menu-file-save = Save
menu-file-save-as = Save as
menu-file-quit = Quit
menu-edit = Edit
menu-edit-undo = Undo
menu-edit-redo = Redo
menu-edit-cut = Cut
menu-edit-copy = Copy
menu-edit-paste = Paste
menu-edit-select-all = Select all
menu-edit-preferences = Preferences
menu-view = View
menu-view-score = Score
menu-view-tracks = Tracks
menu-view-mixer = Mixer
menu-view-full-screen = Full screen
menu-help-contents = Help

## Status bar (glossary: measure). $count is the number of selected measures.

status-measures-selected =
    { $count ->
        [one] { $count } measure selected
       *[other] { $count } measures selected
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Playback
playback-metronome = Metronome
playback-count-in = Count-in
playback-metronome-volume = Metronome volume
playback-track-name = Track name
playback-track-name-placeholder = Unnamed

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Restore defaults
dialog-ok = OK
