# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — en-GB. A separate catalog from en-US because British musical
# terminology differs (bar/measure, stave/staff, crotchet/quarter note): see glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = File
menu-edit = Edit
menu-edit-undo = Undo
menu-edit-redo = Redo
menu-view = View
menu-view-score = Score
menu-view-tracks = Tracks
menu-view-mixer = Mixer

## Status bar (glossary: measure → "bar" in en-GB)

status-measures-selected =
    { $count ->
        [one] { $count } bar selected
       *[other] { $count } bars selected
    }

## Playback options (glossary: playback, metronome, count_in, volume, track_name, unnamed)

playback-title = Playback
playback-metronome = Metronome
playback-count-in = Count-in
playback-metronome-volume = Metronome volume
playback-track-name = Track name
playback-track-name-placeholder = Unnamed

## Dialog buttons (glossary: restore_defaults, ok)

dialog-restore-defaults = Restore defaults
dialog-ok = OK
