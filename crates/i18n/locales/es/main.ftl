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

## The shortcut editor in the Preferences' Keyboard section (F2.12c1, ADR-0048 AM7; glossary:
## shortcut, command, reassign, add, remove, search, restore_defaults, key_escape). $command and
## $owner are commands' names as the menus write them ("Save as…" keeps its ellipsis); $shortcut is a
## shortcut written as the menus write it ("Ctrl+Shift+S", "⇧⌘S"); $search is what the user typed in
## the search field. prefs-shortcut-capture is shown inside the box that waits for the keys;
## prefs-shortcut-fixed under it when a key that cannot be assigned is pressed. The two questions
## come when the shortcut, or a default being restored, belongs to another command; their button
## is dialog-reassign, with dialog-cancel.

prefs-shortcuts = Atajos de teclado
prefs-shortcuts-search = Buscar comandos
prefs-shortcuts-commands = Comandos
prefs-shortcuts-no-match = Ningún comando coincide con «{ $search }».
prefs-shortcuts-of = Atajos de «{ $command }»
prefs-shortcuts-none = Ningún atajo
prefs-shortcut-add = Añadir…
prefs-shortcut-remove = Quitar
prefs-shortcuts-restore-all = Restablecer todos los atajos
prefs-shortcut-capture = Pulse el atajo, o Esc para cancelar
prefs-shortcut-capture-name = Nuevo atajo para «{ $command }»
prefs-shortcut-fixed = { $shortcut } no se puede asignar: sirve para moverse por el programa, para los menús o para el sistema.
prefs-shortcut-conflict-title = { $shortcut } ya está asignado a «{ $owner }»
prefs-shortcut-conflict-message = ¿Asignarlo a «{ $command }» en su lugar? «{ $owner }» dejará de tenerlo.
prefs-shortcut-restore-conflict-title = Algunos atajos predeterminados de «{ $command }» están asignados a otros comandos
prefs-shortcut-restore-conflict-message = ¿Devolverlos a «{ $command }»? Los otros comandos dejarán de tenerlos.
dialog-reassign = Reasignar

## Importing and exporting the shortcuts (F2.12c2, ADR-0051; glossary: import, export, shortcut,
## replace). $file is a file's whole name ("Mine.orchestra-shortcuts"); $count a number of entries.
## file-shortcuts-name is the name proposed when exporting, without the extension (the program adds
## ".orchestra-shortcuts"). The question comes before an import replaces shortcuts the user had
## changed (buttons: dialog-replace, dialog-cancel). The report comes after an import when some
## entries of the file could not be used. The errors' other messages are those of the project's
## (project-open-error-*, project-save-error-*).

prefs-shortcuts-import = Importar…
prefs-shortcuts-export = Exportar…
file-filter-shortcuts = Atajos de { app-name }
file-import-shortcuts-title = Importar atajos de teclado
file-export-shortcuts-title = Exportar atajos de teclado
file-shortcuts-name = Atajos de teclado
prefs-shortcuts-import-question-title = ¿Sustituir sus atajos por los de «{ $file }»?
prefs-shortcuts-import-question-message = Se perderán los atajos que ha cambiado.
prefs-shortcuts-imported-title = Atajos importados
prefs-shortcuts-imported-unused = Entradas de «{ $file }» no usadas: { $count }
prefs-shortcuts-import-error-title = No se puede importar «{ $file }»
prefs-shortcuts-import-error-not-shortcuts = El archivo no es un archivo de atajos de { app-name }.
prefs-shortcuts-import-error-newer = El archivo se guardó con una versión más nueva de { app-name }. Instale la última versión para importarlo.
prefs-shortcuts-import-error-damaged = El archivo está dañado: no se puede leer como atajos de { app-name }.
prefs-shortcuts-export-error-title = No se puede exportar «{ $file }»

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

prefs-theme-duplicate = Duplicar
prefs-theme-delete = Eliminar
prefs-theme-name = Nombre
prefs-theme-copy-name = { $name } (copia)
prefs-theme-colors = Colores
prefs-theme-builtin = Los temas integrados no se pueden cambiar: duplique uno para crear uno propio.
prefs-theme-editing-light = Estos son los colores de la mitad clara. Para cambiar la mitad oscura, elija Oscuro en Modo.
prefs-theme-editing-dark = Estos son los colores de la mitad oscura. Para cambiar la mitad clara, elija Claro en Modo.
prefs-theme-restore = Restaurar
prefs-theme-value-name = Valor de «{ $color }»
prefs-theme-readability = Legibilidad
prefs-theme-readability-ok = Todos los textos y controles se leen bien.
prefs-theme-readability-problem =
    { $kind ->
        [graphic]
            { $severity ->
                [invisible] El color «{ $color }» es casi invisible sobre «{ $background }».
               *[hard] El color «{ $color }» se distingue con dificultad sobre «{ $background }».
            }
       *[text]
            { $severity ->
                [invisible] El color «{ $color }» es casi invisible sobre «{ $background }».
                [hard] El color «{ $color }» se lee con dificultad sobre «{ $background }».
               *[weak] El color «{ $color }» se lee sobre «{ $background }», pero no lo bastante bien para quien ve poco.
            }
    }
prefs-theme-readability-same =
    { $count ->
        [one] El color «{ $culprit }» tiene el mismo problema con otro color.
       *[other] El color «{ $culprit }» tiene el mismo problema con otros { $count } colores.
    }
prefs-theme-readability-advice =
    { $advice ->
        [either]
            { $direction ->
                [lighter]
                    { $partner-direction ->
                        [lighter] Aclare el color «{ $culprit }», o aclare el color «{ $partner }».
                       *[darker] Aclare el color «{ $culprit }», o oscurezca el color «{ $partner }».
                    }
               *[darker]
                    { $partner-direction ->
                        [lighter] Oscurezca el color «{ $culprit }», o aclare el color «{ $partner }».
                       *[darker] Oscurezca el color «{ $culprit }», o oscurezca el color «{ $partner }».
                    }
            }
        [restore] Ningún color más claro o más oscuro en lugar de «{ $culprit }» es suficiente: pulse «{ prefs-theme-restore }» en su fila.
       *[change]
            { $direction ->
                [lighter] Aclare el color «{ $culprit }».
               *[darker] Oscurezca el color «{ $culprit }».
            }
    }
## The text, lines and density rows (F2.12b2a; glossary: text_size, font_roles, equal_width_digits,
## theme_lines, density). Plain words, not designers' terms: "Rounded corners" rather than
## "corner radius". prefs-theme-pixels writes a value in pixels and prefs-theme-percent a
## percentage, $value a whole number; follow the language's habit for the space before "%" (a
## no-break space where there is one). The family names other than Inter and JetBrains Mono (the
## fonts' own names, never translated) describe Inter with digits of the same width, for numbers
## that change in place.

prefs-theme-text = Texto
prefs-theme-text-size = Tamaño del texto
prefs-theme-family-ui = Fuente de la interfaz
prefs-theme-family-numeric = Fuente de los números
prefs-theme-family-code = Fuente de las rutas y los detalles
prefs-theme-family-inter = Inter
prefs-theme-family-inter-tabular = Inter, cifras de igual anchura
prefs-theme-family-mono = JetBrains Mono
prefs-theme-lines = Líneas, esquinas y sombras
prefs-theme-shape-corners = Esquinas redondeadas
prefs-theme-shape-lines = Grosor de las líneas
prefs-theme-shape-focus = Grosor de la decoración del foco
prefs-theme-shape-shadow-blur = Desenfoque de la sombra
prefs-theme-shape-shadow-distance = Distancia de la sombra
prefs-theme-shape-dialog-frame = Grosor del marco de los diálogos
prefs-theme-density = Densidad
prefs-theme-density-comfortable = Cómoda
prefs-theme-density-compact = Compacta
prefs-theme-pixels = { $value } px
prefs-theme-percent = { $value } %
prefs-theme-delete-title = ¿Eliminar el tema «{ $name }»?
prefs-theme-delete-message = Se perderán sus colores.
dialog-delete = Eliminar
theme-color-surface = Fondo de las ventanas
theme-color-surface-raised = Fondo de los paneles
theme-color-surface-sunken = Fondo de los campos
theme-color-surface-overlay = Fondo de menús y diálogos
theme-color-border = Bordes y separadores
theme-color-control-border = Bordes de los controles
theme-color-text = Texto normal
theme-color-text-muted = Texto secundario
theme-color-text-disabled = Texto desactivado
theme-color-accent = Color de énfasis
theme-color-accent-hover = Color de énfasis bajo el puntero
theme-color-accent-pressed = Color de énfasis al pulsar
theme-color-on-accent = Texto sobre el color de énfasis
theme-color-accent-text = Texto en color de énfasis
theme-color-control = Fondo de los botones
theme-color-control-hover = Fondo de los botones bajo el puntero
theme-color-control-pressed = Fondo de los botones al pulsar
theme-color-selection = Fondo de las selecciones
theme-color-on-selection = Texto seleccionado
theme-color-focus = Decoración del foco
theme-color-error = Errores
theme-color-warning = Advertencias
theme-color-success = Éxito
theme-color-shadow = Sombra
