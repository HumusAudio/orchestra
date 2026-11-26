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

## The open project (F2.12a5b, ADR-0050 DG5; glossary: project, untitled, save, dont_save,
## replace). $name is the project's name: its file's name without ".orchestra", or file-untitled
## for a project never saved; $file is a file's whole name ("Song.orchestra"). The window title
## puts the name before the program's; while the project has unsaved changes it starts with a
## bullet and a space, the same mark in every language. The question about unsaved changes comes
## before New, Open and closing; its three buttons are dialog-save, dialog-dont-save and
## dialog-cancel. The question about replacing comes when the program added ".orchestra" to the
## name typed and a file with that name exists. The errors' titles name the file; their messages
## are shown under them, with the file's path, and for a save with project-save-error-kept.

window-title = { $name } — { app-name }
window-title-modified = • { $name } — { app-name }
project-save-changes-title = Änderungen an „{ $name }“ speichern?
project-save-changes-message = Ihre Änderungen gehen verloren, wenn sie nicht gespeichert werden.
dialog-save = Speichern
dialog-dont-save = Nicht speichern
project-replace-title = Eine Datei namens „{ $file }“ existiert bereits. Möchten Sie sie ersetzen?
project-replace-message = Beim Ersetzen wird ihr Inhalt überschrieben.
dialog-replace = Ersetzen
project-new-error-title = Ein neues Projekt kann nicht erstellt werden
project-new-error-message = Dies ist ein Fehler in { app-name }: Bitte melden Sie ihn.
project-open-error-title = „{ $file }“ kann nicht geöffnet werden
project-open-error-not-found = Die Datei existiert nicht oder ist im Moment nicht erreichbar.
project-open-error-permission = { app-name } hat nicht die Berechtigung, diese Datei zu lesen.
project-open-error-read = Die Datei konnte nicht gelesen werden.
project-open-error-too-large = Die Datei ist größer, als { app-name } öffnen kann.
project-open-error-not-project = Die Datei ist kein { app-name }-Projekt.
project-open-error-newer = Das Projekt wurde mit einer neueren Version von { app-name } gespeichert. Installieren Sie die neueste Version, um es zu öffnen.
project-open-error-damaged = Die Datei ist beschädigt: Ein Teil davon kann nicht als { app-name }-Projekt gelesen werden.
project-save-error-title = „{ $file }“ kann nicht gespeichert werden
project-save-error-permission = { app-name } hat nicht die Berechtigung, diese Datei oder ihren Ordner zu schreiben.
project-save-error-full = Auf dem Datenträger ist nicht genügend Speicherplatz frei.
project-save-error-write = Die Datei konnte nicht geschrieben werden.
project-save-error-internal = Das Projekt konnte nicht zum Speichern vorbereitet werden. Dies ist ein Fehler in { app-name }: Bitte melden Sie ihn.
project-save-error-kept = Die Datei, die bereits dort war, ist unverändert geblieben.

## The shortcut editor in the Preferences' Keyboard section (F2.12c1, ADR-0048 AM7; glossary:
## shortcut, command, reassign, add, remove, search, restore_defaults, key_escape). $command and
## $owner are commands' names as the menus write them ("Save as…" keeps its ellipsis); $shortcut is a
## shortcut written as the menus write it ("Ctrl+Shift+S", "⇧⌘S"); $search is what the user typed in
## the search field. prefs-shortcut-capture is shown inside the box that waits for the keys;
## prefs-shortcut-fixed under it when a key that cannot be assigned is pressed. The two questions
## come when the shortcut, or a default being restored, belongs to another command; their button
## is dialog-reassign, with dialog-cancel.

prefs-shortcuts = Tastaturkürzel
prefs-shortcuts-search = Befehle suchen
prefs-shortcuts-commands = Befehle
prefs-shortcuts-no-match = Kein Befehl passt zu „{ $search }“.
prefs-shortcuts-of = Tastaturkürzel von „{ $command }“
prefs-shortcuts-none = Kein Tastaturkürzel
prefs-shortcut-add = Hinzufügen…
prefs-shortcut-remove = Entfernen
prefs-shortcuts-restore-all = Alle Tastaturkürzel zurücksetzen
prefs-shortcut-capture = Drücken Sie das Tastaturkürzel oder Esc zum Abbrechen
prefs-shortcut-capture-name = Neues Tastaturkürzel für „{ $command }“
prefs-shortcut-fixed = { $shortcut } kann nicht zugewiesen werden: Es dient der Navigation im Programm, den Menüs oder dem System.
prefs-shortcut-conflict-title = { $shortcut } ist bereits „{ $owner }“ zugewiesen
prefs-shortcut-conflict-message = Stattdessen „{ $command }“ zuweisen? „{ $owner }“ verliert es.
prefs-shortcut-restore-conflict-title = Einige Standard-Tastaturkürzel von „{ $command }“ sind anderen Befehlen zugewiesen
prefs-shortcut-restore-conflict-message = An „{ $command }“ zurückgeben? Die anderen Befehle verlieren sie.
dialog-reassign = Neu zuweisen

## Importing and exporting the shortcuts (F2.12c2, ADR-0051; glossary: import, export, shortcut,
## replace). $file is a file's whole name ("Mine.orchestra-shortcuts"); $count a number of entries.
## file-shortcuts-name is the name proposed when exporting, without the extension (the program adds
## ".orchestra-shortcuts"). The question comes before an import replaces shortcuts the user had
## changed (buttons: dialog-replace, dialog-cancel). The report comes after an import when some
## entries of the file could not be used. The errors' other messages are those of the project's
## (project-open-error-*, project-save-error-*).

prefs-shortcuts-import = Importieren…
prefs-shortcuts-export = Exportieren…
file-filter-shortcuts = { app-name }-Tastaturkürzel
file-import-shortcuts-title = Tastaturkürzel importieren
file-export-shortcuts-title = Tastaturkürzel exportieren
file-shortcuts-name = Tastaturkürzel
prefs-shortcuts-import-question-title = Ihre Tastaturkürzel durch die aus „{ $file }“ ersetzen?
prefs-shortcuts-import-question-message = Die Tastaturkürzel, die Sie geändert haben, gehen verloren.
prefs-shortcuts-imported-title = Tastaturkürzel importiert
prefs-shortcuts-imported-unused = Nicht verwendete Einträge aus „{ $file }“: { $count }
prefs-shortcuts-import-error-title = „{ $file }“ kann nicht importiert werden
prefs-shortcuts-import-error-not-shortcuts = Die Datei ist keine { app-name }-Tastaturkürzeldatei.
prefs-shortcuts-import-error-newer = Die Datei wurde mit einer neueren Version von { app-name } gespeichert. Installieren Sie die neueste Version, um sie zu importieren.
prefs-shortcuts-import-error-damaged = Die Datei ist beschädigt: Sie kann nicht als { app-name }-Tastaturkürzel gelesen werden.
prefs-shortcuts-export-error-title = „{ $file }“ kann nicht exportiert werden

## The theme editor in the Preferences' Appearance section (F2.12b1, ADR-0037 T7–T8; glossary:
## theme_color_names, accent_color, duplicate, delete, color, readability). The colour names
## are those of the theme's tokens, as the editor lists them; the user picks a value for each.
## $name is a theme's name; $color and $background are colour names (theme-color-*). The editing
## notes name the Mode control and its values (prefs-mode, prefs-mode-light, prefs-mode-dark):
## keep them identical.
## The readability warnings (owner's choices of 2026-11-26): plain words, no numbers. A warning
## is up to three sentences, each its own message, shown one after the other with a space:
## prefs-theme-readability-problem says how hard $color is to see on $background (both names of
## rows of the editor, so the user finds them). $kind is "text" (letters) or "graphic" (a line or
## a shape: borders, the focus ring, a switch that is on); $severity is "invisible" (almost
## invisible), "hard" (hard to read, or to make out for graphics) or "weak" (readable, but not
## well enough for people who do not see well; text only).
## prefs-theme-readability-same says that $culprit (the colour to fix) has the same problem with
## $count other colours (1 or more).
## prefs-theme-readability-advice says what to do with $culprit: $advice is "change" (make it
## lighter or darker, $direction "lighter" or "darker"), "either" (the same, or make $partner, the
## other colour, lighter or darker: $partner-direction) or "restore" (no lighter or darker colour
## is enough: press the row's Restore button, prefs-theme-restore). Say "the colour" before each
## name, so that adjectives agree with "colour" and not with the name (which may be plural).
## Address the user as the rest of the catalogue does.

prefs-theme-duplicate = Duplizieren
prefs-theme-delete = Löschen
prefs-theme-name = Name
prefs-theme-copy-name = { $name } (Kopie)
prefs-theme-colors = Farben
prefs-theme-builtin = Mitgelieferte Darstellungen können nicht geändert werden: Duplizieren Sie eine, um eine eigene zu erstellen.
prefs-theme-editing-light = Dies sind die Farben der hellen Hälfte. Um die dunkle Hälfte zu ändern, wählen Sie Dunkel unter Modus.
prefs-theme-editing-dark = Dies sind die Farben der dunklen Hälfte. Um die helle Hälfte zu ändern, wählen Sie Hell unter Modus.
prefs-theme-restore = Wiederherstellen
prefs-theme-value-name = Wert von „{ $color }“
prefs-theme-readability = Lesbarkeit
prefs-theme-readability-ok = Alle Texte und Bedienelemente sind gut lesbar.
prefs-theme-readability-problem =
    { $kind ->
        [graphic]
            { $severity ->
                [invisible] Die Farbe „{ $color }“ ist auf „{ $background }“ fast unsichtbar.
               *[hard] Die Farbe „{ $color }“ ist auf „{ $background }“ schwer zu erkennen.
            }
       *[text]
            { $severity ->
                [invisible] Die Farbe „{ $color }“ ist auf „{ $background }“ fast unsichtbar.
                [hard] Die Farbe „{ $color }“ ist auf „{ $background }“ schwer zu lesen.
               *[weak] Die Farbe „{ $color }“ ist auf „{ $background }“ lesbar, aber nicht gut genug für Menschen, die schlecht sehen.
            }
    }
prefs-theme-readability-same =
    { $count ->
        [one] Die Farbe „{ $culprit }“ hat dasselbe Problem mit einer weiteren Farbe.
       *[other] Die Farbe „{ $culprit }“ hat dasselbe Problem mit { $count } weiteren Farben.
    }
prefs-theme-readability-advice =
    { $advice ->
        [either]
            { $direction ->
                [lighter]
                    { $partner-direction ->
                        [lighter] Machen Sie die Farbe „{ $culprit }“ heller oder die Farbe „{ $partner }“ heller.
                       *[darker] Machen Sie die Farbe „{ $culprit }“ heller oder die Farbe „{ $partner }“ dunkler.
                    }
               *[darker]
                    { $partner-direction ->
                        [lighter] Machen Sie die Farbe „{ $culprit }“ dunkler oder die Farbe „{ $partner }“ heller.
                       *[darker] Machen Sie die Farbe „{ $culprit }“ dunkler oder die Farbe „{ $partner }“ dunkler.
                    }
            }
        [restore] Keine hellere oder dunklere Farbe anstelle von „{ $culprit }“ reicht aus: Klicken Sie in ihrer Zeile auf „{ prefs-theme-restore }“.
       *[change]
            { $direction ->
                [lighter] Machen Sie die Farbe „{ $culprit }“ heller.
               *[darker] Machen Sie die Farbe „{ $culprit }“ dunkler.
            }
    }
## The text, lines and density rows (F2.12b2a; glossary: text_size, font_roles, equal_width_digits,
## theme_lines, density). Plain words, not designers' terms: "Rounded corners" rather than
## "corner radius". prefs-theme-pixels writes a value in pixels and prefs-theme-percent a
## percentage, $value a whole number; follow the language's habit for the space before "%" (a
## no-break space where there is one). The family names other than Inter and JetBrains Mono (the
## fonts' own names, never translated) describe Inter with digits of the same width, for numbers
## that change in place.

prefs-theme-text = Text
prefs-theme-text-size = Textgröße
prefs-theme-family-ui = Schrift der Oberfläche
prefs-theme-family-numeric = Schrift für Zahlen
prefs-theme-family-code = Schrift für Pfade und Details
prefs-theme-family-inter = Inter
prefs-theme-family-inter-tabular = Inter, gleich breite Ziffern
prefs-theme-family-mono = JetBrains Mono
prefs-theme-lines = Linien, Ecken und Schatten
prefs-theme-shape-corners = Abgerundete Ecken
prefs-theme-shape-lines = Linienstärke
prefs-theme-shape-focus = Stärke der Fokus-Dekoration
prefs-theme-shape-shadow-blur = Unschärfe des Schattens
prefs-theme-shape-shadow-distance = Abstand des Schattens
prefs-theme-shape-dialog-frame = Stärke des Dialograhmens
prefs-theme-density = Dichte
prefs-theme-density-comfortable = Komfortabel
prefs-theme-density-compact = Kompakt
prefs-theme-pixels = { $value } px
prefs-theme-percent = { $value } %
prefs-theme-delete-title = Die Darstellung „{ $name }“ löschen?
prefs-theme-delete-message = Ihre Farben gehen verloren.
dialog-delete = Löschen
theme-color-surface = Fenster-Hintergrund
theme-color-surface-raised = Bereichs-Hintergrund
theme-color-surface-sunken = Eingabefeld-Hintergrund
theme-color-surface-overlay = Menü- und Dialog-Hintergrund
theme-color-border = Rahmen und Trennlinien
theme-color-control-border = Rahmen der Bedienelemente
theme-color-text = Normaler Text
theme-color-text-muted = Sekundärer Text
theme-color-text-disabled = Deaktivierter Text
theme-color-accent = Akzentfarbe
theme-color-accent-hover = Akzentfarbe unter dem Zeiger
theme-color-accent-pressed = Akzentfarbe (gedrückt)
theme-color-on-accent = Text auf der Akzentfarbe
theme-color-accent-text = Text in der Akzentfarbe
theme-color-control = Knopf-Hintergrund
theme-color-control-hover = Knopf-Hintergrund unter dem Zeiger
theme-color-control-pressed = Knopf-Hintergrund (gedrückt)
theme-color-selection = Auswahl-Hintergrund
theme-color-on-selection = Ausgewählter Text
theme-color-focus = Fokus-Dekoration
theme-color-error = Fehler
theme-color-warning = Warnungen
theme-color-success = Erfolg
theme-color-shadow = Schatten
