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
project-save-changes-title = Salvare le modifiche a «{ $name }»?
project-save-changes-message = Le modifiche andranno perse se non le salvi.
dialog-save = Salva
dialog-dont-save = Non salvare
project-replace-title = Esiste già un file con nome «{ $file }». Sostituirlo?
project-replace-message = Sostituendolo, il suo contenuto verrà sovrascritto.
dialog-replace = Sostituisci
project-new-error-title = Impossibile creare un nuovo progetto
project-new-error-message = È un errore di { app-name }: per favore segnalalo.
project-open-error-title = Impossibile aprire «{ $file }»
project-open-error-not-found = Il file non esiste o non è accessibile al momento.
project-open-error-permission = { app-name } non ha il permesso di leggere questo file.
project-open-error-read = Non è stato possibile leggere il file.
project-open-error-too-large = Il file è più grande di quanto { app-name } possa aprire.
project-open-error-not-project = Il file non è un progetto di { app-name }.
project-open-error-newer = Il progetto è stato salvato con una versione più recente di { app-name }. Installa l'ultima versione per aprirlo.
project-open-error-damaged = Il file è danneggiato: una sua parte non si può leggere come progetto di { app-name }.
project-save-error-title = Impossibile salvare «{ $file }»
project-save-error-permission = { app-name } non ha il permesso di scrivere questo file o la sua cartella.
project-save-error-full = Non c'è abbastanza spazio sul disco.
project-save-error-write = Non è stato possibile scrivere il file.
project-save-error-internal = Non è stato possibile preparare il progetto per il salvataggio. È un errore di { app-name }: per favore segnalalo.
project-save-error-kept = Il file che c'era già, se c'era, è rimasto com'era.

## The shortcut editor in the Preferences' Keyboard section (F2.12c1, ADR-0048 AM7; glossary:
## shortcut, command, reassign, add, remove, search, restore_defaults, key_escape). $command and
## $owner are commands' names as the menus write them ("Save as…" keeps its ellipsis); $shortcut is a
## shortcut written as the menus write it ("Ctrl+Shift+S", "⇧⌘S"); $search is what the user typed in
## the search field. prefs-shortcut-capture is shown inside the box that waits for the keys;
## prefs-shortcut-fixed under it when a key that cannot be assigned is pressed. The two questions
## come when the shortcut, or a default being restored, belongs to another command; their button
## is dialog-reassign, with dialog-cancel.

prefs-shortcuts = Scorciatoie
prefs-shortcuts-search = Cerca comandi
prefs-shortcuts-commands = Comandi
prefs-shortcuts-no-match = Nessun comando corrisponde a «{ $search }».
prefs-shortcuts-of = Scorciatoie di «{ $command }»
prefs-shortcuts-none = Nessuna scorciatoia
prefs-shortcut-add = Aggiungi…
prefs-shortcut-remove = Rimuovi
prefs-shortcuts-restore-all = Ripristina tutte le scorciatoie
prefs-shortcut-capture = Premi la scorciatoia, oppure Esc per annullare
prefs-shortcut-capture-name = Nuova scorciatoia per «{ $command }»
prefs-shortcut-fixed = { $shortcut } non si può assegnare: serve per spostarsi nel programma, per i menu o al sistema.
prefs-shortcut-conflict-title = { $shortcut } è già assegnata a «{ $owner }»
prefs-shortcut-conflict-message = Assegnarla invece a «{ $command }»? «{ $owner }» non l'avrà più.
prefs-shortcut-restore-conflict-title = Alcune scorciatoie predefinite di «{ $command }» sono assegnate ad altri comandi
prefs-shortcut-restore-conflict-message = Restituirle a «{ $command }»? Gli altri comandi non le avranno più.
dialog-reassign = Riassegna

## Importing and exporting the shortcuts (F2.12c2, ADR-0051; glossary: import, export, shortcut,
## replace). $file is a file's whole name ("Mine.orchestra-shortcuts"); $count a number of entries.
## file-shortcuts-name is the name proposed when exporting, without the extension (the program adds
## ".orchestra-shortcuts"). The question comes before an import replaces shortcuts the user had
## changed (buttons: dialog-replace, dialog-cancel). The report comes after an import when some
## entries of the file could not be used. The errors' other messages are those of the project's
## (project-open-error-*, project-save-error-*).

prefs-shortcuts-import = Importa…
prefs-shortcuts-export = Esporta…
file-filter-shortcuts = Scorciatoie di { app-name }
file-import-shortcuts-title = Importa scorciatoie
file-export-shortcuts-title = Esporta scorciatoie
file-shortcuts-name = Scorciatoie
prefs-shortcuts-import-question-title = Sostituire le tue scorciatoie con quelle di «{ $file }»?
prefs-shortcuts-import-question-message = Le scorciatoie che hai cambiato andranno perse.
prefs-shortcuts-imported-title = Scorciatoie importate
prefs-shortcuts-imported-unused = Voci di «{ $file }» non usate: { $count }
prefs-shortcuts-import-error-title = Impossibile importare «{ $file }»
prefs-shortcuts-import-error-not-shortcuts = Il file non è un file di scorciatoie di { app-name }.
prefs-shortcuts-import-error-newer = Il file è stato salvato con una versione più recente di { app-name }. Installa l'ultima versione per importarlo.
prefs-shortcuts-import-error-damaged = Il file è danneggiato: non si può leggere come scorciatoie di { app-name }.
prefs-shortcuts-export-error-title = Impossibile esportare «{ $file }»

## The theme editor in the Preferences' Appearance section (F2.12b1, ADR-0037 T7–T8; glossary:
## theme_color_names, accent_color, duplicate, delete, color, decimal_separator). The colour names
## are those of the theme's tokens, as the editor lists them; the user picks a value for each.
## number-decimal-separator is the character written between the whole part and the decimals of a
## number in this language ("3.2" or "3,2"): the program writes the contrast ratios with it,
## because Fluent does not localise decimals. $name is a theme's name; $color, $foreground and
## $background are colour names (theme-color-*); $ratio and $minimum are ratios already written
## with one decimal ("3,2"). The editing notes name the Mode control and its values
## (prefs-mode, prefs-mode-light, prefs-mode-dark): keep them identical.

number-decimal-separator = ,
prefs-theme-duplicate = Duplica
prefs-theme-delete = Elimina
prefs-theme-name = Nome
prefs-theme-copy-name = { $name } (copia)
prefs-theme-colors = Colori
prefs-theme-builtin = I temi integrati non si possono cambiare: duplicane uno per farne uno tuo.
prefs-theme-editing-light = Questi sono i colori della metà chiara. Per cambiare la metà scura, scegli Scuro in Modalità.
prefs-theme-editing-dark = Questi sono i colori della metà scura. Per cambiare la metà chiara, scegli Chiaro in Modalità.
prefs-theme-restore = Ripristina
prefs-theme-value-name = Valore di «{ $color }»
prefs-theme-contrast = Contrasto
prefs-theme-contrast-ok = Tutti i testi e i controlli hanno abbastanza contrasto.
prefs-theme-contrast-low = { $foreground } su { $background }: { $ratio }:1, almeno { $minimum }:1
prefs-theme-delete-title = Eliminare il tema «{ $name }»?
prefs-theme-delete-message = I suoi colori andranno persi.
dialog-delete = Elimina
theme-color-surface = Sfondo delle finestre
theme-color-surface-raised = Sfondo dei pannelli
theme-color-surface-sunken = Sfondo dei campi
theme-color-surface-overlay = Sfondo di menu e finestre di dialogo
theme-color-border = Bordi e separatori
theme-color-control-border = Bordi dei controlli
theme-color-text = Testo normale
theme-color-text-muted = Testo secondario
theme-color-text-disabled = Testo disabilitato
theme-color-accent = Colore in evidenza
theme-color-accent-hover = Colore in evidenza sotto il puntatore
theme-color-accent-pressed = Colore in evidenza premuto
theme-color-on-accent = Testo sul colore in evidenza
theme-color-accent-text = Testo nel colore in evidenza
theme-color-control = Sfondo dei pulsanti
theme-color-control-hover = Sfondo dei pulsanti sotto il puntatore
theme-color-control-pressed = Sfondo dei pulsanti premuti
theme-color-selection = Sfondo delle selezioni
theme-color-on-selection = Testo selezionato
theme-color-focus = Decorazione del focus
theme-color-error = Errori
theme-color-warning = Avvisi
theme-color-success = Esito positivo
theme-color-shadow = Ombra
