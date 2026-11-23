# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — en-US (SOURCE language).
# Every message here must exist in all six catalogs (tests/parity.rs) with the same variables.
# Before adding or changing a message, check crates/i18n/glossary.toml (docs/i18n-process.md).

## Application

app-name = Orchestra

## Main menu (glossary: menu_file, menu_edit, menu_view, undo, redo, score, track, mixer, new,
## open, save, save_as, quit, cut, copy, paste, select_all, preferences, full_screen, help,
## user_guide). The items that open a window end with an ellipsis (F2.11d2a, choice S2).
## Each name has a .mnemonic: the letter that chooses it with Alt (titles) or alone (in an open
## menu) on Linux and Windows, unique among its siblings (ADR-0048 AM10, F2.11d2b; tested).
## The items are also the actions' names (ADR-0048 AM10).

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

## macOS's own menus (F2.11e2, ADR-0048 AM9; glossary: mac_settings … mac_bring_all_to_front).
## Shown only in the system's menu bar on macOS: the application menu (Settings, Services, Hide,
## Hide Others, Show All, Quit) and the Window menu (Minimize, Zoom, Bring All to Front). The
## names are Apple's own in each language, so they match every other Mac program; { app-name }
## is the program's name, placed where the language wants it. No mnemonics: macOS has none.

menu-mac-settings = Settings…
menu-mac-services = Services
menu-mac-hide = Hide { app-name }
menu-mac-hide-others = Hide Others
menu-mac-show-all = Show All
menu-mac-quit = Quit { app-name }
menu-mac-window = Window
menu-mac-minimize = Minimize
menu-mac-zoom = Zoom
menu-mac-bring-all-to-front = Bring All to Front

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
