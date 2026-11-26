# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — French. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Fichier
    .mnemonic = f
menu-file-new = Nouveau
    .mnemonic = n
menu-file-open = Ouvrir…
    .mnemonic = o
menu-file-save = Enregistrer
    .mnemonic = s
menu-file-save-as = Enregistrer sous…
    .mnemonic = g
menu-file-quit = Quitter
    .mnemonic = q
menu-edit = Éditer
    .mnemonic = é
menu-edit-undo = Annuler
    .mnemonic = a
menu-edit-redo = Rétablir
    .mnemonic = r
menu-edit-cut = Couper
    .mnemonic = u
menu-edit-copy = Copier
    .mnemonic = c
menu-edit-paste = Coller
    .mnemonic = e
menu-edit-select-all = Tout sélectionner
    .mnemonic = t
menu-edit-preferences = Préférences…
    .mnemonic = p
menu-view = Affichage
    .mnemonic = a
menu-view-score = Partition
menu-view-tracks = Pistes
menu-view-mixer = Table de mixage
menu-view-full-screen = Plein écran
    .mnemonic = é
menu-help = Aide
    .mnemonic = d
menu-help-contents = Guide de l'utilisateur
    .mnemonic = g
menu-help-about = À propos d'{ app-name }
    .mnemonic = p

## macOS's own menus (F2.11e2, ADR-0048 AM9; glossary: mac_settings … mac_bring_all_to_front).
## Shown only in the system's menu bar on macOS: the application menu (Settings, Services, Hide,
## Hide Others, Show All, Quit) and the Window menu (Minimize, Zoom, Bring All to Front). The
## names are Apple's own in each language, so they match every other Mac program; { app-name }
## is the program's name, placed where the language wants it. No mnemonics: macOS has none.

menu-mac-settings = Réglages…
menu-mac-services = Services
menu-mac-hide = Masquer { app-name }
menu-mac-hide-others = Masquer les autres
menu-mac-show-all = Tout afficher
menu-mac-quit = Quitter { app-name }
menu-mac-window = Fenêtre
menu-mac-minimize = Placer dans le Dock
menu-mac-zoom = Zoom
menu-mac-bring-all-to-front = Tout ramener au premier plan

## Our title bar's window buttons (F2.11b2, ADR-0049 TB10; glossary: window_close …
## window_minimize). Drawn only where the system draws no frame (GNOME on Wayland), so the names
## are GTK's own; they are what assistive technologies read and the buttons' tooltips.
## "restore" is the Maximise button's name while the window is maximised.

window-close = Fermer
window-maximize = Maximiser
window-restore = Restaurer
window-minimize = Réduire

## Keys' names in shortcuts, as menus write them next to an item, joined by "+" (glossary:
## key_ctrl … key_numpad; ADR-0048 AM12, F2.11d2a). $number is a function key's number (1–24),
## $digit a digit of the numeric keypad (0–9).

key-ctrl = Ctrl
key-shift = Maj
key-alt = Alt
key-super = Super
key-windows = Win
key-enter = Entrée
key-escape = Échap
key-tab = Tab
key-space = Espace
key-backspace = Retour arrière
key-delete = Suppr
key-insert = Inser
key-home = Origine
key-end = Fin
key-page-up = Pg. préc
key-page-down = Pg. suiv
key-left = Gauche
key-right = Droite
key-up = Haut
key-down = Bas
key-menu = Menu
key-function = F{ $number }
key-numpad = Num { $digit }

## Status bar (glossary: measure → "mesure", feminine). French CLDR plural "one" covers 0 and 1,
## so "0 mesure sélectionnée" is produced, which is correct French.

status-measures-selected =
    { $count ->
        [one] { $count } mesure sélectionnée
       *[other] { $count } mesures sélectionnées
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Lecture
playback-metronome = Métronome
playback-count-in = Décompte
playback-metronome-volume = Volume du métronome
playback-track-name = Nom de la piste
playback-track-name-placeholder = Sans nom

## Dialog buttons (glossary: restore_defaults, ok, cancel, dialog_close)

dialog-restore-defaults = Réinitialiser aux valeurs par défaut
dialog-ok = OK
dialog-cancel = Annuler
dialog-close = Fermer

## The About dialog (F2.12a3b, ADR-0050 DG10; glossary: about, version, license, source_code,
## bundled_fonts, all_rights_reserved, logo). Its title is menu-help-about. $version is the
## program's version ("0.0.0"), $url the source repository's address; the fonts' and the
## licences' names are proper names and stay in English.

about-version = Version { $version }
about-license = Logiciel libre distribué sous la Licence publique générale GNU, version 3 ou ultérieure.
about-logo-notice = Le logo est © 2026 Emanuele Rainoldi, tous droits réservés : il n'est pas couvert par cette licence.
about-source = Code source : { $url }
about-fonts = Polices incluses : Inter, Bravura et JetBrains Mono, sous licence SIL Open Font License 1.1.
about-logo = Logo d'{ app-name }

## The Preferences (F2.12a4a, ADR-0050 DG8–DG9; glossary: preferences, preferences_section,
## appearance, theme, theme_mode, system_mode, light, dark, blinking_cursor, theme_pairs). The
## title has no ellipsis (the menu item has one). The pairs' names are the ten built-in themes
## of ADR-0037 T1, translated where they are common words (owner's choice 8 of F2.12a4a).

prefs-title = Préférences
prefs-sections = Sections
prefs-section-appearance = Apparence
prefs-theme = Thème
prefs-mode = Mode
prefs-mode-system = Système
prefs-mode-light = Clair
prefs-mode-dark = Sombre
prefs-caret-blink = Curseur clignotant
theme-absolute = Absolu
theme-graphite = Graphite
theme-signal = Signal
theme-parchment = Parchemin
theme-slate = Ardoise
theme-studio = Studio
theme-forest = Forêt
theme-lagoon = Lagon
theme-nocturne = Nocturne
theme-snow = Neige

## The Preferences' Language and Keyboard sections (F2.12a4b1; glossary: language, keyboard,
## system_language, fine_adjustment, language_names). The languages' names are each written in
## its own language, the same in every catalogue (owner's choice 1 of F2.12a4b1). Until F2.13 a
## new language applies at the next start, which the note beside the choice says.

prefs-section-language = Langue
prefs-section-keyboard = Clavier
prefs-language = Langue
prefs-language-system = Langue du système
prefs-language-restart = S'applique au prochain démarrage d'{ app-name }.
prefs-fine-modifier = Réglage fin
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

prefs-section-playback = Lecture
prefs-playback-intro = Comment les accents et les nuances sont joués. Ces valeurs sont personnelles : elles valent pour tous les projets.
prefs-playback-articulations = Accents d'articulation
prefs-playback-dynamic-accents = Accents de nuance
prefs-playback-changes = Changements de niveau
prefs-playback-louder = Plus fort de
prefs-playback-back = Retour en
prefs-playback-held = Tenu
prefs-playback-change = Changement en
prefs-playback-ms = { $value } ms
prefs-playback-step = { $value }
articulation-soft-accent = Accent adouci
articulation-stress = Accentué
articulation-accent = Accent
articulation-marcato = Marcato
articulation-unstress = Inaccentué
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

file-untitled = Sans titre
file-filter-project = Projet { app-name }
file-open-title = Ouvrir un projet
file-save-title = Enregistrer le projet sous
file-chooser-unavailable-title = Impossible de choisir des fichiers
file-chooser-unavailable-message = { app-name } utilise la fenêtre du bureau pour choisir les fichiers, mais aucun service du bureau ne la fournit. Il faut le paquet xdg-desktop-portal avec celui du bureau utilisé (par exemple xdg-desktop-portal-kde ou xdg-desktop-portal-gnome).

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
project-save-changes-title = Enregistrer les modifications de « { $name } » ?
project-save-changes-message = Vos modifications seront perdues si vous ne les enregistrez pas.
dialog-save = Enregistrer
dialog-dont-save = Ne pas enregistrer
project-replace-title = Un fichier nommé « { $file } » existe déjà. Voulez-vous le remplacer ?
project-replace-message = Le remplacer écrasera son contenu.
dialog-replace = Remplacer
project-new-error-title = Impossible de créer un nouveau projet
project-new-error-message = Il s'agit d'une erreur d'{ app-name } : veuillez la signaler.
project-open-error-title = Impossible d'ouvrir « { $file } »
project-open-error-not-found = Ce fichier n'existe pas ou n'est pas accessible pour le moment.
project-open-error-permission = { app-name } n'a pas la permission de lire ce fichier.
project-open-error-read = Le fichier n'a pas pu être lu.
project-open-error-too-large = Le fichier est plus volumineux que ce qu'{ app-name } peut ouvrir.
project-open-error-not-project = Ce fichier n'est pas un projet { app-name }.
project-open-error-newer = Ce projet a été enregistré avec une version plus récente d'{ app-name }. Installez la dernière version pour l'ouvrir.
project-open-error-damaged = Le fichier est endommagé : une partie ne peut pas être lue comme un projet { app-name }.
project-save-error-title = Impossible d'enregistrer « { $file } »
project-save-error-permission = { app-name } n'a pas la permission d'écrire ce fichier ou son dossier.
project-save-error-full = Il n'y a pas assez d'espace sur le disque.
project-save-error-write = Le fichier n'a pas pu être écrit.
project-save-error-internal = Le projet n'a pas pu être préparé pour l'enregistrement. Il s'agit d'une erreur d'{ app-name } : veuillez la signaler.
project-save-error-kept = Le fichier qui s'y trouvait déjà, le cas échéant, est resté tel quel.

## The shortcut editor in the Preferences' Keyboard section (F2.12c1, ADR-0048 AM7; glossary:
## shortcut, command, reassign, add, remove, search, restore_defaults, key_escape). $command and
## $owner are commands' names as the menus write them ("Save as…" keeps its ellipsis); $shortcut is a
## shortcut written as the menus write it ("Ctrl+Shift+S", "⇧⌘S"); $search is what the user typed in
## the search field. prefs-shortcut-capture is shown inside the box that waits for the keys;
## prefs-shortcut-fixed under it when a key that cannot be assigned is pressed. The two questions
## come when the shortcut, or a default being restored, belongs to another command; their button
## is dialog-reassign, with dialog-cancel.

prefs-shortcuts = Raccourcis
prefs-shortcuts-search = Rechercher des commandes
prefs-shortcuts-commands = Commandes
prefs-shortcuts-no-match = Aucune commande ne correspond à « { $search } ».
prefs-shortcuts-of = Raccourcis de « { $command } »
prefs-shortcuts-none = Aucun raccourci
prefs-shortcut-add = Ajouter…
prefs-shortcut-remove = Enlever
prefs-shortcuts-restore-all = Réinitialiser tous les raccourcis
prefs-shortcut-capture = Appuyez sur le raccourci, ou sur Échap pour annuler
prefs-shortcut-capture-name = Nouveau raccourci pour « { $command } »
prefs-shortcut-fixed = { $shortcut } ne peut pas être affecté : il sert à se déplacer dans le programme, aux menus ou au système.
prefs-shortcut-conflict-title = { $shortcut } est déjà affecté à « { $owner } »
prefs-shortcut-conflict-message = L'affecter plutôt à « { $command } » ? « { $owner } » ne l'aura plus.
prefs-shortcut-restore-conflict-title = Certains raccourcis par défaut de « { $command } » sont affectés à d'autres commandes
prefs-shortcut-restore-conflict-message = Les rendre à « { $command } » ? Les autres commandes ne les auront plus.
dialog-reassign = Réaffecter

## Importing and exporting the shortcuts (F2.12c2, ADR-0051; glossary: import, export, shortcut,
## replace). $file is a file's whole name ("Mine.orchestra-shortcuts"); $count a number of entries.
## file-shortcuts-name is the name proposed when exporting, without the extension (the program adds
## ".orchestra-shortcuts"). The question comes before an import replaces shortcuts the user had
## changed (buttons: dialog-replace, dialog-cancel). The report comes after an import when some
## entries of the file could not be used. The errors' other messages are those of the project's
## (project-open-error-*, project-save-error-*).

prefs-shortcuts-import = Importer…
prefs-shortcuts-export = Exporter…
file-filter-shortcuts = Raccourcis { app-name }
file-import-shortcuts-title = Importer les raccourcis
file-export-shortcuts-title = Exporter les raccourcis
file-shortcuts-name = Raccourcis
prefs-shortcuts-import-question-title = Remplacer vos raccourcis par ceux de « { $file } » ?
prefs-shortcuts-import-question-message = Les raccourcis que vous avez modifiés seront perdus.
prefs-shortcuts-imported-title = Raccourcis importés
prefs-shortcuts-imported-unused = Entrées de « { $file } » non utilisées : { $count }
prefs-shortcuts-import-error-title = Impossible d'importer « { $file } »
prefs-shortcuts-import-error-not-shortcuts = Ce fichier n'est pas un fichier de raccourcis { app-name }.
prefs-shortcuts-import-error-newer = Ce fichier a été enregistré avec une version plus récente d'{ app-name }. Installez la dernière version pour l'importer.
prefs-shortcuts-import-error-damaged = Le fichier est endommagé : il ne peut pas être lu comme des raccourcis { app-name }.
prefs-shortcuts-export-error-title = Impossible d'exporter « { $file } »

## The theme editor in the Preferences' Appearance section (F2.12b1, ADR-0037 T7–T8; glossary:
## theme_color_names, accent_color, duplicate, delete, color, readability). The colour names
## are those of the theme's tokens, as the editor lists them; the user picks a value for each.
## $name is a theme's name; $color and $background are colour names (theme-color-*). The editing
## notes name the Mode control and its values (prefs-mode, prefs-mode-light, prefs-mode-dark):
## keep them identical.
## prefs-theme-readability-issue tells, in plain words and without numbers, that a colour is hard
## to read on a colour behind it, and what to do. $color is the colour that is hard to see,
## $background the colour behind it where it is worst (both names of rows of the editor, so the
## user finds them), $count how many OTHER colours behind it are a problem too (0 or more).
## $kind is "text" (letters) or "graphic" (a line or a shape: borders, the focus ring, a switch
## that is on). $severity is "invisible" (almost invisible), "hard" (hard to read or to make out)
## or "weak" (readable, but not well enough for people who do not see well; text only).
## $direction is "lighter" when $color is the lighter of the two (so the advice is to make it
## lighter still, or the colours behind it darker) and "darker" otherwise. Address the user as
## the rest of the catalogue does.

prefs-theme-duplicate = Dupliquer
prefs-theme-delete = Supprimer
prefs-theme-name = Nom
prefs-theme-copy-name = { $name } (copie)
prefs-theme-colors = Couleurs
prefs-theme-builtin = Les thèmes intégrés ne peuvent pas être modifiés : dupliquez-en un pour créer le vôtre.
prefs-theme-editing-light = Voici les couleurs de la moitié claire. Pour modifier la moitié sombre, choisissez Sombre dans Mode.
prefs-theme-editing-dark = Voici les couleurs de la moitié sombre. Pour modifier la moitié claire, choisissez Clair dans Mode.
prefs-theme-restore = Restaurer
prefs-theme-value-name = Valeur de « { $color } »
prefs-theme-readability = Lisibilité
prefs-theme-readability-ok = Tous les textes et contrôles se lisent bien.
prefs-theme-readability-issue =
    { $kind ->
        [graphic]
            { $severity ->
                [invisible] La couleur « { $color } » est presque invisible sur « { $background } »
               *[hard] La couleur « { $color } » se distingue mal sur « { $background } »
            }
       *[text]
            { $severity ->
                [invisible] La couleur « { $color } » est presque invisible sur « { $background } »
                [hard] La couleur « { $color } » se lit difficilement sur « { $background } »
               *[weak] La couleur « { $color } » se lit sur « { $background } », mais pas assez bien pour les personnes qui voient mal
            }
    }{ $count ->
        [0] .
        [one] {" "}et sur une autre couleur.
       *[other] {" "}et sur { $count } autres couleurs.
    } { $direction ->
        [lighter]
            { $count ->
                [0] Éclaircissez-la, ou assombrissez « { $background } ».
               *[other] Éclaircissez-la, ou assombrissez les couleurs derrière elle.
            }
       *[darker]
            { $count ->
                [0] Assombrissez-la, ou éclaircissez « { $background } ».
               *[other] Assombrissez-la, ou éclaircissez les couleurs derrière elle.
            }
    }
prefs-theme-delete-title = Supprimer le thème « { $name } » ?
prefs-theme-delete-message = Ses couleurs seront perdues.
dialog-delete = Supprimer
theme-color-surface = Arrière-plan des fenêtres
theme-color-surface-raised = Arrière-plan des panneaux
theme-color-surface-sunken = Arrière-plan des champs
theme-color-surface-overlay = Arrière-plan des menus et dialogues
theme-color-border = Bordures et séparateurs
theme-color-control-border = Bordures des contrôles
theme-color-text = Texte standard
theme-color-text-muted = Texte secondaire
theme-color-text-disabled = Texte désactivé
theme-color-accent = Couleur d'accentuation
theme-color-accent-hover = Couleur d'accentuation sous le pointeur
theme-color-accent-pressed = Couleur d'accentuation enfoncée
theme-color-on-accent = Texte sur la couleur d'accentuation
theme-color-accent-text = Texte en couleur d'accentuation
theme-color-control = Arrière-plan des boutons
theme-color-control-hover = Arrière-plan des boutons sous le pointeur
theme-color-control-pressed = Arrière-plan des boutons enfoncés
theme-color-selection = Arrière-plan de sélection
theme-color-on-selection = Texte sélectionné
theme-color-focus = Décoration du focus
theme-color-error = Erreurs
theme-color-warning = Avertissements
theme-color-success = Succès
theme-color-shadow = Ombre
