# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — Italian. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = File
    .mnemonic = f
menu-file-new = Nuovo
    .mnemonic = n
menu-file-open = Apri…
    .mnemonic = a
menu-file-save = Salva
    .mnemonic = s
menu-file-save-as = Salva come…
    .mnemonic = c
menu-file-quit = Esci
    .mnemonic = e
menu-edit = Modifica
    .mnemonic = m
menu-edit-undo = Annulla
    .mnemonic = a
menu-edit-redo = Ripristina
    .mnemonic = r
menu-edit-cut = Taglia
    .mnemonic = g
menu-edit-copy = Copia
    .mnemonic = c
menu-edit-paste = Incolla
    .mnemonic = i
menu-edit-select-all = Seleziona tutto
    .mnemonic = t
menu-edit-preferences = Preferenze…
    .mnemonic = p
menu-view = Vista
    .mnemonic = v
menu-view-score = Partitura
menu-view-tracks = Tracce
menu-view-mixer = Mixer
menu-view-full-screen = Schermo intero
    .mnemonic = s
menu-help = Aiuto
    .mnemonic = i
menu-help-contents = Guida per l'utente
    .mnemonic = g
menu-help-about = Informazioni su { app-name }
    .mnemonic = i

## macOS's own menus (F2.11e2, ADR-0048 AM9; glossary: mac_settings … mac_bring_all_to_front).
## Shown only in the system's menu bar on macOS: the application menu (Settings, Services, Hide,
## Hide Others, Show All, Quit) and the Window menu (Minimize, Zoom, Bring All to Front). The
## names are Apple's own in each language, so they match every other Mac program; { app-name }
## is the program's name, placed where the language wants it. No mnemonics: macOS has none.

menu-mac-settings = Impostazioni…
menu-mac-services = Servizi
menu-mac-hide = Nascondi { app-name }
menu-mac-hide-others = Nascondi altre
menu-mac-show-all = Mostra tutte
menu-mac-quit = Esci da { app-name }
menu-mac-window = Finestra
menu-mac-minimize = Contrai
menu-mac-zoom = Ridimensiona
menu-mac-bring-all-to-front = Porta tutto in primo piano

## Our title bar's window buttons (F2.11b2, ADR-0049 TB10; glossary: window_close …
## window_minimize). Drawn only where the system draws no frame (GNOME on Wayland), so the names
## are GTK's own; they are what assistive technologies read and the buttons' tooltips.
## "restore" is the Maximise button's name while the window is maximised.

window-close = Chiudi
window-maximize = Massimizza
window-restore = Ripristina
window-minimize = Minimizza

## Keys' names in shortcuts, as menus write them next to an item, joined by "+" (glossary:
## key_ctrl … key_numpad; ADR-0048 AM12, F2.11d2a). $number is a function key's number (1–24),
## $digit a digit of the numeric keypad (0–9).

key-ctrl = Ctrl
key-shift = Maiusc
key-alt = Alt
key-super = Super
key-windows = Win
key-enter = Invio
key-escape = Esc
key-tab = Tab
key-space = Spazio
key-backspace = Backspace
key-delete = Canc
key-insert = Ins
key-home = Home
key-end = Fine
key-page-up = PgSu
key-page-down = PgGiù
key-left = Sinistra
key-right = Destra
key-up = Su
key-down = Giù
key-menu = Menu
key-function = F{ $number }
key-numpad = Num { $digit }

## Status bar (glossary: measure → "battuta", feminine: the participle agrees)

status-measures-selected =
    { $count ->
        [one] { $count } battuta selezionata
       *[other] { $count } battute selezionate
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Riproduzione
playback-metronome = Metronomo
playback-count-in = Conteggio iniziale
playback-metronome-volume = Volume del metronomo
playback-track-name = Nome traccia
playback-track-name-placeholder = Senza nome

## Dialog buttons (glossary: restore_defaults, ok, cancel, dialog_close)

dialog-restore-defaults = Ripristina predefiniti
dialog-ok = OK
dialog-cancel = Annulla
dialog-close = Chiudi

## The About dialog (F2.12a3b, ADR-0050 DG10; glossary: about, version, license, source_code,
## bundled_fonts, all_rights_reserved, logo). Its title is menu-help-about. $version is the
## program's version ("0.0.0"), $url the source repository's address; the fonts' and the
## licences' names are proper names and stay in English.

about-version = Versione { $version }
about-license = Software libero con Licenza GNU General Public versione 3 o successive.
about-logo-notice = Il logo è © 2026 Emanuele Rainoldi, tutti i diritti riservati: non è coperto da questa licenza.
about-source = Codice sorgente: { $url }
about-fonts = Tipi di carattere inclusi: Inter, Bravura e JetBrains Mono, con licenza SIL Open Font License 1.1.
about-logo = Logo di { app-name }

## The Preferences (F2.12a4a, ADR-0050 DG8–DG9; glossary: preferences, preferences_section,
## appearance, theme, theme_mode, system_mode, light, dark, blinking_cursor, theme_pairs). The
## title has no ellipsis (the menu item has one). The pairs' names are the ten built-in themes
## of ADR-0037 T1, translated where they are common words (owner's choice 8 of F2.12a4a).

prefs-title = Preferenze
prefs-sections = Sezioni
prefs-section-appearance = Aspetto
prefs-theme = Tema
prefs-mode = Modalità
prefs-mode-system = Sistema
prefs-mode-light = Chiaro
prefs-mode-dark = Scuro
prefs-caret-blink = Cursore lampeggiante
theme-absolute = Assoluto
theme-graphite = Grafite
theme-signal = Segnale
theme-parchment = Pergamena
theme-slate = Ardesia
theme-studio = Studio
theme-forest = Foresta
theme-lagoon = Laguna
theme-nocturne = Notturno
theme-snow = Neve

## The Preferences' Language and Keyboard sections (F2.12a4b1; glossary: language, keyboard,
## system_language, fine_adjustment, language_names). The languages' names are each written in
## its own language, the same in every catalogue (owner's choice 1 of F2.12a4b1). Until F2.13 a
## new language applies at the next start, which the note beside the choice says.

prefs-section-language = Lingua
prefs-section-keyboard = Tastiera
prefs-language = Lingua
prefs-language-system = Lingua del sistema
prefs-language-restart = Vale dal prossimo avvio di { app-name }.
prefs-fine-modifier = Regolazione fine
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

prefs-section-playback = Riproduzione
prefs-playback-intro = Come vengono suonati gli accenti e i segni dinamici. Questi valori sono personali: valgono per tutti i progetti.
prefs-playback-articulations = Accenti di articolazione
prefs-playback-dynamic-accents = Accenti dinamici
prefs-playback-changes = Cambi di livello
prefs-playback-louder = Più forte di
prefs-playback-back = Torna in
prefs-playback-held = Tenuto per
prefs-playback-change = Cambia in
prefs-playback-ms = { $value } ms
prefs-playback-step = { $value }
articulation-soft-accent = Accento leggero
articulation-stress = Accentuato
articulation-accent = Accento
articulation-marcato = Marcato
articulation-unstress = Non accentato
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

file-untitled = Senza titolo
file-filter-project = Progetto { app-name }
file-open-title = Apri progetto
file-save-title = Salva progetto come
file-chooser-unavailable-title = Impossibile scegliere i file
file-chooser-unavailable-message = { app-name } usa la finestra del desktop per scegliere i file, ma nessun servizio del desktop la fornisce. Serve il pacchetto xdg-desktop-portal insieme a quello del desktop in uso (per esempio xdg-desktop-portal-kde o xdg-desktop-portal-gnome).
