# Where Sill is

What the audit of 2026-08-29 laid out, and what has actually been built since.
Kept here rather than in a planning tool because it is about this code and
should move with it.

A line is done when something checks it. Where that is a test, it is named.

## P0, foundation and correctness

| | Item | State |
| --- | --- | --- |
| P0.1 | Version control, MIT headers, CI running verify | **Repo exists** with full history. CI has no remote to run on yet |
| P0.2 | Idle pass: cap results, debounce, bound the icon cache, drop PATH executables, checkpoint the log | **Done.** Result cap is 120, icon cache evicts, PATH executables default off, the clipboard log is bounded and checkpointed |
| P0.3 | Instrument before promising | **Done.** Ranking, idle memory, idle processor, summon latency and cold start are all measured and held to a budget. Release build: **25 ms to summon, 846 ms to start** |
| P0.4 | Extension correctness | **Done.** Every method the host can call is answered, storage persists, crashes reach the window |
| P0.5 | Release-mode host bundling | **Done.** Three candidates, in order, and only one that exists is returned |
| P0.6 | Credentials at rest | **Done.** Keys go to DPAPI rather than into a settings file |
| P0.7 | Split `lib.rs` | **Done.** Commands live in `commands/`, state in `state.rs` |
| P0.8 | Typed object and action model | **Done.** `object.rs`, `action.rs`, a registry with capabilities and undo |
| P0.9 | Performance budgets | **Done.** `docs/budgets.md`, `tests/budgets.rs`, and idle checks in the device suite |

## P1, best in class core

| | Item | State |
| --- | --- | --- |
| P1.1 | Ranking: hysteresis, typo tolerance, aliases | **Done.** Match classes, a gap limit, initials read as initials, explicit and learned aliases |
| P1.2 | Context captured at summon | **Done.** `ActionCtx` |
| P1.3 | Window enumeration and control | **Done.** `windowing.rs` |
| P1.4 | Window management commands | **Done.** Halves, thirds, quarters, monitors |
| P1.5 | Window switcher | **Done.** Fuzzy search, recent-window order, and a picture of the window under the cursor |
| P1.6 | Selection actions | **Done.** Case, tidy, title |
| P1.7 | Clipboard: merge, HTML, secrets, collections | **Done** |
| P1.8 | Navigation bindings | **Done.** Vim and Emacs presets |
| P1.9 | Command history | **Done.** Up recalls what was searched |
| P1.10 | Application and command hotkeys | **Done** |
| P1.11 | Extension install path | **Not started.** Blocked on a decision, see below |
| P1.12 | Emoji and symbols | **Done.** In the picker and in ordinary searches |
| P1.13 | Snippets: collections, rich text, app-specific, forms | **Done, except forms.** Placeholders, import and export, collections, formatting, and limiting one to the programs it belongs in. Multi-field forms need a surface that does not exist yet |
| P1.14 | Native dictation panel | **Not started, and the reason for it was wrong.** See below |
| P1.15 | Disabled means stopped | **Done** |

## P2, power user

| | Item | State |
| --- | --- | --- |
| P2.1 | Activity history and undo | Not started. `Outcome.undo` exists and window moves use it |
| P2.2 | Screenshot | **Done.** Drag an area or take every screen, with a markup editor: box, arrow, ellipse, pen, highlight, hide, text |
| P2.3 | OCR on demand | **Done.** WinRT recognition, ported from AuraKey. Reads a picture on the clipboard, never automatically. Measured: 35 ms on a 640x160 capture |
| P2.4 | Read aloud | Not started |
| P2.5 | System control | **Done.** Volume, mute, dark mode, lock, audio output switching, Wi-Fi and Bluetooth, all of them switches you press in the list without leaving it, plus a program's own volume behind its own row. Do not disturb and night light have no public way to set them, see below |
| P2.6 | Process and resource view | Not started |
| P2.7 | Terminal execution, capability gated | Not started |
| P2.8 | Scripting | Not started |
| P2.9 | File actions | **Done.** Copy path, copy name, reveal, open a terminal, recycle bin, SHA-256, compress, rename, and move to a folder with a picker and an undo |
| P2.10 | Hyperkey and double-tap modifiers | **Double-tap done**, ported from AuraKey and off until asked for. Hyperkey, meaning a key that stands in for four modifiers at once, is not: it has to swallow a key and synthesise a chord, which is a different and riskier mechanism |
| P2.11 | Browser history and bookmarks | **Done.** Chromium and Firefox families, read on demand and never indexed. Off by default |
| P2.12 | Workspace profiles | Not started |
| P2.13 | Live command results | Not started |
| P2.14 | Migration import | **Done.** Snippets and quicklinks both read a file written by other tools, and both write one another tool has a chance of reading. Importing only ever adds |
| P2.15 | Dictation retention policy | Not started |

## Built since the audit, and not on it

- **A file index of Sill's own.** File search no longer needs another program
  installed. Walks the folders somebody chooses, respects `.gitignore`, keeps
  the index between runs, and updates when files appear or go away.
- **Drives.** Whole volumes can be indexed, with the system folders left out.
- **A frontend test runner.** The window's own logic had none, which is where
  the worst bug of the week lived.
- **A device suite.** Nine checks that only a running Sill can answer.
- **A source check.** Damage that compiles: stray NUL bytes, replacement
  characters, conflict markers.

## Built since the audit, and not on it: switches you press in the list

A Windows switch is drawn as a switch, showing which way it is set, and
pressing it flips the thing without the launcher closing. Turning Wi-Fi off and
having the window vanish answers the only question worth asking, which is
whether it went off, by not answering it.

**The state is read when the row is drawn, never carried by the index.** The
index is built once at startup and searched on every keystroke, and neither is
a place to ask the sound system anything. The reading is taken only if a switch
actually matched the query, and it is cached for a second, so typing
`bluetooth` enumerates the radios once rather than eight times.

**Pressing one re-reads all of them, not just the one pressed.** The audio
outputs are a single choice spread over several rows: turning Speakers on turns
the monitors off, and nothing about the Speakers row says so. A re-search would
answer this too, and would also re-rank, so the row would climb out from under
the cursor of somebody who wanted to press it twice.

**A switch with no state still closes the launcher.** Volume up is a nudge and
lock is a door. Neither has an on and an off, so neither draws as a control,
and Rust decides which is which rather than the window guessing.

The trap, and it cost a working feature for an hour: **the window knows a row
by its id, `sill:system.mute`, and a switch is keyed by what it runs,
`system.mute`.** Asking with the wrong one answers "not a switch", which is
exactly what an ordinary row answers, so every switch quietly kept the state it
had before it was pressed with no error and nothing logged. The translation now
happens in one place with a test on it.

### Reaching one before the pages about it

A switch that does the thing should not sit below three pages that open a
window where the thing can be done. Three causes, each fixed on its own terms.

**The hyphen in Wi-Fi was a wall.** `wifi` is the word people type and the mark
was the only thing between it and the title, so the switch matched as a
scattered subsequence and lost to a PATH executable. Matching now reads through
the marks that join one word, in both directions, so `e-mail` finds `Email`
too. Not spaces: removing those would make every pair of words in a title one
word.

**A phrase only matched inside one field**, so `audio output` found nothing at
all. Every word landing somewhere is now enough, and each word has to be a
whole word of a title or a keyword of its own, which is strict enough that
`the file` still matches nothing.

**A switch nobody has pressed ranks as a familiar one.** A floor, not a bonus,
and the difference is the whole reason it works: recency dominates the frecency
curve, so a settings page opened once earlier today scores 77 and a bonus of a
dozen points is invisible next to it. A page somebody opens repeatedly still
wins, because that is a preference worth honouring. The empty query is exempt,
or twelve switches would climb a list ordered by what you reach for.

### One program's volume, on its own

Windows has kept a separate volume per program since Vista and the only way to
reach it is the volume mixer. Turning one noisy tab down without turning the
music down is a thing people want and nobody has a shortcut for.

**Its own row rather than rows in the root list, and that is a measurement.**
Enumerating the audio sessions costs about three milliseconds, and the root
list runs on every keystroke whether or not anything about sound was typed. It
sits behind an "App Volume" row instead, so it costs nothing until somebody
wants it, and the list it opens is filtered by typing like every other list.

Enter mutes and unmutes, in place, the way a Windows switch does. The rest are
in the action panel: louder, quieter, half, full. All five go through the
action registry, so the panel and the Enter key are one implementation rather
than two opinions.

**The switch answers the row's title.** The system row is called "Toggle Mute",
so its switch says whether mute is on. A program's row is called by the
program's name, so its switch says whether the program is audible. The
percentage underneath says the one thing the switch cannot.

### Moving a file somewhere, and putting it back

The destination picker was the question, not the move. A path typed into a
launcher is a path typed wrong: no completion, no telling whether it exists,
and no way to see that there are three folders called "src". So picking a
folder is a list, the same list everything else uses, and typing narrows it.

**What is offered before a letter is typed decides whether this is one
keystroke or twenty.** The folders already moved to come first, then the
folders sitting beside the thing being moved, then the standard places. The
siblings are read off the disk rather than looked up, and that is the
difference between working and not: a folder made a minute ago is in no index
yet, and a folder somebody just made is exactly the one they are about to move
something into. Searching alone answered "Nothing found" for it.

The folder it is already in is never offered. It is the one destination that
cannot be right.

**It moves between drives.** `fs::rename` cannot cross a volume, so between two
drives it copies and then removes, and a failure part way through takes the
half-written copy with it rather than leaving two incomplete versions and
nothing saying which is real. Verified against two real drives.

Ctrl+Z puts it back. The token is two paths, so undoing a move of a ten
gigabyte folder costs what undoing a move of a text file costs.

### Snippets: a group, a program, and formatting

**A collection is a name and nothing else.** There is no list of them
anywhere: they exist because snippets say they do, and renaming one means
renaming it on the snippets that carry it. A separate table would be a second
place for them to disagree. The name travels in the same field an extension
command uses for the extension it belongs to, because both answer the same
question: which heading does this row go under.

**A snippet can be limited to the programs it belongs in.** A signature belongs
in mail and a code fragment belongs in an editor, and a keyword short enough to
be worth typing is short enough to fire somewhere it should not. Matched on the
program's own name without its extension, so one name covers it wherever it is
installed.

The cost of that is the interesting part. Matching runs inside a keyboard hook,
on every character typed anywhere on the machine, and reading which program is
in front is a window handle, a process handle and a path. So it is **asked for
only when it turns out to matter**: match on the text first, and consult the
foreground only when the snippet that won is limited. Nearly always it is never
called at all, and there is a test that fails if it ever is.

A limited snippet does not expand when the program cannot be read. The safe
direction is not firing: a signature appearing in the wrong window is worse
than one that has to be typed.

**Formatting only travels through the clipboard.** There is no way to type
bold, so a snippet that has any is pasted however short it is, both formats
written in one go so a plain field still receives sensible text. Placeholders
are expanded twice, and the formatted one is escaped: a value going into markup
is somebody's clipboard or a file name, text that had no idea it was going
anywhere near a tag.

The clipboard is borrowed and put back through the same guard the text
transforms use. That fixed something as well as reusing something: written by
hand, the write and the restore were two changes the history recorded, so
pasting a long snippet used to leave the snippet and then the user's own older
entry sitting at the top of the history as though they had just copied both.

### How long it takes to reach the launcher

The two numbers the audit refused to let anybody claim without measuring.
Measured on the release build, on this machine:

| | |
| --- | --- |
| Summon, median of eight | **25 ms** (22 showing, 5 painting) |
| Summon, worst of eight | 40 ms |
| Cold start, to the hotkey working | **846 ms** |

**A summon is two halves and only Sill can see both.** Rust knows when the
window was told to show itself; the page knows when it painted, and a window
that is up and blank is not a launcher you can type into. So the page reports
the second half back, from inside the same frame that takes focus, because
taking focus is the last thing that has to happen before a keystroke lands
somewhere useful.

A summon that never reports is kept as half a measurement rather than thrown
away. "Shown in 9 ms and never painted" is the most interesting thing this
could record, and dropping it would hide exactly the failure worth knowing
about.

Cold start is asked of Windows rather than measured from the first line of our
own code, because the loader and the runtime are part of what somebody waited
for. Measuring from `main` would report a number that flatters us by exactly
the part we cannot see.

`scripts/measure-summon.ps1` starts a fresh copy, presses the key, and fails if
the median summon or the cold start is over budget. The numbers are also in
Settings under Advanced, because a launcher's pitch is that it is quick and
that is a claim about numbers.

### Carrying quicklinks in and out

The same shape snippets already had, and deliberately so. Somebody arriving
with thirty saved searches is not going to retype them, somebody leaving should
not feel trapped, and somebody with two machines should not keep them in step
by hand.

Reading is forgiving and merging is not. A file can be written by anything and
is read as generously as it can be understood: a bare array or one wrapped in
an object, and any of `link`, `url` or `target` for the address. What happens
to the links already here afterwards is exactly one predictable thing.
**Importing only ever adds.** A link that is here and not in the file stays,
because somebody importing a colleague's file should not lose their own.

Two things an import will not do quietly. It will not let two links answer one
keyword, which is a coin toss every time it is typed, so the arriving one comes
in without it and the count says how many. And it will not overwrite how often
something has been used, because that is this machine's history rather than the
file's, and a zero from somebody else's export would throw away the ranking.

**A backup that restores less than it saved is not a backup.** Snippets grew a
collection, a program list and formatting, and the export wrote six fields by
hand, so all three were dropped on the way out and the way back in. Nothing
failed; the snippets simply came back plainer than they went. There is a test
that exports and reads back now.

### A picture of the window you are switching to

A list of titles tells you which application and often not which window: four
browser windows are four rows reading almost the same. A picture answers in one
glance what a title cannot answer at all.

**One window is photographed, never the list.** Opening the switcher on twenty
windows must not photograph twenty windows, so the picture is taken for the
selected row only, after the selection has settled for ninety milliseconds.
Holding an arrow key walks past windows without photographing any of them.

Measured on the release build, per preview:

| | |
| --- | --- |
| Photographing the window | 9 to 25 ms |
| Making it small | 3 to 7 ms |
| Encoding it | 2 to 5 ms |

Made small **before** it is encoded, because encoding is the expensive half and
a full-size window is four million pixels nobody is going to look at. Averaged
rather than sampled: taking every eighth pixel of a window full of one-pixel
text turns it into noise, which reads as a broken preview rather than a small
one.

Worth knowing before anybody optimises something that is not slow: **on a debug
build the same work takes twenty times as long** (125 to 414 ms to shrink),
because none of it is optimised. The numbers above are the ones that ship.

A handful are kept while the switcher is open, so arrowing down and back does
not re-photograph anything, and **the whole lot is dropped when it closes**. A
preview is a picture of a moment, and keeping them would mean showing a window
as it was the last time somebody looked.

A minimized window has nothing on screen to photograph, so it shows no picture
rather than a grey box. The strip keeps its width either way: a list that
shuffles sideways as you arrow past one is worse than an empty strip, because
the row being read moves.

### Double-tapping a modifier

The gesture every launcher eventually grows, because it needs no chord and no
key anything else wants: the modifier keeps doing its own job, and doing it
twice quickly is a thing nothing else listens for.

The two-phase state machine is AuraKey's, which had the shape right. Three
things are new, and all three are because a **modifier** is being watched
rather than an ordinary key. **A held key is one press**, because Windows
repeats a held key and a modifier leant on would otherwise confirm a tap
nobody made. **Anything typed in between cancels it**, because Ctrl, C, Ctrl
is somebody copying and reaching for another shortcut. **Either side of the
pair counts**, because left and right Control are two keys to Windows and one
key to a person.

There is no tick. AuraKey expired a stale first press from a loop it already
had; Sill has no such loop and is not growing one for this, so expiry is
decided when the next press arrives, which is the only moment anything looks.

**One hook, two consumers.** A low-level keyboard hook is called for every
keystroke on the machine, in every application, and a second one for a second
feature would double that for nothing. Whether it is installed is one
question, asked in the one place that knows: startup and the settings window
both ask the expander rather than each deciding for themselves.

Off until asked for, like snippet expansion and for the same reason.

## Built since the audit, and not on it: web search

The row every launcher has and the audit never listed. Type anything and the
last thing offered is to look it up. DuckDuckGo by default, four other engines,
or an address of your own with `{query}` in it.

**Last, always, and deliberately not ranked.** It answers every query, so any
score at all would eventually put it above something that actually matched. It
is appended after the commands, files and pages instead, so it is only reached
by somebody who has looked past all of them.

**Every row wears the mark of the program behind it.** The search row carries
the default browser's icon, and a page from history carries the icon of the
browser it came out of. Searching the web is not Sill doing something, it is
Sill handing the question to a browser, and a row branded as Sill's would say
otherwise. Same rule the Windows switches follow.

**It is a quicklink that ships with Sill.** The engine is a template with
`{query}` in it, resolved by `quicklinks::resolve`, so the one hard decision
(escape only what a placeholder produced, never the literal URL around it) is
made in one place rather than two. Sill had this machinery from the start and
was not using it.

## How browser search reads what it reads

Worth writing down, because the shape is not obvious from the code and the next
source that reads somebody else's files should copy it.

**Nothing is indexed.** History is the largest body of text on a normal machine
and the fastest changing: this one holds 37 MB across three browsers. Folding
that into the index would multiply it many times over to answer a question only
ever asked while somebody is typing.

**Nothing is opened.** Chromium takes an exclusive lock on its history, so
reading it while the browser runs fails outright, and Firefox writes through a
journal so a reader can see a torn view. A copy is taken and the copy is read,
along with its write-ahead log, or the newest pages are missing. The copy is
reused for five minutes, because taking a 31 MB copy per keystroke would be
worse than not having the feature.

**It is off by default.** Nothing else Sill reads is as personal, and helping
itself to a browsing history because a browser is installed is not Sill's
decision to make. The settings pane names the browsers it found, so the
question "whose history?" is answered before the switch is touched.

Measured here: 129 ms for the first search of a five minute window, 46 to 76 ms
after, behind the same debounce file search uses.

## Already built, in AuraKey

Four of the P2 items are not really unstarted. They exist, working, in
`winters27/AuraKey`, which is the same stack: Rust, Tauri 2, Windows.

| Sill item | What is already there | Where |
| --- | --- | --- |
| P2.2 Screenshot | `capture_region` and `capture_screenshot` by GDI BitBlt, plus `list_monitors` | `src-tauri/src/ocr_watcher.rs` |
| P2.3 OCR on demand | `Windows.Media.Ocr` over a `SoftwareBitmap`, no model download. Exactly what the audit specified, already written | same file, 534 lines |
| P2.10 Double-tap modifiers | `DoubleTapTracker`, with the phases worked out | `src-tauri/src/executor.rs` |
| P3.9 and P3.10 Automation and workflows | A macro engine with three execution models: sequential, timeline scheduled to sub-millisecond precision, and continuous tick loops. A `MacroAction` enum that already includes `RunProgram`, which is most of P2.8 | `executor.rs`, 25 KB |

There is more beside it: `input.rs` (23 KB), `recorder.rs` (38 KB) and
`daemon.rs` (42 KB).

**All of it is Brandon's own.** AuraKey and Aura Battlemate are both his, and
Battlemate is the project AuraKey grew out of, so there is no third party
anywhere in the lineage and nothing to clear. Code can move into Sill and be
published under MIT because he holds the copyright on it.

The only question was ever what to publish rather than what may be used.
AuraKey went private on 2026-08-29, which settles it: the code is available to
this project and not to anyone else.

**The OCR came over on 2026-08-29** and is P2.3, done. `src-tauri/src/ocr.rs`:
the recognition, the pre-multiplied Bgra8 bitmap, the awkward
`IMemoryBufferByteAccess` cast and the upscale for small captures are all
AuraKey's, unchanged in substance. What is new is getting a clipboard image
into the shape it wants, since clipboard blobs are RGBA PNG and Windows wants
Bgra8.

Verified against a picture with known words in it, read exactly, in 35 ms.

**Three ways in, one implementation.** A capability nobody can find is a
capability nobody has, so it is reachable as a row in the list ("Extract Text
from Image", found by typing ocr, read, text, scan, picture or screenshot), as
an action on any clipboard row, and as a key bound to it through the new
`ClipboardImage` binding source. All three end at the same action against the
same picture, resolved by `bindings::last_image`.

No capture surface was needed for any of it: screenshot with the shortcut
Windows already has, then ask for the words.

### The editor's tools

Box, arrow, ellipse, pen, highlight, hide, text, **numbered badges** and
**crop**, with a select tool that picks any of them back up.

**Badges number themselves** one past the highest so far rather than by
counting, so deleting the third of five does not hand out five again. Removing
one renumbers the rest, because deleting the second of four should leave one,
two, three. Where they start is a setting: writing the second half of a
walkthrough starts at seven. The digit is drawn black or white by the
badge's own luminance, since yellow is one of the six colours offered.

**Crop does not trim the picture.** It sets a rectangle everything reads
through, so it can be adjusted or lifted and the pixels outside it are still
there. Marks stay in the picture's own coordinates, which is what stops them
sliding when the crop moves.

### Four ways to take one, and what follows

- **An area**, dragged in the picker.
- **A window**, by clicking it in that same picker: the window under the
  pointer lights up and a click takes the whole of it, including the parts
  another window is covering. `PrintWindow` with `PW_RENDERFULLCONTENT` asks
  the window to draw itself rather than copying what is on screen. It is not
  universal, so the result is checked for being one flat colour and falls back
  to reading the screen, which is at least what somebody can see.
- **Every screen at once.**
- **One display**, by number.

Two bindable keys, area and whole screen, both empty by default because there
is no obviously free combination and a default that collides is worse than
none. The settings recorder now works on any hotkey by name rather than a
hardcoded pair, and checks a new key against **every** other rather than one of
them: Windows registers the first and refuses the second, so a collision is a
key that silently does nothing.

**What follows a capture is one setting**, read in one place, so the four ways
of taking a picture cannot disagree. It always reaches the clipboard; whether
the editor opens on top of that is `screenshot.after`. The editor's starting
tool, colour and stroke width are settings too.

**Screenshot capture (P2.2) came over the same day.** `src-tauri/src/capture.rs`
is AuraKey's BitBlt into a memory device context and its `GetDIBits` read-back
with a negative height, plus cleanup on every path out, an error when the read
fails rather than a blank picture, and `virtual_screen` so it knows where the
screen is when there is more than one of them.

Measured here: the whole desk, 3640x2241 across two displays with a negative
origin, in 137 ms.

**Picking an area** is a window of its own laid over every screen, sized in
physical pixels because logical ones differ per display and there is no single
scale that works on a desk with one at 150% and one at 100%. Four dimming
panels are laid around the selection rather than a mask being cut out of a
sheet, so the area being picked stays exactly the colour it will be in the
picture. The overlay hides itself and waits before the read, or it is in its
own photograph.

**Markup** is `src/routes/markup/+page.svelte`: box, arrow, ellipse, pen,
highlight, hide and text, with undo. The picture is never drawn on. Shapes are
kept as a list and painted over it each frame, so undo is dropping the last one
and nothing keeps a copy of a thirty megabyte image per stroke.

**Hide averages blocks rather than blurring.** A blur is a filter somebody can
partly undo; averaging a block throws the pixels away. If it is used on a
password it has to actually hide it.

Verified by driving it: dragged an area, got exactly the rectangle asked for,
opened it for markup, drew a box, and the same size came back with the box in
it.

## What Windows will not let a program do

Two things on P2.5 are listed as not started and should be listed as not
possible, at least not honestly.

**Do not disturb** has no public way to set it. It moved from Focus Assist to
Do Not Disturb and neither has ever had an API; what exists is an undocumented
notification-state call that has changed between releases. A switch that
silently stops working after an update is worse than no switch.

**Night light** is stored as an opaque blob in the registry, with a timestamp
and a checksum inside it that have to be rebuilt by hand. It has been reverse
engineered and it breaks: the format is not a contract and Windows rewrites it.

Both are one row away as "open the settings page for it", which the settings
catalog already offers. That is the honest version and it is already there.

**Switching the audio output is different**, and worth the exception. It is
also undocumented, through `IPolicyConfig`, but that interface has been in the
same place since Vista and is what every audio switcher on the platform uses.
It is declared by hand in `src-tauri/src/audio.rs`, and the methods before the
one that is wanted are placeholders that put it at the right vtable offset.

## Decisions waiting

**P1.11, the extension install path.** Extensions are TypeScript and JSX, and
Node runs neither. No bundler is needed, because the host already resolves
modules, but a transpiler is. Measured: `typescript.js` is 8.7 MB of pure
JavaScript and already vendored; `sucrase` is 1.1 MB with seven dependencies;
`esbuild` is a 10.1 MB binary **per platform**. Writing one is a real TS and
JSX parser with a long tail of silent wrongness. This is an install-size call
rather than a technical one.

**P1.14, the native dictation panel.** Its justification was that a second
window costs a second renderer. That was measured and found to be false: both
windows share one. The item should be re-scoped or dropped rather than built
for a reason that turned out not to hold.

**Re-indexing cost.** About a tenth of one core while files are changing in an
indexed folder, and zero at rest. The real answer is patching the index for the
file that changed rather than walking everything again. See `docs/budgets.md`.
