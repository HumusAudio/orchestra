// SPDX-License-Identifier: GPL-3.0-or-later
// Chapter 1 — Introduction (en-US). Italian counterpart: docs/guide/it/chapters/01-introduction.typ

#import "/docs/guide/template.typ": music, note, screenshot-placeholder, smufl, tip

= Introduction

Orchestra is a single program for _writing_, _producing_ and _setting music to picture_. It
combines a music notation editor, a digital audio workstation (DAW) and a simple video editor —
and, unlike a collection of separate tools, it keeps *one* project: the score you write and the
tracks you produce are the same music.

== One project, several views

When you write a #music(smufl.text-quarter-note) quarter note on a staff, the very same note appears in
the DAW view, where you can play it, move it slightly to give it a human feel, or change how hard
it is played. Nothing is copied and nothing has to be synchronized: the notation view and the DAW
view are two ways of looking at the same notes.

- The *notation view* shows the music as it is written: staves, measures, articulations,
  dynamics, repeats.
- The *DAW view* shows the music as it is played: repeats are unfolded, so a passage played twice
  appears twice on the timeline — and each occurrence stays linked to the measure it comes from.
- *Video* is just another kind of track on the same timeline, so music and picture can never drift
  apart.

#screenshot-placeholder(
  [Screenshot: notation view and DAW view of the same project],
  [The same notes seen in the notation view (top) and in the DAW view (bottom).],
)

#note("Note")[
  Small timing changes made in the DAW view — the kind that make a performance sound human —
  do not alter the score. Only a move larger than a configurable tolerance changes where the note
  is written. You decide how strict that tolerance is.
]

== Light and dark appearance

Orchestra will have a light and a dark appearance. By default it follows the setting of your
operating system and changes with it; you can also choose light or dark yourself. The score can
follow the dark appearance or always stay on white paper, as you prefer.

== Adjusting values

Every slider and knob in Orchestra works the same way, wherever it is:

- *Drag* it, or click where you want the value.
- *Turn the mouse wheel* with the pointer over it: no need to click it first. Each notch moves
  the value by a small amount; on a touchpad, swipe up or down.
- *Hold Ctrl* (⌘ on macOS) while dragging, turning the wheel or pressing the arrow keys for a
  finer adjustment: a drag moves ten times more slowly, a notch or an arrow by the smallest step.
- *Double-click* it to go back to its default value.
- With the keyboard, once it is selected: the arrow keys, Page Up and Page Down, Home and End.

#tip("Tip")[
  Hold Shift while turning the wheel to scroll the panel instead of changing the control under
  the pointer.
]

== Keyboard shortcuts

Orchestra's commands have keyboard shortcuts that follow the conventions of your system: on
Linux and Windows they use Ctrl, on macOS ⌘. These already work:

- *F11* (⌃⌘F on macOS) switches full screen on and off.
- *Ctrl+Q* (⌘Q on macOS) quits Orchestra; on Windows close the window with Alt+F4, as usual.
- In a text box, *Ctrl+Z* undoes your typing, *Ctrl+Shift+Z* (and Ctrl+Y on Windows) redoes it,
  and *Ctrl+X*, *Ctrl+C*, *Ctrl+V* and *Ctrl+A* cut, copy, paste and select all. While you are
  typing in a box, Undo only undoes what you typed there, never a change to your project.

The shortcuts follow the letters printed on your keys, whatever your keyboard layout. With a
layout that does not write Latin letters, such as Russian or Greek, they use the key in the place
of the US letter, so Ctrl+Z is still the key where Z is on a US keyboard. The number keys work by
position on every layout, so on a French keyboard you do not need Shift to use them.

#note("Note")[
  You will be able to change every shortcut in the Preferences. A few keys stay as they are on
  every system: Tab, the arrow keys, Enter, Escape and Space move around the window, and F10
  will open the menus.
]

== Status of this guide

Orchestra is at a very early stage of development. This preview edition explains the ideas
behind the program; each chapter will grow, with screenshots, as the corresponding features
become available.

#tip("Tip")[
  This guide exists in two languages, English (GUIDE.pdf) and Italian (GUIDA.pdf), with exactly
  the same chapters and illustrations. The interface itself is available in American and British
  English, Italian, French, German and Spanish.
]
