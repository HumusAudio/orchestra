# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — Spanish. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Archivo
menu-file-new = Nuevo
menu-file-open = Abrir…
menu-file-save = Guardar
menu-file-save-as = Guardar como…
menu-file-quit = Salir
menu-edit = Editar
menu-edit-undo = Deshacer
menu-edit-redo = Rehacer
menu-edit-cut = Cortar
menu-edit-copy = Copiar
menu-edit-paste = Pegar
menu-edit-select-all = Seleccionar todo
menu-edit-preferences = Preferencias…
menu-view = Vista
menu-view-score = Partitura
menu-view-tracks = Pistas
menu-view-mixer = Mezclador
menu-view-full-screen = Pantalla completa
menu-help-contents = Guía del usuario

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
