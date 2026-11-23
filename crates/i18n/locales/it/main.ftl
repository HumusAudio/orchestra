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

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Ripristina predefiniti
dialog-ok = OK
