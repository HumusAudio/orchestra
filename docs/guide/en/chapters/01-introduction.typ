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
  opens the menus.
]

== The menu bar

On Linux and Windows the menus are at the top of the window: File, Edit, View and Help. On macOS
they are in the menu bar at the top of the screen, as in every Mac program. There the first menu
is named Orchestra and has what every Mac program has: *About Orchestra*, *Settings…* (⌘,, the Preferences), the
system's *Services*, *Hide Orchestra* (⌘H), *Hide Others* (⌥⌘H), *Show All* and *Quit Orchestra*
(⌘Q). A *Window* menu, before Help, has *Minimize* (⌘M), *Zoom* and *Bring All to Front*, and
macOS adds the list of Orchestra's windows to it. Because ⌘H, ⌥⌘H and ⌘M belong to these system
commands, Orchestra never gives them to one of its own commands on a Mac.

On GNOME the system draws no title bar around a program's window, so Orchestra draws its own,
with the menus in it to leave more room for the score. Drag its empty part to move the window,
double-click it to maximise or restore the window, and right-click it for the system's window
menu. The buttons on the right close, maximise and minimise the window: which of them appear,
and on which side, follows the choice you made in GNOME's settings (only Close, by default). Drag
the window's edges or corners to resize it. On every other desktop the window keeps the title bar
of the system, with the menus under it.

#screenshot-placeholder(
  [Screenshot: Orchestra's own title bar on GNOME (title-bar.png)],
  [The title bar Orchestra draws on GNOME: the menus, the program's name, the window buttons.],
  height: 1.5cm,
)

- *With the mouse*, click a menu's name to open it, and click a command. You can also keep the
  button pressed, move onto a command and release it there. While a menu is open, moving the
  pointer over another menu's name opens that one; clicking the open menu's name again closes it.
- *With the keyboard*, press *F10*, or press and release *Alt* on its own: the first menu's name is
  highlighted. Move with the left and right arrow keys, open the menu with the down arrow or Enter,
  and choose a command with the up and down arrows and Enter. In an open menu the left and right
  arrows go to the next menu. Escape goes back one step; press it again to leave the menus.
- *Faster*: hold *Alt* and the underlined letter of a menu's name to open it (Alt+F opens File),
  then press a command's underlined letter to run it. The letters are underlined while you hold
  Alt or use the menus from the keyboard. They are chosen for each language, so in Italian
  Alt+M opens _Modifica_.

#tip("Tip")[
  Commands that are not available yet, or cannot do anything right now, are shown greyed out.
  Their shortcuts are still written next to them, so the menus are also a list of the shortcuts.
]

#screenshot-placeholder(
  [Screenshot: the File menu opened from the menu bar],
  [The File menu opened from the keyboard, with the underlined letters.],
)

== Context menus

Click with the right mouse button on a text box to open its *context menu*: Undo and Redo, Cut,
Copy and Paste, Select all. You can also open it from the keyboard, with the Menu key or with
Shift+F10: it then opens at the text cursor, with its first available command already highlighted.

- Each command shows its keyboard shortcut on the right, written as your system writes it
  (Ctrl+C on Linux and Windows, ⌘C on macOS), so the menu also teaches you the shortcuts.
- Commands that cannot do anything right now are shown greyed out: Copy, for example, while no text
  is selected.
- Move with the arrow keys, Home and End; press Enter or Space to run the highlighted command;
  press Escape to close the menu.
- With the mouse you can press the right button, move onto a command and release the button there:
  the command runs at once.
- A command with an arrow on its right opens a submenu: rest the pointer on it for a moment, click
  it, or press the right arrow key; the left arrow key closes the submenu again.

#note("Note")[
  The menu's commands act on the text box you were working in: the text box keeps the keyboard
  while the menu is open, so Copy copies exactly the text you had selected.
]

#screenshot-placeholder(
  [Screenshot: the context menu of a text box],
  [The context menu of a text box, with the shortcut of each command on the right.],
)

== Dialogs

When Orchestra needs to ask you something, or to tell you that something went wrong, it opens a
*dialog* in the middle of the window, with a thick frame in the accent colour. While it is open,
the rest of the window waits: the menus and the shortcuts do not respond, and a click outside the
dialog does nothing. When it closes, the keyboard goes back where it was.

- The buttons are at the bottom right, in your system's order: on Windows and KDE Plasma the
  button that confirms comes first (*Save, Don't save, Cancel*), on macOS, GNOME and the other
  Linux desktops last (*Don't save, Cancel, Save*).
- The button highlighted in the accent colour is the one that answers *Enter*. *Escape* presses
  the button that cancels or closes. A command that throws something away, such as *Don't save*,
  is never the one for Enter.
- *Tab* and *Shift+Tab* move between the buttons without leaving the dialog; *Space* presses the
  one that has the focus.

The first dialog you can open is *Help → About Orchestra* (on macOS the first item of the
Orchestra menu). It shows the logo, the program's version, the licence (Orchestra is free
software, under the GNU GPL version 3 or later; the logo is not part of it), the address where
the source code is, and the fonts bundled with the program with their licence. Press *Close*,
Enter or Escape to go back to work.

#screenshot-placeholder(
  [Screenshot: the About Orchestra dialog (about-dialog.png)],
  [The About Orchestra dialog: the logo, the version, the licences, the source code.],
)

=== Choosing a file

*File → Open…* (Ctrl+O, ⌘O on a Mac) and *File → Save as…* (Ctrl+Shift+S, ⇧⌘S) open your
system's own window for choosing a file, the one every other program on your computer uses: the
standard one on Windows and macOS, your desktop's on Linux (KDE's, GNOME's…). It shows only
Orchestra projects (files ending in `.orchestra`); when you save a project for the first time it
proposes the name *Untitled*.

While that window is open, Orchestra waits for it: nothing in Orchestra's own window reacts, and
it cannot be closed until you choose a file or cancel.

On Linux the window for choosing files comes from a service of the desktop, the *XDG desktop
portal*. If your system has none, Orchestra tells you so instead of doing nothing: install the
package `xdg-desktop-portal` together with the one for your desktop (for example
`xdg-desktop-portal-kde` on KDE Plasma or `xdg-desktop-portal-gnome` on GNOME), then try again.

=== New, open and save a project

Everything you write in Orchestra lives in one *project*: a single file ending in `.orchestra`.
The window's title shows the project's name, followed by Orchestra's (*Song — Orchestra*); a
project you have never saved is called *Untitled*. While the project has changes you have not
saved yet, the title starts with a dot: *• Song — Orchestra*.

- *File → New* (Ctrl+N, ⌘N) starts an empty project: one empty bar in 4/4, at ♩ = 120, in C
  major. A window for choosing these values will come later.
- *File → Open…* (Ctrl+O, ⌘O) opens a project you choose. It takes the place of the one you had
  open.
- *File → Save* (Ctrl+S, ⌘S) writes the project over its own file. The first time, it asks where
  to save, as *Save as* does.
- *File → Save as…* (Ctrl+Shift+S, ⇧⌘S) saves the project in a new file, which from then on is
  the project's file.

If you type a name without `.orchestra`, Orchestra adds it. If a file with that name is already
there, Orchestra asks before replacing it. Enter and Escape both answer *Cancel*, which keeps the
file: to replace it you have to choose *Replace* yourself, so it never happens by mistake.

Saving never damages the file you had: Orchestra writes the new version next to it and puts it in
place only when it is complete, so if anything goes wrong (a full disk, a removed USB stick) the
old file is still there as it was.

Before New, before Open and before closing the window, Orchestra checks for unsaved changes. If
there are any, it asks whether to save them first:

#screenshot-placeholder(
  [Screenshot: the question about unsaved changes (save-changes-dialog.png)],
  [Before closing a project with unsaved changes Orchestra asks whether to save them.],
)

- *Save* saves the project (asking where, if it was never saved), then goes on.
- *Don't save* throws the changes away and goes on.
- *Cancel* (or Escape) goes back to the project, and nothing happens.

If a project cannot be opened or saved, Orchestra tells you why: the file does not exist any
more, you are not allowed to read or write it, the disk is full, the file is not an Orchestra
project, it was saved by a newer version of Orchestra, or it is damaged. Under the message you
find the file's full path.

#tip("Note")[
  In this preview there is no view of the score yet, so nothing changes the project and the title
  never shows the dot. New, Open and Save already work with real project files.
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
