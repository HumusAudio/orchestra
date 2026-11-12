# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — French. Terminology: crates/i18n/glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = Fichier
menu-edit = Éditer
menu-edit-undo = Annuler
menu-edit-redo = Rétablir
menu-view = Affichage
menu-view-score = Partition
menu-view-tracks = Pistes
menu-view-mixer = Table de mixage

## Status bar (glossary: measure → "mesure", feminine). French CLDR plural "one" covers 0 and 1,
## so "0 mesure sélectionnée" is produced, which is correct French.

status-measures-selected =
    { $count ->
        [one] { $count } mesure sélectionnée
       *[other] { $count } mesures sélectionnées
    }

## Playback options (glossary: playback, metronome, count_in, volume)

playback-title = Lecture
playback-metronome = Métronome
playback-count-in = Décompte
playback-metronome-volume = Volume du métronome

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Réinitialiser aux valeurs par défaut
dialog-ok = OK
