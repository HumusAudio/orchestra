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

=== Your own themes

The ten built-in themes cannot be changed, but you can make as many themes of your own as you like,
starting from any of them.

- *Duplicate* makes a copy of the theme you are using, called for example "Studio (copy)", and
  chooses it at once. Your themes appear in the *Theme* list after the built-in ones.
- *Name* renames the theme: type the new name and press Enter, or move to another control. If
  another theme of yours already has that name, Orchestra adds a number to it.
- *Delete* removes the theme after asking you, and goes back to the built-in theme it came from.
- *Export…* saves the theme you are using in a file of its own (`.orchestra-theme`), to carry it
  to another computer or give it to someone. *Import…* reads such a file and adds its theme to
  yours, without replacing any: if the name is taken, a number is added. The imported theme is
  chosen at once. If the file holds something this version of Orchestra cannot use, a short
  message says how many entries were left out.

Under *Text* you choose how large all the interface's text is, from 85% to 150%, and the font
of the interface, of numbers and of file paths. Under *Lines, corners and shadows* you choose how
rounded the corners are, how thick the lines, the focus decoration and the dialogs' frame are, and
how soft and how far the shadows fall. *Density* can be *Comfortable*, with room around the
controls, or *Compact*, with the controls closer together. Each value has its *Restore*, and the window changes as
you move the control. These choices are the same in the light and in the dark half; each slider
stops where the interface would become hard to use. Even with the largest text nothing is pushed
out of the window: the settings rearrange themselves to fit, and a name too long for its place
goes on to a second line.

#screenshot-placeholder(
  [Screenshot: the text, lines and density of a theme of your own (theme-text-lines.png)],
  [Text at 110% and rounder corners: the whole window already uses them.],
)

Under *Colors* each colour of the interface has a row: its name, a sample of the colour, its value
and *Restore*. Write the value as on the web, `#` followed by six hexadecimal digits (`#5b9bf0`),
or eight digits to add transparency (`#00000080` is black at half strength). As soon as the value
is a colour, the whole window takes it: the Preferences themselves show you the result. *Restore*
gives the colour back the value of the theme it came from.

You do not need to know colour codes: click a colour's sample and a colour picker opens under its
row. In the large square, move right for a more intense colour and up for a brighter one; the strip
under it chooses the hue, from red through green and blue back to red; for a transparent colour, such
as the shadow, a third strip sets the transparency. The two samples at the bottom show the colour
before and now. The window changes as you move. Press Enter, or click the sample again, to keep the
colour; Escape gives back the colour you started from. The code in the box follows the picker, and
the picker follows a code you type.

#screenshot-placeholder(
  [Screenshot: the colour picker open under the accent colour (theme-picker.png)],
  [The colour picker: the square, the hue strip, and the colour before and now.],
)

#screenshot-placeholder(
  [Screenshot: a theme of your own with the accent colour changed (theme-editor.png)],
  [A copy of Studio with a blue accent: the whole window already uses it.],
)

#note("Note")[
  Every theme has a light half and a dark half. The rows show the colours of the half the window is
  using, as the line above them says: to change the other half, choose it in *Mode*, and the rows
  follow.
]

Under the name, *Readability* tells you whether everything can still be read. If a colour is too
close to the one behind it, a few plain sentences say how hard it is to see ("almost invisible",
"hard to read", or readable "but not well enough for people who don't see well"), on which colour,
and what to do. The advice is about the colour you changed, and Orchestra checks that it really
works: it tells you to make a colour lighter only if a lighter one can fix the problem, and the
same for darker. When a single pair of colours is involved, it also tells you how to change the
other one. If no lighter or darker colour can help, it tells you to press *Restore*. The rows of
the colours involved are marked with a dot, so you find them at once. It is only advice: the
colour stays yours. When everything is fine, a short line says so.

#screenshot-placeholder(
  [Screenshot: a readability warning at the top of the section (theme-contrast.png)],
  [Secondary text made too dark: almost invisible on four backgrounds; the advice is to make it lighter.],
)

#tip("Tip")[
  Your themes are kept with your preferences, so they are there the next time you open Orchestra.
]

== Language

Choose the language of the interface: American or British English, Italian, French, German or
Spanish. Each language is written in that language, so you find yours even if the interface is in
a language you do not know. The new language applies *at once*: the menus, the window and the
Preferences change before your eyes, and your open work stays as it was, with everything you can
undo.

*System language* uses your system's language and tells you which in brackets: for example
*System language (English (UK))*. If your system lists several languages in order of preference,
Orchestra takes the first one it knows. A regional variant is fine: Swiss German gives German,
Mexican Spanish gives Spanish. For English the country matters, because the musical words differ:
English from Great Britain, Australia, New Zealand, Ireland, South Africa and India gives British
English (_crotchet_, _bar_), every other one American English (_quarter note_, _measure_). If your
system uses none of the six languages, Orchestra speaks American English.

#tip("Tip")[
  On Windows and macOS, if you changed your system's language while Orchestra was open, choose
  *System language* again: Orchestra reads it again at that moment. On Linux a new system language
  reaches programs only when you log in again.
]

#screenshot-placeholder(
  [Screenshot: the Language section (preferences-language.png)],
  [The Language section, with the system's language named in brackets.],
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

=== Shortcuts

Under it is the list of every command with its keyboard shortcuts, in the order of the menus. You
can change them as you like: the menus show the new shortcuts at once, and Orchestra remembers
them the next time it starts.

#screenshot-placeholder(
  [Screenshot: the Keyboard section (preferences-keyboard.png)],
  [The Keyboard section: the key of fine adjustment and the commands with their shortcuts.],
)

- *Search commands* narrows the list as you type. It finds a command by its name, or by its
  shortcut: type "ctrl+s" to see what Ctrl+S does.
- Select a command to see its shortcuts under the list. A command can have more than one, or
  none.
- *Add…* waits for the new shortcut: hold the modifier keys you want (Ctrl, Shift, Alt…) and press
  the key. Esc cancels, and so does Tab, which moves on as always.
- *Remove* takes away the selected shortcut of the command.
- *Restore defaults* gives the command back the shortcuts it had at the start; *Restore all
  shortcuts* does it for every command.

Some keys cannot become shortcuts, because Orchestra and your system need them. On their own or
with Shift: Tab and the arrows to move between controls, Enter, Space and Esc, F10; and always: Alt
and the Menu key, which open the menus, Alt with a
letter for the menus' underlined letters (on Linux and Windows), and on macOS the system's own,
such as ⌘H. If you press one of them, Orchestra tells you why next to the box, and keeps waiting
for another key.

If the shortcut you press already belongs to another command, Orchestra asks before moving it:

#screenshot-placeholder(
  [Screenshot: the question about a shortcut already in use (shortcut-conflict.png)],
  [Ctrl+N belongs to New: Reassign gives it to Open instead.],
)

*Reassign* (or Enter) gives it to the new command, and the other one no longer has it; *Cancel*
(or Esc) leaves everything as it was. *Restore defaults* asks the same way when other commands
have taken some of the command's original shortcuts in the meantime, and lists them.

=== Taking your shortcuts with you

*Export…* saves all your shortcuts in a file ending in `.orchestra-shortcuts`: every command with
its shortcuts, also those you never changed. Copy it to another computer, share it with a
colleague or a class, or keep it before trying something new. The file works on Linux, Windows
and macOS alike: Ctrl on one becomes ⌘ on a Mac.

*Import…* reads such a file and puts its shortcuts in place of yours. If you had changed some
shortcuts, Orchestra asks first, because yours would be lost; *Replace* goes ahead, *Cancel* (or
Esc) leaves them as they are. Commands the file does not mention get their usual shortcuts.

#note("Note")[
  A file made with another version of Orchestra may name commands this one does not have, or keys
  your system keeps for itself. Orchestra keeps what it can use and tells you how many entries it
  could not use. A file made by a newer version of Orchestra cannot be imported: install the
  latest version first.
]
