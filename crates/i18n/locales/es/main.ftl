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
menu-help-about = Acerca de { app-name }
    .mnemonic = a

## macOS's own menus (F2.11e2, ADR-0048 AM9; glossary: mac_settings … mac_bring_all_to_front).
## Shown only in the system's menu bar on macOS: the application menu (Settings, Services, Hide,
## Hide Others, Show All, Quit) and the Window menu (Minimize, Zoom, Bring All to Front). The
## names are Apple's own in each language, so they match every other Mac program; { app-name }
## is the program's name, placed where the language wants it. No mnemonics: macOS has none.

menu-mac-settings = Ajustes…
menu-mac-services = Servicios
menu-mac-hide = Ocultar { app-name }
menu-mac-hide-others = Ocultar otros
menu-mac-show-all = Mostrar todo
menu-mac-quit = Salir de { app-name }
menu-mac-window = Ventana
menu-mac-minimize = Minimizar
menu-mac-zoom = Zoom
menu-mac-bring-all-to-front = Traer todo al frente

## Our title bar's window buttons (F2.11b2, ADR-0049 TB10; glossary: window_close …
## window_minimize). Drawn only where the system draws no frame (GNOME on Wayland), so the names
## are GTK's own; they are what assistive technologies read and the buttons' tooltips.
## "restore" is the Maximise button's name while the window is maximised.

window-close = Cerrar
window-maximize = Maximizar
window-restore = Restaurar
window-minimize = Minimizar

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

## Dialog buttons (glossary: restore_defaults, ok, cancel, dialog_close)

dialog-restore-defaults = Restaurar valores predeterminados
dialog-ok = Aceptar
dialog-cancel = Cancelar
dialog-close = Cerrar

## The About dialog (F2.12a3b, ADR-0050 DG10; glossary: about, version, license, source_code,
## bundled_fonts, all_rights_reserved, logo). Its title is menu-help-about. $version is the
## program's version ("0.0.0"), $url the source repository's address; the fonts' and the
## licences' names are proper names and stay in English.

about-version = Versión { $version }
about-license = Software libre bajo la Licencia Pública General de GNU, versión 3 o posterior.
about-logo-notice = El logotipo es © 2026 Emanuele Rainoldi, todos los derechos reservados: no está cubierto por esta licencia.
about-source = Código fuente: { $url }
about-fonts = Fuentes incluidas: Inter, Bravura y JetBrains Mono, bajo la licencia SIL Open Font License 1.1.
about-logo = Logotipo de { app-name }

## The Preferences (F2.12a4a, ADR-0050 DG8–DG9; glossary: preferences, preferences_section,
## appearance, theme, theme_mode, system_mode, light, dark, blinking_cursor, theme_pairs). The
## title has no ellipsis (the menu item has one). The pairs' names are the ten built-in themes
## of ADR-0037 T1, translated where they are common words (owner's choice 8 of F2.12a4a).

prefs-title = Preferencias
prefs-sections = Secciones
prefs-section-appearance = Apariencia
prefs-theme = Tema
prefs-mode = Modo
prefs-mode-system = Sistema
prefs-mode-light = Claro
prefs-mode-dark = Oscuro
prefs-caret-blink = Cursor parpadeante
theme-absolute = Absoluto
theme-graphite = Grafito
theme-signal = Señal
theme-parchment = Pergamino
theme-slate = Pizarra
theme-studio = Estudio
theme-forest = Bosque
theme-lagoon = Laguna
theme-nocturne = Nocturno
theme-snow = Nieve
