# Using Sill

Sill is a launcher. One key opens a search field over whatever you were
doing, you type, and it finds programs, files, settings pages, snippets,
saved links, things you have copied, open windows, emoji and the machine's
own switches. Press Enter and it does the obvious thing. Press Escape and it
goes away again.

This page is about using it. Building it is in the
[README](../README.md), and writing an extension for it is in
[extensions.md](extensions.md).

## Opening it

The summon key is **Alt+Space** unless you have changed it, and Settings,
under Shortcuts, is where you change it.

If pressing it does nothing, something else on the machine already owns that
combination. Windows does not say which program, and it refuses the
registration silently, so Sill checks at startup and opens Settings at the
row that needs a different key. A launcher you cannot summon is not a small
problem, which is why it interrupts.

There is a second key for the window switcher, **Ctrl+Alt+W** by default,
because the whole value of a switcher is that one press puts the window you
were last in under the cursor. Screenshot keys are in the same place and are
unset by default: there is no obviously free combination for them, and a
default that collides with something you already use is worse than none.

## Typing

Whatever you type is matched against everything at once. Matching is fuzzy
and forgiving in the ways that matter: word boundaries and runs of letters
count for more than scattered ones, hyphens and dots are transparent so
`wifi` finds `Wi-Fi`, and a typo does not throw the result away.

Results are ordered by what you actually open. Something you pick often, or
picked recently, climbs; something you never pick sinks. You can also give
anything a name of your own, and an exact match on a name you chose beats
every kind of guess.

Typing nothing shows the root list, which is where the things that have no
search term live.

## Doing something with what you found

Enter runs the thing under the cursor. What that means depends on what it is,
and everything Sill knows about is a kind of thing with its own set of
actions: a file can be renamed, moved, put in the recycle bin, or have its
path copied; a window can be sent to a half, a third, a quarter or the
centre; a program can be started, or shown in the folder it lives in.

There is more than one action for most things, so there is a panel that
shows the rest of them along with the key each one answers to. It also
appears in the pill at the bottom right of the window, which always names the
key that opens it.

Actions that can be taken back offer to be. After one that can, Ctrl+Z undoes
it. Most cannot, and the key does nothing rather than claiming to have undone
something.

## Everything you have copied

Sill keeps a history of the clipboard: text, images and files, with the
program each came from, searchable by content. You can pin something so it
stays, exclude a program so nothing it copies is ever recorded, and set how
long the rest is kept.

Two things it will not record. Windows lets a program mark what it puts on
the clipboard as confidential, which password managers do, and Sill honours
that. It also spots the shapes that secrets come in, so a key copied out of a
terminal by a tool that did not think to mark it is left out too.

## Text you type over and over

A snippet is saved text with a keyword. Type the keyword anywhere on the
machine and the text replaces it. Snippets can hold the clipboard, the date,
a fresh identifier, and a mark for where the cursor should end up, so a
snippet can put you in the middle of what it just wrote.

They can also be limited to one program, and grouped, and pasted from the
launcher rather than expanded, if you would rather not have a keyword.

## Addresses you use with a search term

A quicklink is a saved address with a hole in it. Type the quicklink's name,
then what goes in the hole, and Sill opens the address in the browser you
named for it. Only what you typed is escaped on the way in, never the address
around it, so a link with its own query string still works.

Web search is the same idea with the search engines already set up.

## Sums

Type a sum and the answer appears. It handles units and conversions, and it
is deliberately hard to trigger: a version number is not a sum, and something
that looks like arithmetic but is not gets left alone rather than replacing
your search with a number.

## Windows

The switcher key opens Sill straight into a list of open windows with a
picture of each. Typing narrows it.

Windows can also be arranged from the launcher: halves, thirds, quarters,
maximise, centre. A workspace saves a set of programs and where their windows
go, and opening one starts anything that is not running.

## The machine's own switches

Volume, one program's volume on its own, which speakers or headphones sound
comes out of, Wi-Fi, Bluetooth, dark mode, emptying the recycle bin, sleep,
hibernate, sign out, restart and shut down, and the settings pages behind all
of them. These are rows in the list you press Enter on, not pages Sill opens
for you to click through.

## Pictures of the screen, and text out of them

Sill takes a screenshot of an area, a window, one display or everything, and
opens it in a small editor. The editor keeps what you draw as a list of
shapes rather than painting them into the picture, so undo stays instant
however much you have drawn.

It also reads text off the screen using the one Windows already has, so
nothing is downloaded and nothing is sent anywhere.

## Speaking, and being read to

Dictation turns speech into text locally. The model is downloaded the first
time you use it, loaded when you start speaking, and the process shuts itself
down after sitting idle, so it costs nothing when you are not dictating.

Text to speech reads text out loud: something you copied, or a text result
in the list.

## Asking a model

With something typed in the launcher, **Tab** asks it as a question instead
of searching for it. The answer appears in place. Escape comes straight back
to the search with your words still there, so nothing is lost if searching
was what you meant.

For a longer conversation there is a chat window with a sidebar,
attachments and formatted answers.

Several providers are supported, including a local one, and your keys are
sealed by Windows rather than left readable in a settings file. Model lists
come from each provider rather than being typed in.

The model can read: the index, files, folders, the clipboard, your windows,
the selection, the screen, and how the machine is set. Anything that writes a
file, starts a program, types for you or changes the machine stops at a card
and waits for you to say yes.

## Extensions

Sill runs extensions written for the Raycast API, installed from within Sill
itself. Coverage is partial, and
[extensions.md](extensions.md) lists exactly which parts of that API work.

An extension is granted nothing when it is installed except the right to draw
in the launcher's own window. Reading files, changing them, reaching the
network and starting programs are each asked for separately, shown on the
install screen, and revocable in Settings under Extensions. Taking one away
reaches a command that is running right now, not only the next time it
starts.

Be clear-eyed about what that is: it is a permission boundary, not a cage. An
extension that sets out to get around it can. What you are protected from is
an ordinary extension quietly doing something you did not agree to.

## The keys

**Press `?` with an empty search field.** That opens the keyboard reference,
and it is the real one: Sill builds it from the keys that are actually
registered, including your own changes and including any chord two things are
fighting over, which it marks rather than hides.

There is no list of keys on this page on purpose. A reference somebody types
into a document is wrong the first time a key changes, and the person reading
it has no way to tell. Settings, under Shortcuts, is where every key is
changed, and it has presets for arrow keys alone, for vim-style movement, and
for emacs-style movement. A preset only ever adds: turning one on never takes
the arrow keys away.

## Settings

Settings opens from the launcher and covers, in its own words: startup and
the tray icon, the window's size and material, snippets, quicklinks,
clipboard history, emoji, every key, screenshots, where results come from,
file search, extensions, scripts, the AI chat, dictation, text to speech,
widgets, the index and where Sill keeps its data, and what version this is.

## When something is not working

Sill reports trouble rather than logging it where nobody looks. A key it
could not register, a startup entry that did not take, a file it could not
save: each becomes a message you can see, and the tray icon says how many
there are. A problem that fixes itself withdraws its own message.

File search needs [Everything](https://www.voidtools.com/) to search the
whole disk. Without it Sill offers to install it and everything else still
works.
