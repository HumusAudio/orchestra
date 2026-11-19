# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — German. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Datei
menu-file-new = Neu
menu-file-open = Öffnen
menu-file-save = Speichern
menu-file-save-as = Speichern unter
menu-file-quit = Beenden
menu-edit = Bearbeiten
menu-edit-undo = Rückgängig
menu-edit-redo = Wiederherstellen
menu-edit-cut = Ausschneiden
menu-edit-copy = Kopieren
menu-edit-paste = Einfügen
menu-edit-select-all = Alles auswählen
menu-edit-preferences = Einstellungen
menu-view = Ansicht
menu-view-score = Partitur
menu-view-tracks = Spuren
menu-view-mixer = Mixer
menu-view-full-screen = Vollbild
menu-help-contents = Hilfe

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
