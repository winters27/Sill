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

Type `confetti` when the occasion calls for it.

## Typing

Whatever you type is matched against everything at once. Matching is fuzzy
and forgiving in the ways that matter: word boundaries and runs of letters
count for more than scattered ones, hyphens and dots are transparent so
`wifi` finds `Wi-Fi`, and a typo does not throw the result away.

Results are ordered by what you actually open. Something you pick often, or
picked recently, climbs; something you never pick sinks. You can also give
anything a name of your own, and an exact match on a name you chose beats
every kind of guess.

`content:` looks inside files rather than at their names, so
`notes content:invoice` narrows the notes to the ones that mention it and
each row shows the line it was found on. It is deliberately bounded: two
hundred files, half a megabyte of each, and a third of a second, whichever
runs out first. Sill indexes names, not contents, and this is the question
people actually ask rather than a second index of everything on the disk.

A few words open lists of their own. `font mono` lists the installed fonts
whose names hold those words, each drawn in itself, and Enter copies the
name. `resolution` lists the modes your display can be in. `tag:work` keeps
only the snippets and quicklinks you tagged that way, on their own or with
more words to narrow them.

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

A picture also offers to become another kind of picture: Convert to PNG or to
JPEG, written beside the original under a free name so nothing is replaced.
Sill reads whatever Windows reads, which includes WebP, and HEIC once the
free HEIF and HEVC extensions are installed. It writes PNG and JPEG only,
because Windows ships no WebP encoder.

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

An entry can be given a name, which stands in for its first line in the list
and is searched like the text is, and its text can be corrected in place for
the typo in the thing you copied three times. Both come back with Undo. An
edit that would turn one entry into the text of another is refused rather
than quietly merged.

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

A quicklink can also be a file, a folder, or a program's own address such as
a `notion://` link into a page. Sill opens web, mail and settings addresses on
its own; for any other scheme the link's editor shows a switch, and turning it
on lets that one link open that one scheme. A file you import arrives with
every such switch off, and the schemes that run code can never be switched on.

## Sums

Type a sum and the answer appears. It handles units and conversions, and it
is deliberately hard to trigger: a version number is not a sum, and something
that looks like arithmetic but is not gets left alone rather than replacing
your search with a number.

Dates add up too. `today + 3 weeks` says which day that is, `days until
2026-12-25` counts them, and one date minus another is the days between. A
date on its own is left alone, because typed alone it is more often the start
of a file name than a question.

Every answer you press Enter on is remembered, fifty at most. Type `sums` to
see them, or `sums usd` for the ones that mention it, and Enter copies the
answer again.

A colour typed one way is offered in the others: `#ff8800` gets its `rgb()`
and `hsl()` forms, each a row with a swatch that Enter copies. `Pick a
Colour` goes the other way: click any pixel on screen and its hex lands on
the clipboard.

`tokyo time`, or `time in tokyo`, says what the clock reads there and how
far ahead or behind that is. Cities are the ones Windows lists for each time
zone, so `eastern time` works too. The World clock widget shows the cities
you choose in Settings, ticking on this machine's own clock.

## Reminders and timers

Type `remind me in 20 minutes to call Sam`, or `timer 5m tea`, and a row says
what will happen and at what time. Pressing Enter hands it to Windows: it
becomes a one-off task in the Task Scheduler folder called `Sill`, and Windows
does the waiting. So the reminder survives Sill being restarted, and while it
is pending Sill is doing nothing at all about it. When it fires, Sill opens
with the reminder on screen and the action panel on it, so it can be copied or
read aloud like any other piece of text. The task removes itself afterwards.

A length of time can be written `20`, `20m`, `20 minutes` or `1h30m`. A bare
number is minutes. Anything shorter than ten seconds or longer than thirty days
is refused rather than rounded, and a reminder with no time in it is not
offered at all, because a reminder at a moment nobody chose is worse than none.

## Notes

Off to begin with. Settings, General, Notes turns it on, and the row says what
it is: a prototype, one note at a time, in its own window. There are no
folders, no tags and no formatting.

With it on, typing `note` lists what you have written, newest first, and
anything after the word narrows it by looking inside the notes themselves. The
last row makes a new one. A note is saved while you type and again when the
window closes, and it is called whatever its first line says.

Only the word `note` (or `notes`, `scratch`, `scratchpad`) reaches them.
Nothing you write is searched by an ordinary query, so a launcher opened in
front of somebody else does not put a paragraph of yours under an application.

Switched off, Sill does not open the notes file at all.

## Windows

The switcher key opens Sill straight into a list of open windows with a
picture of each. Typing narrows it.

Windows can also be arranged from the launcher: halves, thirds, quarters,
maximise, centre. A workspace saves a set of programs and where their windows
go, and opening one starts anything that is not running.

`Quit All Applications` asks every program with a window to close, the way
its own close button would, after saying how many that is and asking you
first. Anything with unsaved work puts up its own question, and nothing is
ended by force. The desktop and Sill itself are left alone.

Positions of your own live in Settings, under Shortcuts, as fractions of the
display: left 0, top 0, width 0.5, height 1 is the left half. Every layout is
in the action panel on any window, and a key on one sends the window in front
there.

`resolution` lists the modes the display can be in, and `display 2
resolution` the second display's. Enter sets one, then asks whether to keep
it; with no answer in fifteen seconds it goes back by itself, and Undo puts
it back later.

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

`Read a QR Code` drags a box over a code on screen and copies what it says.
Any picture in your clipboard history offers the same thing from its action
panel. What a code holds is copied and named, never opened: a code on a page
was put there by whoever made the page, so following it stays a separate,
deliberate keystroke.

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

Reading the screen can be all of it or a part: the model can name a region,
or ask you to drag one out on the capture overlay, and the card says which it
is about to read.

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

## MCP servers

An MCP server you already have can put its tools in Sill's action panel, so a
file found by search offers something the server does rather than only what
Sill does. [mcp.md](mcp.md) is the whole of how, including what one costs and
the four things one cannot do.

Two things worth knowing before you set one up. **Nothing is started until you
run one of its actions**, so a server listed in Settings and never used has
never been started. And **an action from a server counts as running a program**,
because that is what it is, so a scheduled trigger cannot use one and Sill's
own AI has to prove somebody is at the machine first.

## The keys

**Press `?` with an empty search field.** That opens the keyboard reference,
and it is the real one: Sill builds it from the keys that are actually
registered, including your own changes and including any chord two things are
fighting over, which it marks rather than hides.

Every global key goes through Sill's own low-level keyboard hook first, so
Sill sees it before any other program's registration does and takes it, Menu
key and media keys included. Windows' registration is kept behind the hook as
a backstop, because Windows can silently remove a slow hook and a backstop is
what brings it back on the next summon. A key another program has already
registered is therefore still yours; Settings says when the backstop was
refused, and why that is not a dead key.

There is no list of keys on this page on purpose. A reference somebody types
into a document is wrong the first time a key changes, and the person reading
it has no way to tell. A key is changed on the panel of the thing it does:
the summon key and the window switcher under General, the screenshot keys
under Screenshots, the dictation trigger under Dictation. Settings, under
Shortcuts, holds the keyboard map, shortcuts of your own, the movement keys
and the action keys, and it has presets for arrow keys alone, for vim-style
movement, and for emacs-style movement. A preset only ever adds: turning one
on never takes the arrow keys away.

The top of that panel is a keyboard with every bound key lit. Choose a
modifier above it, or hold one down over it, and it shows that layer; hover a
lit key to read what it does, and click one to go to the row that set it. When
you record a key, the recorder shows the modifiers you are holding, and before
it saves it asks Sill what already runs on that combination: something in the
same section is refused with the reason, something elsewhere is saved and
mentioned. A key Windows would not register is marked on the keyboard, in the
reference, and on the row, and the tray menu shows no key at all rather than
one that does nothing.

## Settings

Settings opens from the launcher and covers, in its own words: startup,
privacy and the settings file, the window's size and material, snippets,
quicklinks, clipboard history, emoji, every key, screenshots, which programs
it scans, browser pages and the web, file search, extensions, scripts, the AI
chat, dictation, text to speech, widgets, the index and what it found,
diagnostics and where Sill keeps its data, and what version this is.

It opens at the size Raycast opens its settings at, about two thirds of a
2560 by 1440 display, because a settings window is read slowly and wants air.
Rows sit on the window itself, full width, one idea each, with a hairline
between them and a plain heading over each group.

## When something is not working

Sill reports trouble rather than logging it where nobody looks. A key it
could not register, a startup entry that did not take, a file it could not
save: each becomes a message you can see, and the tray icon says how many
there are. A problem that fixes itself withdraws its own message.

File search needs [Everything](https://www.voidtools.com/) to search the
whole disk. Without it Sill offers to install it and everything else still
works.
