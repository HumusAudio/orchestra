# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — German. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Datei
    .mnemonic = d
menu-file-new = Neu
    .mnemonic = n
menu-file-open = Öffnen…
    .mnemonic = ö
menu-file-save = Speichern
    .mnemonic = s
menu-file-save-as = Speichern unter…
    .mnemonic = u
menu-file-quit = Beenden
    .mnemonic = b
menu-edit = Bearbeiten
    .mnemonic = b
menu-edit-undo = Rückgängig
    .mnemonic = r
menu-edit-redo = Wiederherstellen
    .mnemonic = w
menu-edit-cut = Ausschneiden
    .mnemonic = a
menu-edit-copy = Kopieren
    .mnemonic = k
menu-edit-paste = Einfügen
    .mnemonic = i
menu-edit-select-all = Alles auswählen
    .mnemonic = l
menu-edit-preferences = Einstellungen…
    .mnemonic = e
menu-view = Ansicht
    .mnemonic = a
menu-view-score = Partitur
menu-view-tracks = Spuren
menu-view-mixer = Mixer
menu-view-full-screen = Vollbild
    .mnemonic = v
menu-help = Hilfe
    .mnemonic = h
menu-help-contents = Benutzerhandbuch
    .mnemonic = b
menu-help-about = Über { app-name }
    .mnemonic = e

## macOS's own menus (F2.11e2, ADR-0048 AM9; glossary: mac_settings … mac_bring_all_to_front).
## Shown only in the system's menu bar on macOS: the application menu (Settings, Services, Hide,
## Hide Others, Show All, Quit) and the Window menu (Minimize, Zoom, Bring All to Front). The
## names are Apple's own in each language, so they match every other Mac program; { app-name }
## is the program's name, placed where the language wants it. No mnemonics: macOS has none.

menu-mac-settings = Einstellungen…
menu-mac-services = Dienste
menu-mac-hide = { app-name } ausblenden
menu-mac-hide-others = Andere ausblenden
menu-mac-show-all = Alle einblenden
menu-mac-quit = { app-name } beenden
menu-mac-window = Fenster
menu-mac-minimize = Im Dock ablegen
menu-mac-zoom = Zoomen
menu-mac-bring-all-to-front = Alle nach vorne bringen

## Our title bar's window buttons (F2.11b2, ADR-0049 TB10; glossary: window_close …
## window_minimize). Drawn only where the system draws no frame (GNOME on Wayland), so the names
## are GTK's own; they are what assistive technologies read and the buttons' tooltips.
## "restore" is the Maximise button's name while the window is maximised.

window-close = Schließen
window-maximize = Maximieren
window-restore = Wiederherstellen
window-minimize = Minimieren

## Keys' names in shortcuts, as menus write them next to an item, joined by "+" (glossary:
## key_ctrl … key_numpad; ADR-0048 AM12, F2.11d2a). $number is a function key's number (1–24),
## $digit a digit of the numeric keypad (0–9).

key-ctrl = Strg
key-shift = Umschalt
key-alt = Alt
key-super = Super
key-windows = Win
key-enter = Eingabe
key-escape = Esc
key-tab = Tab
key-space = Leertaste
key-backspace = Rücktaste
key-delete = Entf
key-insert = Einfg
key-home = Pos1
key-end = Ende
key-page-up = Bild auf
key-page-down = Bild ab
key-left = Links
key-right = Rechts
key-up = Hoch
key-down = Runter
key-menu = Menü
key-function = F{ $number }
key-numpad = Zahlenblock { $digit }

## Status bar (glossary: measure → "Takt", plural "Takte")

status-measures-selected =
    { $count ->
        [one] { $count } Takt ausgewählt
       *[other] { $count } Takte ausgewählt
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Wiedergabe
playback-metronome = Metronom
playback-count-in = Einzähler
playback-metronome-volume = Metronomlautstärke
playback-track-name = Spurname
playback-track-name-placeholder = Unbenannt

## Dialog buttons (glossary: restore_defaults, ok, cancel, dialog_close)

dialog-restore-defaults = Voreinstellungen wiederherstellen
dialog-ok = OK
dialog-cancel = Abbrechen
dialog-close = Schließen

## The About dialog (F2.12a3b, ADR-0050 DG10; glossary: about, version, license, source_code,
## bundled_fonts, all_rights_reserved, logo). Its title is menu-help-about. $version is the
## program's version ("0.0.0"), $url the source repository's address; the fonts' and the
## licences' names are proper names and stay in English.

about-version = Version { $version }
about-license = Freie Software unter der GNU General Public License, Version 3 oder neuer.
about-logo-notice = Das Logo ist © 2026 Emanuele Rainoldi, alle Rechte vorbehalten; es fällt nicht unter diese Lizenz.
about-source = Quellcode: { $url }
about-fonts = Mitgelieferte Schriften: Inter, Bravura und JetBrains Mono, unter der SIL Open Font License 1.1.
about-logo = { app-name }-Logo

## The Preferences (F2.12a4a, ADR-0050 DG8–DG9; glossary: preferences, preferences_section,
## appearance, theme, theme_mode, system_mode, light, dark, blinking_cursor, theme_pairs). The
## title has no ellipsis (the menu item has one). The pairs' names are the ten built-in themes
## of ADR-0037 T1, translated where they are common words (owner's choice 8 of F2.12a4a).

prefs-title = Einstellungen
prefs-sections = Bereiche
prefs-section-appearance = Erscheinungsbild
prefs-theme = Darstellung
prefs-mode = Modus
prefs-mode-system = System
prefs-mode-light = Hell
prefs-mode-dark = Dunkel
prefs-caret-blink = Blinkender Cursor
theme-absolute = Absolut
theme-graphite = Graphit
theme-signal = Signal
theme-parchment = Pergament
theme-slate = Schiefer
theme-studio = Studio
theme-forest = Wald
theme-lagoon = Lagune
theme-nocturne = Nocturne
theme-snow = Schnee

## The Preferences' Language and Keyboard sections (F2.12a4b1; glossary: language, keyboard,
## system_language, fine_adjustment, language_names). The languages' names are each written in
## its own language, the same in every catalogue (owner's choice 1 of F2.12a4b1). Until F2.13 a
## new language applies at the next start, which the note beside the choice says.

prefs-section-language = Sprache
prefs-section-keyboard = Tastatur
prefs-language = Sprache
prefs-language-system = Systemsprache
prefs-language-restart = Gilt ab dem nächsten Start von { app-name }.
prefs-fine-modifier = Feineinstellung
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

prefs-section-playback = Wiedergabe
prefs-playback-intro = Wie Akzente und Dynamikzeichen gespielt werden. Diese Werte sind persönliche Einstellungen und gelten für alle Projekte.
prefs-playback-articulations = Artikulationsakzente
prefs-playback-dynamic-accents = Dynamische Akzente
prefs-playback-changes = Lautstärkewechsel
prefs-playback-louder = Lauter um
prefs-playback-back = Zurück in
prefs-playback-held = Gehalten
prefs-playback-change = Wechsel in
prefs-playback-ms = { $value } ms
prefs-playback-step = { $value }
articulation-soft-accent = Weicher Akzent
articulation-stress = Betont
articulation-accent = Akzent
articulation-marcato = Marcato
articulation-unstress = Unbetont
dynamic-rf = rf, rfz
dynamic-sf = sf, sfz, fz
dynamic-sffz = sffz
dynamic-fp = fp
dynamic-sfp = sfp, sfpp
dynamic-pf = pf

## The file choosers (F2.12a5a, ADR-0050 DG4; glossary: project, untitled, open, save_as). The
## filter names the project files; the titles are the choosers' own (some systems do not show
## them). file-untitled is the name proposed for a project never saved, without the extension
## (the program adds ".orchestra"). The error comes only on Linux, when the desktop offers no
## file chooser: the packages' names are not translated.

file-untitled = Unbenannt
file-filter-project = { app-name }-Projekt
file-open-title = Projekt öffnen
file-save-title = Projekt speichern unter
file-chooser-unavailable-title = Dateien können nicht ausgewählt werden
file-chooser-unavailable-message = { app-name } verwendet zum Auswählen von Dateien das Fenster der Arbeitsumgebung, aber kein Dienst der Arbeitsumgebung stellt es bereit. Dafür wird das Paket xdg-desktop-portal zusammen mit dem Paket der verwendeten Arbeitsumgebung benötigt (zum Beispiel xdg-desktop-portal-kde oder xdg-desktop-portal-gnome).
