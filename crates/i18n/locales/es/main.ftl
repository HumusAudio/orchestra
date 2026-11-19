# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — Spanish. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Archivo
    .mnemonic = a
menu-file-new = Nuevo
    .mnemonic = n
menu-file-open = Abrir…
    .mnemonic = a
menu-file-save = Guardar
    .mnemonic = g
menu-file-save-as = Guardar como…
    .mnemonic = c
menu-file-quit = Salir
    .mnemonic = s
menu-edit = Editar
    .mnemonic = e
menu-edit-undo = Deshacer
    .mnemonic = d
menu-edit-redo = Rehacer
    .mnemonic = r
menu-edit-cut = Cortar
    .mnemonic = t
menu-edit-copy = Copiar
    .mnemonic = c
menu-edit-paste = Pegar
    .mnemonic = p
menu-edit-select-all = Seleccionar todo
    .mnemonic = s
menu-edit-preferences = Preferencias…
    .mnemonic = e
menu-view = Vista
    .mnemonic = v
menu-view-score = Partitura
menu-view-tracks = Pistas
menu-view-mixer = Mezclador
menu-view-full-screen = Pantalla completa
    .mnemonic = c
menu-help = Ayuda
    .mnemonic = u
menu-help-contents = Guía del usuario
    .mnemonic = g

## Keys' names in shortcuts, as menus write them next to an item, joined by "+" (glossary:
## key_ctrl … key_numpad; ADR-0048 AM12, F2.11d2a). $number is a function key's number (1–24),
## $digit a digit of the numeric keypad (0–9).

key-ctrl = Ctrl
key-shift = Mayús
key-alt = Alt
key-super = Super
key-windows = Win
key-enter = Intro
key-escape = Esc
key-tab = Tab
key-space = Espacio
key-backspace = Retroceso
key-delete = Supr
key-insert = Insert
key-home = Inicio
key-end = Fin
key-page-up = Re Pág
key-page-down = Av Pág
key-left = Izquierda
key-right = Derecha
key-up = Arriba
key-down = Abajo
key-menu = Menú
key-function = F{ $number }
key-numpad = Núm { $digit }

## Status bar (glossary: measure → "compás", masculine, plural "compases")

status-measures-selected =
    { $count ->
        [one] { $count } compás seleccionado
       *[other] { $count } compases seleccionados
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Reproducción
playback-metronome = Metrónomo
playback-count-in = Cuenta inicial
playback-metronome-volume = Volumen del metrónomo
playback-track-name = Nombre de pista
playback-track-name-placeholder = Sin nombre

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Restaurar valores predeterminados
dialog-ok = Aceptar
