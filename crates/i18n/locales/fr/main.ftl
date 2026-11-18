# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — French. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Fichier
menu-file-new = Nouveau
menu-file-open = Ouvrir
menu-file-save = Enregistrer
menu-file-save-as = Enregistrer sous
menu-file-quit = Quitter
menu-edit = Éditer
menu-edit-undo = Annuler
menu-edit-redo = Rétablir
menu-edit-cut = Couper
menu-edit-copy = Copier
menu-edit-paste = Coller
menu-edit-select-all = Tout sélectionner
menu-edit-preferences = Préférences
menu-view = Affichage
menu-view-score = Partition
menu-view-tracks = Pistes
menu-view-mixer = Table de mixage
menu-view-full-screen = Plein écran
menu-help-contents = Aide

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
