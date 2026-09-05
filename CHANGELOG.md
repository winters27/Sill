# Changelog

What changed, said the way somebody using Sill would notice it. Not a list of
commits: half of those are merges, and most of the rest name a mechanism.

`.github/workflows/release.yml` reads the section for the version being tagged
and uses it as the release body, and it refuses to build if the section is not
there. So the order is: `npm run version:set 0.2.0`, write the section below,
commit, then tag.

To write one, start from the diff rather than from memory:

```bash
npm run changelog -- research
```

That prints where the work went, what modules appeared and went away, the new
IPC surface, the dependencies added, and every commit body in full. What goes
in a section here is the subset of that which somebody would notice, said
differently.

## Unreleased

The first build with an installer. Everything before this was cloned and
compiled.

**Dates add up.** `today + 3 weeks`, `days until 2026-12-25` and one date
minus another are answered in the list the way a sum is. A date typed on its
own is still a search, and a date that used to reach the calculator as a
subtraction, `2024-08-28` answering 1988, no longer does.

**Past sums come back.** Every answer you press Enter on is kept, fifty at
most, and `sums` lists them with Enter copying the answer again. Nothing is
read until the word is typed.

**A colour typed in one form is offered in the others.** `#ff8800`, `rgb()`
and `hsl()` each answer with the two forms they are not, drawn as a swatch,
and Enter copies. `Pick a Colour` reads one pixel off the screen through the
capture overlay and copies it as hex.

**Ask what time it is somewhere.** `tokyo time` answers with the clock there
and how far ahead it is, from the time zone table Windows already keeps. A
World clock widget shows the cities you choose, and its ticking is the
machine's own clock, so a pinned one costs nothing while the launcher is
hidden.

**One row asks everything to close.** `Quit All Applications` sends every
program with a window the close its own button sends, after saying how many
that is and asking. Anything with unsaved work puts up its own question, and
nothing is terminated. The desktop and Sill are left alone.

**Confetti.** Because it is Friday.

**A quicklink can open a program's own address once you say so.** A link to
`notion://` or another app scheme shows a switch in its editor; turned on, that
one link opens that one scheme. Web, mail and settings addresses never needed
it, the schemes that run code can never have it, and a file you import arrives
with every switch off.

**A clipboard entry can be named, and its text corrected.** Name This Entry
puts a name on a row in place of its first line, and Edit Text fixes the typo
in the thing you copied three times. Both can be taken back with Undo, and an
edit that would turn one entry into another is refused rather than merged.

**The model can be shown a corner of the screen instead of all of it.** A
request to read the screen can name a region, or ask you to drag one out on
the capture overlay, and the approval card says which it is about to read.

**Fonts, with a line set in each.** `font mono` lists the installed faces
whose names hold those words, each row drawn in itself, and Enter copies the
name. The list is read the first time it is asked for and held ten minutes.

**A display's resolution and refresh rate, from the list.** `resolution`
lists the modes the display can be in, `display 2 resolution` the second
display's, and Enter sets one. It asks whether to keep it and goes back by
itself after fifteen seconds if nobody answers, and Undo puts it back later.

**Draw your own window positions and put a key on each.** Settings, under
Shortcuts, takes layouts as fractions of the display: left 0, top 0, width
0.5, height 1 is the left half. Every layout is in the action panel on any
window, and a key sends the window in front to it.

**Tags.** Snippets and quicklinks take tags, a plain word finds a tagged
row, and `tag:work` keeps only those rows. On the clipboard `tag:work` is the
collection of that name.

**Search knows what changed instead of looking again.** A file created,
renamed or deleted patches the index in place, so a folder somebody is working
in stays current without the walk that used to follow every change. The index
and the ranker now agree on what counts as a match, which was the cause of
results that were found but sorted as though they were not.

**Tab finishes a path.** Typing part of a path and pressing Tab completes it as
far as every match agrees, the way a shell does, with a trailing separator on a
folder so the next Tab reads inside it.

**Three words narrow a file search.** `ext:pdf`, `size:>1mb` and `date:week`
filter what Sill's own index answers with, and they combine, so
`notes ext:md date:month` is the notes touched this month. One on its own is a
question too: `ext:pdf` lists the PDFs. Typing anything else is not slower for
them existing.

**Recently opened files are findable by name.** Windows keeps a note of
everything that has been opened and Sill reads it when a query asks rather than
watching it, so a document in a folder nobody added as a search root is still
one word away.

**The file under the cursor shows what is inside it.** A picture, or the first
screenful of text, once the selection has stopped moving. A path in a subtitle
says where a file is and not what it is, and two files with the same name in
two folders were two rows nobody could tell apart. Folders, programs, archives
and anything whose bytes are still in somebody's cloud are left alone: a
preview never downloads a file and never opens one it cannot show.

**Every action can be given a key, and the keys are listed.** Actions carry
their own shortcut rather than the launcher knowing about four of them, and
Settings > Shortcuts shows every action with the key that runs it, which
Backspace resets and Delete clears. Nothing that destroys anything ships with a
key by default.

**Renaming and moving a file are ordinary actions now**, so a key can be bound
to either, they appear in the activity log, and they can be undone from it.

**Text utilities and unit conversions answer in the row the calculator uses**,
and pressing Enter on an answer copies it.

**Windows, processes and the recycle bin are reachable.** The process list can
end a program, sleep and shut down are rows, and anything that cannot be taken
back asks first.

**Screen readers are told which row is highlighted.** The clipboard, the
extension store, and an extension's own list and grid were silent about it;
so were the three menus. Ten Windows tooltips are gone, replaced with the
window's own, which appear on focus as well as hover. Windows high contrast
gets a focus ring, a selected row and switches back, all of which it used to
delete along with every shadow.

**A hidden launcher does close to nothing.** Renderers suspend twenty seconds
after the window goes away, caches and timers stop when the feature they belong
to is off, dictation stops waking the machine at rest, and the clipboard
history and a conversation both stop growing without bound. Icons are extracted
once rather than once per search, and Node is looked for once rather than on
every extension activation.

**Failures say so.** Something that goes wrong now appears in the window and
keeps appearing until it stops, rather than only reaching `sill.log`. A panic
writes to that log, which in a packaged build is the only place anything can be
said at all.

**Timers are held by Windows, so Sill runs nothing while one is pending.**
Typing `remind me in 20 minutes to call Sam`, or `timer 5m tea`, offers a row
saying when it will arrive; pressing Enter writes a one-off scheduled task and
Windows does the waiting. Nothing in Sill ticks, the reminder survives Sill
being restarted, and the task deletes itself once it has fired. With no timer
set there is no task, so a machine with none of them is a machine where this
feature does not exist.

**Notes, as a prototype that is switched off.** One note at a time, in one
window, found by typing `note` and whatever you remember of it. There are no
folders, no tags and no formatting, which is why it is off in Settings under
General rather than on: turned off, Sill never opens the notes file at all. A
note that cannot be read costs that note rather than the file, and a file that
cannot be read at all is kept beside itself and said out loud rather than
quietly replaced.

**A string somebody else wrote is not an address Sill will open.** Text arriving
from a file, a clipboard entry or an extension is no longer treated as a link
just because it looks like one.

**Cloud files keep their icons without being downloaded.** A OneDrive
placeholder used to be fetched twice over just to draw a row. Paths longer than
260 characters are indexed rather than silently dropped, and a network folder
that is not there no longer holds up indexing while SMB gives up.
