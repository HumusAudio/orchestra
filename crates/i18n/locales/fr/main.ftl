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
