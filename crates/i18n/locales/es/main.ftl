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

## The Preferences' Language and Keyboard sections (F2.12a4b1; glossary: language, keyboard,
## system_language, fine_adjustment, language_names). The languages' names are each written in
## its own language, the same in every catalogue (owner's choice 1 of F2.12a4b1). Until F2.13 a
## new language applies at the next start, which the note beside the choice says.

prefs-section-language = Idioma
prefs-section-keyboard = Teclado
prefs-language = Idioma
prefs-language-system = Idioma del sistema
prefs-language-restart = Se aplica la próxima vez que se inicie { app-name }.
prefs-fine-modifier = Ajuste fino
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

prefs-section-playback = Reproducción
prefs-playback-intro = Cómo se tocan los acentos y los matices dinámicos. Estos valores son personales: se aplican a todos los proyectos.
prefs-playback-articulations = Acentos de articulación
prefs-playback-dynamic-accents = Acentos dinámicos
prefs-playback-changes = Cambios de nivel
prefs-playback-louder = Más fuerte en
prefs-playback-back = Vuelve en
prefs-playback-held = Sostenido
prefs-playback-change = Cambia en
prefs-playback-ms = { $value } ms
prefs-playback-step = { $value }
articulation-soft-accent = Acento suave
articulation-stress = Tiempo fuerte
articulation-accent = Acento
articulation-marcato = Marcato
articulation-unstress = Tiempo débil
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

file-untitled = Sin título
file-filter-project = Proyecto de { app-name }
file-open-title = Abrir proyecto
file-save-title = Guardar proyecto como
file-chooser-unavailable-title = No se pueden elegir archivos
file-chooser-unavailable-message = { app-name } usa la ventana del escritorio para elegir archivos, pero ningún servicio del escritorio la ofrece. Hace falta el paquete xdg-desktop-portal junto con el del escritorio en uso (por ejemplo, xdg-desktop-portal-kde o xdg-desktop-portal-gnome).

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
project-save-changes-title = ¿Guardar los cambios en «{ $name }»?
project-save-changes-message = Los cambios se perderán si no se guardan.
dialog-save = Guardar
dialog-dont-save = No guardar
project-replace-title = Ya existe un archivo llamado «{ $file }». ¿Quiere reemplazarlo?
project-replace-message = Si lo reemplaza, se sobrescribirá su contenido.
dialog-replace = Reemplazar
project-new-error-title = No se puede crear un proyecto nuevo
project-new-error-message = Es un error de { app-name }: por favor, notifíquelo.
project-open-error-title = No se puede abrir «{ $file }»
project-open-error-not-found = Este archivo no existe o no está accesible en este momento.
project-open-error-permission = { app-name } no tiene permiso para leer este archivo.
project-open-error-read = No se ha podido leer el archivo.
project-open-error-too-large = El archivo es más grande de lo que { app-name } puede abrir.
project-open-error-not-project = El archivo no es un proyecto de { app-name }.
project-open-error-newer = El proyecto se guardó con una versión más nueva de { app-name }. Instale la última versión para abrirlo.
project-open-error-damaged = El archivo está dañado: una parte no se puede leer como proyecto de { app-name }.
project-save-error-title = No se puede guardar «{ $file }»
project-save-error-permission = { app-name } no tiene permiso para escribir este archivo o su carpeta.
project-save-error-full = No hay suficiente espacio en el disco.
project-save-error-write = No se ha podido escribir el archivo.
project-save-error-internal = No se ha podido preparar el proyecto para guardarlo. Es un error de { app-name }: por favor, notifíquelo.
project-save-error-kept = El archivo que ya estaba allí, si lo había, se ha dejado como estaba.
