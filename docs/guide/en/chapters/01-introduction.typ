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

== Status of this guide

Orchestra is at a very early stage of development. This preview edition explains the ideas
behind the program; each chapter will grow, with screenshots, as the corresponding features
become available.

#tip("Tip")[
  This guide exists in two languages, English (GUIDE.pdf) and Italian (GUIDA.pdf), with exactly
  the same chapters and illustrations. The interface itself is available in American and British
  English, Italian, French, German and Spanish.
]
