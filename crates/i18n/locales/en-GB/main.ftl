# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — en-GB. A separate catalog from en-US because British musical
# terminology differs (bar/measure, stave/staff, crotchet/quarter note): see glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = File
    .mnemonic = f
menu-file-new = New
    .mnemonic = n
menu-file-open = Open…
    .mnemonic = o
menu-file-save = Save
    .mnemonic = s
menu-file-save-as = Save as…
    .mnemonic = a
menu-file-quit = Quit
    .mnemonic = q
menu-edit = Edit
    .mnemonic = e
menu-edit-undo = Undo
    .mnemonic = u
menu-edit-redo = Redo
    .mnemonic = r
menu-edit-cut = Cut
    .mnemonic = t
menu-edit-copy = Copy
    .mnemonic = c
menu-edit-paste = Paste
    .mnemonic = p
menu-edit-select-all = Select all
    .mnemonic = a
menu-edit-preferences = Preferences…
    .mnemonic = n
menu-view = View
    .mnemonic = v
menu-view-score = Score
menu-view-tracks = Tracks
menu-view-mixer = Mixer
menu-view-full-screen = Full screen
    .mnemonic = f
menu-help = Help
    .mnemonic = h
menu-help-contents = User guide
    .mnemonic = u

## Keys' names in shortcuts, as menus write them next to an item, joined by "+" (glossary:
## key_ctrl … key_numpad; ADR-0048 AM12, F2.11d2a). $number is a function key's number (1–24),
## $digit a digit of the numeric keypad (0–9).

key-ctrl = Ctrl
key-shift = Shift
key-alt = Alt
key-super = Super
key-windows = Win
key-enter = Enter
key-escape = Esc
key-tab = Tab
key-space = Space
key-backspace = Backspace
key-delete = Del
key-insert = Ins
key-home = Home
key-end = End
key-page-up = PgUp
key-page-down = PgDn
key-left = Left
key-right = Right
key-up = Up
key-down = Down
key-menu = Menu
key-function = F{ $number }
key-numpad = Num { $digit }

## Status bar (glossary: measure → "bar" in en-GB)

status-measures-selected =
    { $count ->
        [one] { $count } bar selected
       *[other] { $count } bars selected
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
