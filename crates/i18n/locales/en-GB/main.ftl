# SPDX-License-Identifier: GPL-3.0-or-later
#
# Orchestra interface messages — en-GB. A separate catalog from en-US because British musical
# terminology differs (bar/measure, stave/staff, crotchet/quarter note): see glossary.toml.

## Application

app-name = Orchestra

## Main menu

menu-file = File
    .mnemonic = f
menu-file-new = New
    .mnemonic = n
menu-file-open = Open…
    .mnemonic = o
menu-file-save = Save
    .mnemonic = s
menu-file-save-as = Save as…
    .mnemonic = a
menu-file-quit = Quit
    .mnemonic = q
menu-edit = Edit
    .mnemonic = e
menu-edit-undo = Undo
    .mnemonic = u
menu-edit-redo = Redo
    .mnemonic = r
menu-edit-cut = Cut
    .mnemonic = t
menu-edit-copy = Copy
    .mnemonic = c
menu-edit-paste = Paste
    .mnemonic = p
menu-edit-select-all = Select all
    .mnemonic = a
menu-edit-preferences = Preferences…
    .mnemonic = n
menu-view = View
    .mnemonic = v
menu-view-score = Score
menu-view-tracks = Tracks
menu-view-mixer = Mixer
menu-view-full-screen = Full screen
    .mnemonic = f
menu-help = Help
    .mnemonic = h
menu-help-contents = User guide
    .mnemonic = u
menu-help-about = About { app-name }
    .mnemonic = a

## macOS's own menus (F2.11e2, ADR-0048 AM9; glossary: mac_settings … mac_bring_all_to_front).
## Shown only in the system's menu bar on macOS: the application menu (Settings, Services, Hide,
## Hide Others, Show All, Quit) and the Window menu (Minimize, Zoom, Bring All to Front). The
## names are Apple's own in each language, so they match every other Mac program; { app-name }
## is the program's name, placed where the language wants it. No mnemonics: macOS has none.

menu-mac-settings = Settings…
menu-mac-services = Services
menu-mac-hide = Hide { app-name }
menu-mac-hide-others = Hide Others
menu-mac-show-all = Show All
menu-mac-quit = Quit { app-name }
menu-mac-window = Window
menu-mac-minimize = Minimise
menu-mac-zoom = Zoom
menu-mac-bring-all-to-front = Bring All to Front

## Our title bar's window buttons (F2.11b2, ADR-0049 TB10; glossary: window_close …
## window_minimize). Drawn only where the system draws no frame (GNOME on Wayland), so the names
## are GTK's own; they are what assistive technologies read and the buttons' tooltips.
## "restore" is the Maximise button's name while the window is maximised.

window-close = Close
window-maximize = Maximise
window-restore = Restore
window-minimize = Minimise

## Keys' names in shortcuts, as menus write them next to an item, joined by "+" (glossary:
## key_ctrl … key_numpad; ADR-0048 AM12, F2.11d2a). $number is a function key's number (1–24),
## $digit a digit of the numeric keypad (0–9).

key-ctrl = Ctrl
key-shift = Shift
key-alt = Alt
key-super = Super
key-windows = Win
key-enter = Enter
key-escape = Esc
key-tab = Tab
key-space = Space
key-backspace = Backspace
key-delete = Del
key-insert = Ins
key-home = Home
key-end = End
key-page-up = PgUp
key-page-down = PgDn
key-left = Left
key-right = Right
key-up = Up
key-down = Down
key-menu = Menu
key-function = F{ $number }
key-numpad = Num { $digit }

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

## Dialog buttons (glossary: restore_defaults, ok, cancel, dialog_close)

dialog-restore-defaults = Restore defaults
dialog-ok = OK
dialog-cancel = Cancel
dialog-close = Close

## The About dialog (F2.12a3b, ADR-0050 DG10; glossary: about, version, license, source_code,
## bundled_fonts, all_rights_reserved, logo). Its title is menu-help-about. $version is the
## program's version ("0.0.0"), $url the source repository's address; the fonts' and the
## licences' names are proper names and stay in English.

about-version = Version { $version }
about-license = Free software under the GNU General Public Licence, version 3 or later.
about-logo-notice = The logo is © 2026 Emanuele Rainoldi, all rights reserved: it is not covered by that licence.
about-source = Source code: { $url }
about-fonts = Bundled fonts: Inter, Bravura and JetBrains Mono, under the SIL Open Font License 1.1.
about-logo = { app-name } logo

## The Preferences (F2.12a4a, ADR-0050 DG8–DG9; glossary: preferences, preferences_section,
## appearance, theme, theme_mode, system_mode, light, dark, blinking_cursor, theme_pairs). The
## title has no ellipsis (the menu item has one). The pairs' names are the ten built-in themes
## of ADR-0037 T1, translated where they are common words (owner's choice 8 of F2.12a4a).

prefs-title = Preferences
prefs-sections = Sections
prefs-section-appearance = Appearance
prefs-theme = Theme
prefs-mode = Mode
prefs-mode-system = System
prefs-mode-light = Light
prefs-mode-dark = Dark
prefs-caret-blink = Blinking cursor
theme-absolute = Absolute
theme-graphite = Graphite
theme-signal = Signal
theme-parchment = Parchment
theme-slate = Slate
theme-studio = Studio
theme-forest = Forest
theme-lagoon = Lagoon
theme-nocturne = Nocturne
theme-snow = Snow

## The Preferences' Language and Keyboard sections (F2.12a4b1; glossary: language, keyboard,
## system_language, fine_adjustment, language_names). The languages' names are each written in
## its own language, the same in every catalogue (owner's choice 1 of F2.12a4b1). Until F2.13 a
## new language applies at the next start, which the note beside the choice says.

prefs-section-language = Language
prefs-section-keyboard = Keyboard
prefs-language = Language
prefs-language-system = System language
prefs-language-restart = Applies the next time { app-name } starts.
prefs-fine-modifier = Fine adjustment
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

prefs-section-playback = Playback
prefs-playback-intro = How accents and dynamic marks are played. These values are personal: they apply to every project.
prefs-playback-articulations = Articulation accents
prefs-playback-dynamic-accents = Dynamic accents
prefs-playback-changes = Changes of level
prefs-playback-louder = Louder by
prefs-playback-back = Back in
prefs-playback-held = Held for
prefs-playback-change = Change in
prefs-playback-ms = { $value } ms
prefs-playback-step = { $value }
articulation-soft-accent = Soft accent
articulation-stress = Stress
articulation-accent = Accent
articulation-marcato = Marcato
articulation-unstress = Unstress
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

file-untitled = Untitled
file-filter-project = { app-name } project
file-open-title = Open project
file-save-title = Save project as
file-chooser-unavailable-title = Files can't be chosen
file-chooser-unavailable-message = { app-name } uses the desktop's own window for choosing files, and no service of the desktop provides it. It needs the xdg-desktop-portal package together with the one for the desktop in use (for example xdg-desktop-portal-kde or xdg-desktop-portal-gnome).

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
project-save-changes-title = Save the changes to “{ $name }”?
project-save-changes-message = Your changes will be lost if you don't save them.
dialog-save = Save
dialog-dont-save = Don't save
project-replace-title = “{ $file }” already exists. Do you want to replace it?
project-replace-message = Replacing it will overwrite its contents.
dialog-replace = Replace
project-new-error-title = A new project can't be created
project-new-error-message = This is a mistake in { app-name }: please report it.
project-open-error-title = “{ $file }” can't be opened
project-open-error-not-found = The file doesn't exist or can't be reached at the moment.
project-open-error-permission = { app-name } isn't allowed to read this file.
project-open-error-read = The file could not be read.
project-open-error-too-large = The file is larger than { app-name } can open.
project-open-error-not-project = The file isn't an { app-name } project.
project-open-error-newer = The project was saved with a newer version of { app-name }. Install the latest version to open it.
project-open-error-damaged = The file is damaged: part of it can't be read as an { app-name } project.
project-save-error-title = “{ $file }” can't be saved
project-save-error-permission = { app-name } isn't allowed to write this file or its folder.
project-save-error-full = There isn't enough space on the disk.
project-save-error-write = The file could not be written.
project-save-error-internal = The project could not be prepared for saving. This is a mistake in { app-name }: please report it.
project-save-error-kept = The file that was already there, if any, has been left as it was.

## The shortcut editor in the Preferences' Keyboard section (F2.12c1, ADR-0048 AM7; glossary:
## shortcut, command, reassign, add, remove, search, restore_defaults, key_escape). $command and
## $owner are commands' names as the menus write them ("Save as…" keeps its ellipsis); $shortcut is a
## shortcut written as the menus write it ("Ctrl+Shift+S", "⇧⌘S"); $search is what the user typed in
## the search field. prefs-shortcut-capture is shown inside the box that waits for the keys;
## prefs-shortcut-fixed under it when a key that cannot be assigned is pressed. The two questions
## come when the shortcut, or a default being restored, belongs to another command; their button
## is dialog-reassign, with dialog-cancel.

prefs-shortcuts = Shortcuts
prefs-shortcuts-search = Search commands
prefs-shortcuts-commands = Commands
prefs-shortcuts-no-match = No command matches “{ $search }”.
prefs-shortcuts-of = Shortcuts of “{ $command }”
prefs-shortcuts-none = No shortcut
prefs-shortcut-add = Add…
prefs-shortcut-remove = Remove
prefs-shortcuts-restore-all = Restore all shortcuts
prefs-shortcut-capture = Press the shortcut, or Esc to cancel
prefs-shortcut-capture-name = New shortcut for “{ $command }”
prefs-shortcut-fixed = { $shortcut } can't be assigned: moving around the program, the menus or the system use it.
prefs-shortcut-conflict-title = { $shortcut } is already assigned to “{ $owner }”
prefs-shortcut-conflict-message = Assign it to “{ $command }” instead? “{ $owner }” will no longer have it.
prefs-shortcut-restore-conflict-title = Some default shortcuts of “{ $command }” are assigned to other commands
prefs-shortcut-restore-conflict-message = Give them back to “{ $command }”? The other commands will no longer have them.
dialog-reassign = Reassign

## Importing and exporting the shortcuts (F2.12c2, ADR-0051; glossary: import, export, shortcut,
## replace). $file is a file's whole name ("Mine.orchestra-shortcuts"); $count a number of entries.
## file-shortcuts-name is the name proposed when exporting, without the extension (the program adds
## ".orchestra-shortcuts"). The question comes before an import replaces shortcuts the user had
## changed (buttons: dialog-replace, dialog-cancel). The report comes after an import when some
## entries of the file could not be used. The errors' other messages are those of the project's
## (project-open-error-*, project-save-error-*).

prefs-shortcuts-import = Import…
prefs-shortcuts-export = Export…
file-filter-shortcuts = { app-name } shortcuts
file-import-shortcuts-title = Import shortcuts
file-export-shortcuts-title = Export shortcuts
file-shortcuts-name = Shortcuts
prefs-shortcuts-import-question-title = Replace your shortcuts with those of “{ $file }”?
prefs-shortcuts-import-question-message = The shortcuts you changed will be lost.
prefs-shortcuts-imported-title = Shortcuts imported
prefs-shortcuts-imported-unused = Entries of “{ $file }” not used: { $count }
prefs-shortcuts-import-error-title = “{ $file }” can't be imported
prefs-shortcuts-import-error-not-shortcuts = The file isn't an { app-name } shortcut file.
prefs-shortcuts-import-error-newer = The file was saved with a newer version of { app-name }. Install the latest version to import it.
prefs-shortcuts-import-error-damaged = The file is damaged: it can't be read as { app-name } shortcuts.
prefs-shortcuts-export-error-title = “{ $file }” can't be exported

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

prefs-theme-duplicate = Duplicate
prefs-theme-delete = Delete
prefs-theme-name = Name
prefs-theme-copy-name = { $name } (copy)
prefs-theme-colors = Colours
prefs-theme-builtin = Built-in themes can't be changed: duplicate one to make a theme of your own.
prefs-theme-editing-light = These are the colours of the light half. To change the dark half, choose Dark in Mode.
prefs-theme-editing-dark = These are the colours of the dark half. To change the light half, choose Light in Mode.
prefs-theme-restore = Restore
prefs-theme-value-name = Value of “{ $color }”
prefs-theme-readability = Readability
prefs-theme-readability-ok = Every text and control is easy to read.
prefs-theme-readability-problem =
    { $kind ->
        [graphic]
            { $severity ->
                [invisible] The colour “{ $color }” is almost invisible on “{ $background }”.
               *[hard] The colour “{ $color }” is hard to make out on “{ $background }”.
            }
       *[text]
            { $severity ->
                [invisible] The colour “{ $color }” is almost invisible on “{ $background }”.
                [hard] The colour “{ $color }” is hard to read on “{ $background }”.
               *[weak] The colour “{ $color }” can be read on “{ $background }”, but not well enough for people who don't see well.
            }
    }
prefs-theme-readability-same =
    { $count ->
        [one] The colour “{ $culprit }” has the same problem with one other colour.
       *[other] The colour “{ $culprit }” has the same problem with { $count } other colours.
    }
prefs-theme-readability-advice =
    { $advice ->
        [either]
            { $direction ->
                [lighter]
                    { $partner-direction ->
                        [lighter] Make the colour “{ $culprit }” lighter, or make the colour “{ $partner }” lighter.
                       *[darker] Make the colour “{ $culprit }” lighter, or make the colour “{ $partner }” darker.
                    }
               *[darker]
                    { $partner-direction ->
                        [lighter] Make the colour “{ $culprit }” darker, or make the colour “{ $partner }” lighter.
                       *[darker] Make the colour “{ $culprit }” darker, or make the colour “{ $partner }” darker.
                    }
            }
        [restore] No lighter or darker colour in place of “{ $culprit }” is enough: press “{ prefs-theme-restore }” on its row.
       *[change]
            { $direction ->
                [lighter] Make the colour “{ $culprit }” lighter.
               *[darker] Make the colour “{ $culprit }” darker.
            }
    }
## The text, lines and density rows (F2.12b2a; glossary: text_size, font_roles, equal_width_digits,
## theme_lines, density). Plain words, not designers' terms: "Rounded corners" rather than
## "corner radius". prefs-theme-pixels writes a value in pixels and prefs-theme-percent a
## percentage, $value a whole number; follow the language's habit for the space before "%" (a
## no-break space where there is one). The family names other than Inter and JetBrains Mono (the
## fonts' own names, never translated) describe Inter with digits of the same width, for numbers
## that change in place.

prefs-theme-text = Text
prefs-theme-text-size = Text size
prefs-theme-family-ui = Interface font
prefs-theme-family-numeric = Font for numbers
prefs-theme-family-code = Font for file paths and details
prefs-theme-family-inter = Inter
prefs-theme-family-inter-tabular = Inter, equal-width digits
prefs-theme-family-mono = JetBrains Mono
prefs-theme-lines = Lines, corners and shadows
prefs-theme-shape-corners = Rounded corners
prefs-theme-shape-lines = Line thickness
prefs-theme-shape-focus = Focus decoration thickness
prefs-theme-shape-shadow-blur = Shadow blur
prefs-theme-shape-shadow-distance = Shadow distance
prefs-theme-shape-dialog-frame = Dialogue frame thickness
prefs-theme-density = Density
prefs-theme-density-comfortable = Comfortable
prefs-theme-density-compact = Compact
prefs-theme-pixels = { $value } px
prefs-theme-percent = { $value }%
prefs-theme-delete-title = Delete the theme “{ $name }”?
prefs-theme-delete-message = Its colours will be lost.
dialog-delete = Delete
theme-color-surface = Window background
theme-color-surface-raised = Panel background
theme-color-surface-sunken = Field background
theme-color-surface-overlay = Menu and dialogue background
theme-color-border = Borders and separators
theme-color-control-border = Control borders
theme-color-text = Normal text
theme-color-text-muted = Secondary text
theme-color-text-disabled = Disabled text
theme-color-accent = Accent colour
theme-color-accent-hover = Accent colour under the pointer
theme-color-accent-pressed = Accent colour when pressed
theme-color-on-accent = Text on the accent colour
theme-color-accent-text = Text in the accent colour
theme-color-control = Button background
theme-color-control-hover = Button background under the pointer
theme-color-control-pressed = Button background when pressed
theme-color-selection = Selection background
theme-color-on-selection = Selected text
theme-color-focus = Focus decoration
theme-color-error = Errors
theme-color-warning = Warnings
theme-color-success = Success
theme-color-shadow = Shadow
