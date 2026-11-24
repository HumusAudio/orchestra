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

## The Preferences (F2.12a4a, ADR-0050 DG8–DG9; glossary: preferences, preferences_section,
## appearance, theme, theme_mode, system_mode, light, dark, blinking_cursor, theme_pairs). The
## title has no ellipsis (the menu item has one). The pairs' names are the ten built-in themes
## of ADR-0037 T1, translated where they are common words (owner's choice 8 of F2.12a4a).

prefs-title = Preferences
prefs-sections = Sections
prefs-section-appearance = Appearance
prefs-theme = Theme
prefs-mode = Mode
prefs-mode-system = System
prefs-mode-light = Light
prefs-mode-dark = Dark
prefs-caret-blink = Blinking cursor
theme-absolute = Absolute
theme-graphite = Graphite
theme-signal = Signal
theme-parchment = Parchment
theme-slate = Slate
theme-studio = Studio
theme-forest = Forest
theme-lagoon = Lagoon
theme-nocturne = Nocturne
theme-snow = Snow

## The Preferences' Language and Keyboard sections (F2.12a4b1; glossary: language, keyboard,
## system_language, fine_adjustment, language_names). The languages' names are each written in
## its own language, the same in every catalogue (owner's choice 1 of F2.12a4b1). Until F2.13 a
## new language applies at the next start, which the note beside the choice says.

prefs-section-language = Language
prefs-section-keyboard = Keyboard
prefs-language = Language
prefs-language-system = System language
prefs-language-restart = Applies the next time { app-name } starts.
prefs-fine-modifier = Fine adjustment
language-en-us = English (US)
language-en-gb = English (UK)
language-it = Italiano
language-fr = Français
language-de = Deutsch
language-es = Español

## The Preferences' Playback section (F2.12a4b2, ADR-0050 DG9, ADR-0027 §14–§15; glossary:
## playback, articulation_accents, dynamic_accents, changes_of_level, soft_accent, stress,
## accent, marcato, unstress). The dynamic marks are written as in the score, the same in every
## language. $value is a number already written with its sign ("+22", "−7") or a whole number of
## milliseconds; the space before "ms" is a no-break space.

prefs-section-playback = Playback
prefs-playback-intro = How accents and dynamic marks are played. These values are personal: they apply to every project.
prefs-playback-articulations = Articulation accents
prefs-playback-dynamic-accents = Dynamic accents
prefs-playback-changes = Changes of level
prefs-playback-louder = Louder by
prefs-playback-back = Back in
prefs-playback-held = Held for
prefs-playback-change = Change in
prefs-playback-ms = { $value } ms
prefs-playback-step = { $value }
articulation-soft-accent = Soft accent
articulation-stress = Stress
articulation-accent = Accent
articulation-marcato = Marcato
articulation-unstress = Unstress
dynamic-rf = rf, rfz
dynamic-sf = sf, sfz, fz
dynamic-sffz = sffz
dynamic-fp = fp
dynamic-sfp = sfp, sfpp
dynamic-pf = pf
