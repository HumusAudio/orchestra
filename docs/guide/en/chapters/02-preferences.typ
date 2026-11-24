// SPDX-License-Identifier: GPL-3.0-or-later
// Chapter 2 — Preferences (English). Italian counterpart: docs/guide/it/chapters/02-preferences.typ

#import "/docs/guide/template.typ": note, screenshot-placeholder, tip

= Preferences

The Preferences hold the choices that apply to Orchestra itself, not to one project: the window's
appearance, the language, the way of playing, the keyboard. Open them with *Edit → Preferences…* or with Ctrl+, (on
macOS with *Orchestra → Settings…* or ⌘,).

== How they work

On the left is the list of sections, on the right the settings of the chosen section. Move between
the sections with the mouse or with the up and down arrow keys; Tab goes to the settings.

- Every change applies *at once*: you see the result while you choose, with nothing to confirm.
  There are no OK or Cancel buttons.
- Orchestra remembers your choices and finds them again the next time you open it.
- Close the window with *Close*, with Escape or with Ctrl+, again (⌘, on macOS).

#screenshot-placeholder(
  [Screenshot: the Preferences open on the Appearance section (preferences-dialog.png)],
  [The Preferences: the list of sections on the left, the Appearance section on the right.],
)

== Appearance

- *Theme* chooses one of Orchestra's ten colour pairs (Studio, Slate, Forest, Snow and the others),
  each with a light and a dark half.
- *Mode* chooses which half to use: *System* follows your system's light or dark setting and changes
  with it, *Light* and *Dark* fix it.
- *Blinking cursor* turns off the blinking of the cursor in text boxes: the cursor stays still.

== Language

Choose the language of the interface: American or British English, Italian, French, German or
Spanish. Each language is written in that language, so you find yours even if the interface is in
a language you do not know. *System language* uses your system's.

#note("Note")[
  For now a new language applies the next time Orchestra starts, as the note under the choice says.
  In a future version it will change at once, and *System language* will recognise your system's
  language (today it uses American English).
]

#screenshot-placeholder(
  [Screenshot: the Language section (preferences-language.png)],
  [The Language section, with the note about restarting.],
)

== Playback

Here you choose how Orchestra plays accents and dynamic marks: how much louder an accent is and how
soon the sound goes back to the level before it, how long the hold and the change of *fp*, *sfp* and
*pf* last. The starting values were tuned by ear. Every value has a slider, with the value written
beside it; *Restore defaults* brings them all back to the starting values.

#note("Note")[
  These values are yours, not the project's: they apply to every project you open, and someone who
  opens the same project with their own preferences may hear it played a little differently.
]

#screenshot-placeholder(
  [Screenshot: the Playback section (preferences-playback.png)],
  [The Playback section: the accents, the dynamic accents and the changes of level.],
)

== Keyboard

*Fine adjustment* chooses the key that, held down, makes sliders and dividers more precise: the
wheel and the arrow keys move one step at a time and dragging goes ten times slower. It is Ctrl by
default (⌘ on macOS); you can choose Shift or Alt (⌥ on macOS) if that suits you better.

#tip("Tip")[
  If you choose Shift, Shift with the wheel over a slider adjusts it finely instead of scrolling the
  window: to scroll, use the wheel outside the slider.
]

#screenshot-placeholder(
  [Screenshot: the Keyboard section (preferences-keyboard.png)],
  [The Keyboard section: the key of fine adjustment.],
)

The list of keyboard shortcuts, to change as you like, will come here too.
