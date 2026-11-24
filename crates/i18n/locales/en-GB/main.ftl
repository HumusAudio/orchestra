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
menu-help-about = About { app-name }
    .mnemonic = a

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
menu-mac-minimize = Minimise
menu-mac-zoom = Zoom
menu-mac-bring-all-to-front = Bring All to Front

## Our title bar's window buttons (F2.11b2, ADR-0049 TB10; glossary: window_close …
## window_minimize). Drawn only where the system draws no frame (GNOME on Wayland), so the names
## are GTK's own; they are what assistive technologies read and the buttons' tooltips.
## "restore" is the Maximise button's name while the window is maximised.

window-close = Close
window-maximize = Maximise
window-restore = Restore
window-minimize = Minimise

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

## Dialog buttons (glossary: restore_defaults, ok, cancel, dialog_close)

dialog-restore-defaults = Restore defaults
dialog-ok = OK
dialog-cancel = Cancel
dialog-close = Close

## The About dialog (F2.12a3b, ADR-0050 DG10; glossary: about, version, license, source_code,
## bundled_fonts, all_rights_reserved, logo). Its title is menu-help-about. $version is the
## program's version ("0.0.0"), $url the source repository's address; the fonts' and the
## licences' names are proper names and stay in English.

about-version = Version { $version }
about-license = Free software under the GNU General Public Licence, version 3 or later.
about-logo-notice = The logo is © 2026 Emanuele Rainoldi, all rights reserved: it is not covered by that licence.
about-source = Source code: { $url }
about-fonts = Bundled fonts: Inter, Bravura and JetBrains Mono, under the SIL Open Font License 1.1.
about-logo = { app-name } logo
