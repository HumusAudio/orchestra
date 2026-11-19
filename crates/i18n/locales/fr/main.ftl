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

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Réinitialiser aux valeurs par défaut
dialog-ok = OK
