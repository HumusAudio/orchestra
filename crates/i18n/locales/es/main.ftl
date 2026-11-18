# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — Spanish. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Archivo
menu-file-new = Nuevo
menu-file-open = Abrir
menu-file-save = Guardar
menu-file-save-as = Guardar como
menu-file-quit = Salir
menu-edit = Editar
menu-edit-undo = Deshacer
menu-edit-redo = Rehacer
menu-edit-cut = Cortar
menu-edit-copy = Copiar
menu-edit-paste = Pegar
menu-edit-select-all = Seleccionar todo
menu-edit-preferences = Preferencias
menu-view = Vista
menu-view-score = Partitura
menu-view-tracks = Pistas
menu-view-mixer = Mezclador
menu-view-full-screen = Pantalla completa
menu-help-contents = Ayuda

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
