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

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Voreinstellungen wiederherstellen
dialog-ok = OK
