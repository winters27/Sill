# Where Sill is

What the audit of 2026-08-29 laid out, and what has actually been built since.
Kept here rather than in a planning tool because it is about this code and
should move with it.

A line is done when something checks it. Where that is a test, it is named.

## P0, foundation and correctness

| | Item | State |
| --- | --- | --- |
| P0.1 | Version control, MIT headers, CI running verify | **Done.** Public at github.com/winters27/Sill, MIT, with `verify` running on every push. Third-party work is named in `resources/NOTICE`, and the one font that may not be redistributed is fetched rather than committed |
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
| P1.10 | Application and command hotkeys | **Done.** Rebuilt 2026-09-05: one recorder (`KeyRecorder`) and one keycap renderer (`Chord`) replace three recorders and four chord drawings; the global keys moved from the settings page into the panel, which retired the window-level key listener that never disarmed. `key_owners` in Rust answers "what already runs on this chord" from the same sheet the keyboard reference draws, so a recorder refuses a key taken in its own section and mentions one taken elsewhere, where three unrelated checks used to each see a third of the keys. The reference gained a "From anywhere" section (global hotkeys and bindings) and a `refused` flag, so the key sheet, the tray menu and the panel's keyboard map read one answer; the tray menu shows no key rather than a refused one. Action keys arrive grouped by the first kind they act on, decided in Rust, with a filter and unbound rows hidden by default. The panel's hero is `KeyMap`: the keyboard with every bound key lit, following a held modifier. The four 120 ms re-reads after a write are awaits. Later the same day: the browser chrome is off in every window (`webchrome.rs`, read back rather than trusted, plus `quiet.ts` cancelling the context menu and the browser keys in the page), and a Sill window in front honours the global hotkeys itself through `hotkey_chords`/`press_hotkey`, because the low-level hook was measured not to receive the press of a key while a Sill window had the keyboard, only its release, with the hook thread answering pings at 0 ms throughout |
| P1.11 | Extension install path | **Done.** Point at a folder and its commands are built and listed, or browse the store and install from there. esbuild ships beside the host; the build is Rust, not a repository script |
| P1.12 | Emoji and symbols | **Done.** In the picker and in ordinary searches |
| P1.13 | Snippets: collections, rich text, app-specific, forms | **Done.** Placeholders, import and export, collections, formatting, and limiting one to the programs it belongs in. A snippet with named holes is asked about one at a time in the launcher's own field, counted out so it is clear how many are left. The keyword expander keeps its old behaviour, because it fires while somebody is typing elsewhere and there is nothing to ask on |
| P1.14 | Native dictation panel | **Dropped 2026-08-31.** Its justification, removing a second renderer, was measured false; both windows share one. Reconsider only on a recording-time processor measurement |
| P1.15 | Disabled means stopped | **Done** |

## P2, power user

| | Item | State |
| --- | --- | --- |
| P2.1 | Activity history and undo | **Done.** Every action is recorded and what can be taken back says so, in Advanced. "Undo Last Action" works after the launcher has closed, which is when it is wanted |
| P2.2 | Screenshot | **Done.** Drag an area or take every screen, with a markup editor: box, arrow, ellipse, pen, highlight, hide, text |
| P2.3 | OCR on demand | **Done.** WinRT recognition, ported from AuraKey. Reads a picture on the clipboard, never automatically. Measured: 35 ms on a 640x160 capture |
| P2.4 | Read aloud | **Done.** Any text a transform accepts can be spoken instead, in the system voice, over OpenAI's speech shape, or in a neural voice Sill downloads and runs. Stopping is its own action, because silence is wanted after the text has left the screen |
| P2.5 | System control | **Done.** Volume, mute, dark mode, lock, audio output switching, Wi-Fi and Bluetooth, all of them switches you press in the list without leaving it, plus a program's own volume behind its own row. Do not disturb and night light have no public way to set them, see below |
| P2.6 | Process and resource view | **Done.** A readout rather than a task manager: processor and memory as bars, the five heaviest programs, and Sill's own weight underneath. Nothing selects and nothing acts |
| P2.7 | Terminal execution, capability gated | **Done.** `shell.rs` runs a command with three bounds: output capped and the pipes still drained, a deadline, and cancellation, all three killing the whole process tree through a job object. `ShellExecution` is its own capability, so the model raises a card and extensions cannot reach it. Output has a surface of its own, shown while the script runs, with Escape stopping it before it leaves |
| P2.8 | Scripting | **Done.** Raycast script headers are read as written, in PowerShell, cmd, bash, Python or a bare executable. Found by search, run from the launcher, output shown per the header's mode. Declared arguments are asked for one at a time in the author's own words. A key or the model can answer the first of them, so a script that asks for something is no longer one the window alone can run. Under Sill's own `@sill.` marker a header can also name a folder to run in and variables to run with, both of which reach the child without going near a command line, and can ask for administrator rights, which are granted per script in Settings and never by the file |
| P2.9 | File actions | **Done.** Copy path, copy name, reveal, open a terminal, recycle bin, SHA-256, compress, rename, and move to a folder with a picker and an undo |
| P2.10 | Hyperkey and double-tap modifiers | **Done.** Both live in the snippet expander's existing hook rather than a second one. The hyper key never holds a modifier down: each keystroke is one complete chord, sent and released together, so nothing is stranded if Sill stops. Off by default, chosen from a short list of keys nobody uses for anything. **The logic has tests; the hook and the injection are not verified on a real keyboard** |
| P2.11 | Browser history and bookmarks | **Done.** Chromium and Firefox families, read on demand and never indexed. Off by default |
| P2.12 | Workspace profiles | **Done.** Name an arrangement, and restoring it starts the programs that are closed before putting every window back where it was, rescaled onto whatever displays are attached now. One start per program rather than one per window, and a program already open is left alone |
| P2.13 | Live command results | **Done.** The Widgets row shows what the machine is doing and changes while the launcher is open. **Rust refuses to measure when the window is not visible** and answers with nothing, which is what stops the timer, so no route out of the launcher can leave it running |
| P2.14 | Migration import | **Done.** Snippets and quicklinks both read a file written by other tools, and both write one another tool has a chance of reading. Importing only ever adds |
| P2.15 | Dictation retention policy | **Done.** Days a transcript is kept, set beside "Keep a history" in Dictation settings. Zero keeps everything, and it is the default here on purpose |

## P3, ecosystem

| | Item | State |
| --- | --- | --- |
| P3.1 | Capability model: declared permissions, consent, an inspector | **Done.** Extensions speak the same `Capability` an action speaks, every one of the twenty-two host methods is gated, and Settings, Extensions, Permissions lists what each one holds with a Revoke beside it. Asked on first real attempt rather than at install, because a Raycast manifest declares nothing and a consent screen that says "this could reach anything" teaches people to agree without reading |
| P3.2 | Extension sandboxing: deny `fs`, `net` and `child_process` unless declared; per-extension budgets; suspend on idle; activation events | **Done.** Ten Node built-ins are refused in `patch-require` unless the permission is held. A worker pinned at 0.95 event loop utilisation for thirty seconds is stopped and says why. `fetch`, `WebSocket` and friends are refused too, which the module gate could never have caught because they are globals rather than requires. A session nobody unloaded is let go with the host rather than keeping it resident. Memory was already capped per worker. **Activation events were not built and will not be**: nothing loads an extension before somebody runs its command, so there is no eager load to defer. **Not containment, and the code says so.** The gate is an **allowlist**: a named set of built-ins is handed over, a second named set costs a permission, and everything else Node ships is refused whatever is granted. It used to be a list of dangerous modules, which is a list every Node release adds a hole to, and it had five, `dns`, `cluster`, `sqlite`, `v8` and `wasi`, all reachable by an extension granted nothing. Every route to a built-in now meets the same answer: `require`, `Module._load`, `module.createRequire`, `process.getBuiltinModule`, `process.binding`, and a dynamic `import()` through a `module.registerHooks` resolve hook, which is why extensions need Node 22.15. `process.dlopen` is refused outright, since native code runs outside every permission there is. `process.kill` and `process.report.writeReport` are globals that were nobody's idea of a module and are gated too. `host/test/integration.mjs` refuses each escape and then reaches the disk with the same fixture once the permission is granted, so the refusal means something. **`eval`, `new Function` and `WebAssembly` were named here as ways out and are not**: generated code cannot see `require`, and every global it can see is wrapped. What it defeats is the store's scan, which reads source text, and that is a limit on the description rather than on the gate. What still gets out is named rather than implied: a permission is granted whole, a dependency shares the worker, `processLaunch` puts the started program beyond all of it, and `process.env` is readable |
| P3.3 | Extension store | **Done**, separately. 2026-09-05: an extension command ranks above the application of the same name within one match class (`from_an_extension` in the registry sort), and an extension's name is matched as a name for every command it provides rather than as a category, which was `Elsewhere` (five tests, two of them sabotage-checked); the store draws "Installed" and "Update" as pills, the detail pane shows the key and the verb, and the hint strip uses keycaps at the meta size |
| P3.4 | `@raycast/utils` | **Done.** The hooks the store is written against: `usePromise`, `useCachedPromise`, `useCachedState`, `useFetch`, `useLocalStorage`, `useForm`, `useFrecencySorting`, plus the toast, icon and cache helpers and `runPowerShellScript`. What cannot work here throws its own name rather than being absent. `useCachedState` hydrates from storage a moment after launch rather than synchronously, because Raycast reads a file on the same thread and Sill's storage is an RPC |
| P3.6 | AI provider abstraction | **Done.** Eight providers, keys under DPAPI |
| P3.7 | Quick AI and selected-text AI | **Done.** From the launcher and in its own window |
| P3.8 | AI tool calling bound to the action registry | **Done.** The same registry, the same capabilities, the same approval card |
| P3.9, P3.10 | Automation and workflows | Not started |
| P3.11 | MCP server | **Done.** The same tools over MCP, through the same gate |

## The Raycast gap, 2026-09-05

Raycast's own list of what it can do, checked item by item against Sill.
Twenty-eight features in eleven tranches; the plan and the check that
shaped it are in the vault. A row is done when the named test is green.

| | Item | State |
| --- | --- | --- |
| G0 | Shared foundation: a one-shot model call, a capture overlay that hands a rectangle back, date helpers | **Done.** `ai::oneshot` with no tools and no conversation; `commands::system::choose_region` with `Purpose` and a sixty second patience; `timers::weekday`. Checked by `oneshot::tests`, `system::choosing`, `dates::tests::the_weekday_of_the_epoch_was_a_thursday` |
| G1.1 | Date arithmetic in the search field | **Done.** `dates.rs` in front of the calculator: `today + 3 weeks`, `days until`, date minus date, month arithmetic that clamps the day. The calculator now refuses anything holding an ISO date, which was answering `2024-08-28` with 1988. Checked by `dates::tests` and `calculator::tests::forty_ordinary_queries_are_not_mistaken_for_calculations` |
| G1.2 | Calculator history | **Done.** `sums.rs`, fifty kept, read only when `sums` is typed, remembered by `CopyAnswer` on Enter. Checked by `sums::tests::nothing_is_read_unless_asked` and `fifty_is_the_most_kept` |
| G1.3 | Colour formats and a picker | **Done.** `colour.rs` reads hex, `rgb()` and `hsl()` and answers with the forms not typed, as answer rows carrying the hex in `icon` for the swatch. `Pick a Colour` is a builtin that puts the overlay up with `Purpose::Colour`, reads one pixel and copies it. Checked by `colour::tests::hsl_round_trips_through_rgb` and `a_captured_pixel_reads_as_the_colour_it_is` |
| G1.4 | World clock | **Done.** `zones.rs` reads Windows' own zone table (`EnumDynamicTimeZoneInformation` plus each zone's registry `Display` for its cities) once an hour at most, and ICU maps a zone to the IANA name the browser's clock understands. `tokyo time` is an answer row; the widget resolves its cities once per settings change and ticks on machine time, asking Rust nothing a minute. Checked by `zones::tests::the_difference_is_said_in_hours_and_halves`, `a_city_is_matched_as_a_whole_word` and `nothing_is_read_unless_asked` |
| G2.1 | Quit all applications | **Done.** `processes::quit_all_targets` picks the visible programs that are not the shell, not protected and not Sill; the builtin says the count and asks on a native dialog before `quit` sends each its own close. No undo, which is why it asks. Checked by `processes::quitting_everything` |
| G2.2 | Confetti | **Done.** A deferred transparent window over every screen that ignores the mouse, drawn from `$lib/confetti` and put away by the page itself once every piece has fallen off the bottom. Checked by `confetti.test.ts` and `lazy_windows::tests` |
| G2.3 | Quicklinks to app deep links | **Done.** `Quicklink.allowed_scheme`, one scheme per link, granted only in the editor; `reach::target_allowing` opens exactly that scheme and nothing on `NEVER`; an import and an export both leave it behind. Checked by `reach::allowances` and `quicklinks::transfer::allowances` |
| G2.4 | Clipboard entry name and edit | **Done.** A `title` column and `set_text` that moves the hash and refuses a collision; `sill.clipboard.rename` and `sill.clipboard.edit` borrow the field like renaming a file, both undone by `Undo::RestoreClipboardEntry`. Checked by `suite::clipboard_edit` |
| G2.5 | The model reads a region of the screen | **Done.** `read_screen` takes a `region`, clamped to the screens there are, or `choose`, which puts the capture overlay up with `Purpose::Choose`; the card names which. Checked by `tools::reading_a_region` |
| G3.1 | Installed fonts with a preview line | **Done.** `fonts.rs` enumerates GDI's families once per ten minutes when `font` is typed, tidied of vertical faces and duplicates; `ObjectKind::Font`, a row whose sample line is set in the face, `sill.font.copyName`. Checked by `fonts::tests::nothing_is_read_unless_asked` and `suite::real_fonts` |
| G3.2 | Display resolution and refresh rate | **Done.** `displays.rs` lists a display's 32-bit modes from `EnumDisplaySettingsEx`, tests a change with `CDS_TEST`, applies it with the registry updated, asks on a native dialog raced against fifteen seconds and puts the mode back on silence or Revert; a late Keep re-applies it. `Undo::RestoreDisplayMode`. Checked by `displays::tests::modes_are_deduplicated_and_ordered`, `a_rows_target_reads_back_as_the_mode_it_was` and `suite::real_displays` |
| G3.3 | Custom window layouts, each bindable | **Done.** `layouts.rs` keeps rectangles as fractions of the work area, clamped and rounded so shared edges meet; `sill.window.layout` takes the layout's name as its argument, which is what a key records and the panel asks for. Checked by `layouts::tests::two_half_layouts_tile_the_work_area_exactly` and `a_layout_never_leaves_the_work_area` |
| G3.4 | Tags on snippets, quicklinks and the clipboard | **Done.** A tag is a `#name` keyword on the row, so a plain word finds it; `registry::tag_operator` takes `tag:name` out of the query and keeps only tagged rows; on the clipboard the tag is the collection of that name. Checked by `registry::tags` and `suite::clipboard_collections::a_collection_name_answers_a_tag_filter` |
| G7.1 | QR codes | **Done.** `qr.rs` over `rqrr` with its `img` feature off, so the whole `image` crate is not pulled in to convert pixels Sill already has: the greyscale entry point takes a closure over the BGRA buffer. `sill.clipboard.readQr` on any picture in the history, and a `Read a QR Code` builtin that drags a box through `Purpose::Qr`. **The payload is copied, never opened.** Checked by `qr::tests::a_known_code_decodes`, which carries a real code as its modules rather than a binary fixture |
| G7.2 | Image conversion | **Done.** `images.rs` over Windows' own imaging component, so what Sill reads is what the machine reads and no crate was added: `Graphics_Imaging` was already on and only `Storage_Streams` is new. Reads WebP, and HEIC when both free Store extensions are there, which the refusal names because Windows' own message does not. Writes PNG and JPEG only, since Windows ships no WebP encoder. The new file lands beside the original under a free name, which is what makes `Undo::DeleteFile` honest. Checked by `suite::real_images`, which round-trips a PNG through the real codecs and back |
| G7.4 | File search by content | **Done.** `content.rs` greps the files a name search already found, under three bounds that each have a bad case the others do not cover: two hundred files, half a megabyte of each, a third of a second. It stops between files when the search is overtaken, which needed `Searching::claim`, a movable form of the stale token, because the work has to happen off the async runtime. `content:` with no name to match on orders candidates newest first, since the question is then "somewhere in what I have been working on". The row shows the line. Checked by `content::tests`, whose four bounds were each broken and watched go red |

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

### Asking a model, from the launcher

Tab asks whatever is in the field. It is the gesture every launcher with an AI
in it has settled on and it is the right one: the question is already typed,
because searching for something and asking about it start the same way. Escape
comes back to the results with the words still there, so nothing is lost if the
search was what you meant.

Enter asks a follow-up. The conversation is held in Rust rather than in the
window, because the window is closed most of the time and reloaded whenever the
page does; a conversation living there would be lost every time somebody
pressed Escape, which is the opposite of what a follow-up is for.

**Two ways to reach a model, and only one of them needs a key.**

Over HTTP, in the shape nearly everything speaks: OpenAI, xAI, Google's
compatibility route, OpenRouter, LM Studio, Ollama, or anything else pointed
at. One adapter for all of them, and a second for Anthropic's own format when
it is written.

Or through **Claude Code**, which is already installed and already signed in,
on the subscription. That is the sanctioned way to reach one: the Agent SDK,
`claude -p` and third-party tools all draw on the ordinary subscription pools,
and running the official tool is a different thing from minting its tokens.
Sill holds no credential at all on that path.

The same path reaches every other provider too, because Claude Code reads
`ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN`. Other tools write those into
the user's own settings file; Sill sets them in the environment of the process
it spawns and nowhere else, so it cannot break somebody's Claude Code setup and
there is nothing to restore if it is killed half way.

**Where a key may be sent is a rule, not a checkbox.** Plain http to this
machine or this network is fine, because a local model has no certificate and
never will. Plain http anywhere else is refused: the key and every word of the
conversation would be readable by anything on the path. A checkbox marked
"allow insecure" is one people tick to make an error go away.

Keys are sealed with DPAPI before the preferences file is written. The sealer
learned to walk arrays for this, since there is a key per provider and no fixed
path names them all.

**Choosing, in Settings.** An Ask panel lists what is set up, which one answers
and what each of the rest would involve. Three of the services on offer have a
subscription with the same name that does not pay for the thing being set up,
and the row says so before anybody pastes a key.

**Models are asked for, not typed.** A model id is a string, and one character
wrong is a request that fails talking about a model nobody meant to name. The
HTTP providers publish their list and Claude Code offers its own aliases, with
"whatever Claude Code is set to" first: a choice made in Claude Code itself
should not be overridden by a launcher. A service that will not say what it has
leaves a text field, which still works.

A model on somebody's own machine names no default. What is installed differs
per machine, so any name shipped in the table is one most people do not have,
and it arrived as a picker showing nothing beside a line saying there were five
to choose from. It takes the first thing actually installed instead. A model
that is set but no longer offered is still listed rather than blanking the
control, because a picker showing nothing beside a stored value reads as the
setting having been lost.

Verified by driving it: a local model answers, keeps context across a follow-up,
and switching the model persists. **The Claude Code path is not verified end to
end.** Its refusal is, and reads correctly.

That was first written as "the CLI is not installed on this machine", and that
was wrong. It is installed: the desktop application carries its own copy under
`%APPDATA%\Claude\claude-code\<version>\claude.exe` and puts nothing on `PATH`,
so a machine with the desktop application and nothing else looked exactly like
a machine with no Claude Code on it. `locate` now looks there too, newest
version first by number rather than by name. What actually stands in the way
today is that the sign-in on this machine has expired: the CLI answers
`Failed to authenticate: OAuth session expired and could not be refreshed`,
which needs `claude` run once in a terminal and is nothing Sill can do from
its side.

Three things the panel work turned up that were broken elsewhere. Tab outside
the root list fell through to the browser's own focus key, and since the only
other focusable thing is outside the window, the field lost focus and the
launcher dismissed itself on blur: a key that did nothing looked like a key that
closed the launcher. Escape out of a conversation cleared the field, next to a
comment promising the words would still be there. And Windows draws an open
picker list in a window of its own and starts it white however the page is
painted, so three panels remembered to colour the options, two did not, and Web
Search had no rule at all and drew a white control on dark glass. One `Select`
and one `TextField` decide it once now, and `verify:source` refuses a
hand-rolled picker in settings.

### The chat, as Dosage's, 2026-09-05

A model at work used to be a line that said Thinking and a grey list of
tool names. Now the window shows what it is thinking, folded to one line
once the answer starts; each tool it reaches for, in words, with whether it
managed; the words arriving at a steady pace rather than in clumps; and an
orb, in the theme's own colours, that moves only while a turn is live.

Rust records all of it in the order it happened, as `parts` on the stored
answer, so a conversation reopened tomorrow shows the working as well as the
words. Both transports record through one path, which is what makes a Claude
Code turn look like an HTTP turn: it used to run six tools in silence. Stop
now kills the Claude Code process rather than leaving it to finish unread.

The launcher's inline chat and the chat window draw the same components,
which used to be two copies of the same rows with different scroll rules.
### P3.12, the extension store

Deferred since P1.11 with a note saying a registry is "a different problem,
made of trust and updates rather than of transpiling". Both halves of that
turned out to be the design, so this is what each one answered.

**Two sources, and the split is the point.** The catalogue comes from Raycast's
public store index, because nothing else aggregates it: the repository holds one
`package.json` per extension and no summary of itself, so the alternative is
three thousand requests to build a list of titles. **The code comes from
`github.com/raycast/extensions`, which is MIT, at the exact commit the
catalogue names.** Sill never downloads a built bundle from anybody. What lands
on the machine is source, at a revision that is written down, transpiled by the
same esbuild call a folder install uses. If the index changes shape or goes
away, browsing stops and installing from a folder does not.

**Not git.** `.github/workflows/verify.yml` does a sparse blobless clone for the
two extensions the view gate builds, and it works. It also needs git, which
would be a third program somebody has to have for one feature. Plain HTTP
reaches the same bytes: measured on `uuid-generator`, **three API calls, 19
files, 158 KB, 2.1 seconds**. Only the calls to `api.github.com` are rate
limited, at sixty an hour unauthenticated, so about twenty installs an hour; the
file bytes come from `raw.githubusercontent.com`, which is not counted. A token
in settings raises it to five thousand, and it is sealed rather than written to
the settings file.

**Dependencies are not optional.** `uuid-generator` imports `uuid`, `typeid-js`
and `ulidx`, and esbuild bundles what it is pointed at, so an unresolved import
is a build failure rather than a warning. Every store install runs npm. Node was
already required to run any extension at all and npm arrives with it, so this
adds no requirement that was not already there. **The staged source does not
stay**: `node_modules` measured 45 MB for that one extension, and what is kept
is the bundles.

**`--ignore-scripts`, which is the one real limit.** A package's `postinstall`
hook is arbitrary code that runs at install time, before anybody has agreed to
anything and before the extension has ever been launched. Turning it off costs
the rare native package that needs a build step, and that one now fails loudly.

**Installing is two steps because the first one has to be readable.** Step one
fetches the source and reads it; step two runs npm and builds. Between them the
window shows what the code appears to reach, derived from the source that is
about to be built rather than from a description somebody typed. Nothing
executes before the answer.

**What that screen does not claim.** There is no sandbox, and the screen says
so rather than looking like a permission dialog that grants everything anyway.
It over-reports on purpose: the scan is substring matching over the extension's
own source, it cannot see through a dependency, and a parser would be more
precise and still wrong the moment a module name is built at runtime. **The
scan describes and does not decide**, which is what makes that acceptable: the
worker's gate is an allowlist, so a capability the scan failed to notice is
refused at runtime with the permission named rather than quietly allowed. The
one thing on the screen that is a fact about Sill rather than about the
extension is **whether Sill is even in the way**: a capability that goes
through `host_bridge.rs` can be logged and could one day be refused, and one
that is Node reaching the disk directly cannot. A test reads that trait's
source and refuses a method no capability names.

**Updates are a comparison, not a poll.** Every install writes an `origin.json`
beside its bundles recording the commit, the way `tts::piper` pins its model
revision. Out of date is that commit against the one the catalogue publishes,
computed when somebody opens the store. Updating is the same two steps at the
newer commit, including the screen, because an extension can gain the ability
to run programs in a version somebody would otherwise have accepted without
looking. Raycast's own client updates extensions in the background; Sill cannot,
because nothing here runs at rest, so "Update Extensions" is a row in the
launcher.

**Raycast ships for two platforms and its store is one index for both.** Of
3,234 listings, 886 name Windows, 1,048 name macOS and not Windows, and 1,300
name nothing because they predate the field. The middle group is dropped at the
point of parsing and never reaches disk. The last group is kept, marked, and
hidden behind a switch that says how many there are, because treating silence as
refusal would throw away two fifths of the store.

**Nothing runs at rest.** No timer, no warm-up, no revalidation. The catalogue
is fetched when the store is opened and the copy on disk is over six hours old,
and at no other time. It is held while the store is open and dropped when it
closes, which is the bargain `meter.rs` already makes with its previous reading.

Verified end to end rather than compiled: the catalogue was fetched, 2,183
listings survived the platform filter, `uuid-generator` was installed from it at
`6939fc2`, and both a `no-view` and a `view` command were run through
`scripts/run-extension.mjs`, the same runner the view gate uses. Three items,
four actions, zero unimplemented APIs. That test lives in
`src-tauri/tests/store_install.rs` and is `#[ignore]`d, because it reaches two
services and runs npm.

**Still open.** Nothing enforces anything, which is stated rather than solved:
capability gating would mean the host refusing a bridge call an extension did
not declare, and that is a larger piece of work than a store. There is no
progress bar during an install, so a slow npm looks like a stall. And a hung npm
has no timeout, the same as esbuild in `extension_install.rs`.

### P3.11, the tools reach Claude Code

The chat window's eleven tools worked over HTTP and nowhere else, because a
request in that shape carries a tool list and `claude -p` has no request to put
one in. So the one provider that costs nothing to run was the one that could
not look at the machine it was running on.

MCP is the interface it does have. Sill now answers it, and the same work makes
the tools reachable from any other client that speaks it.

**One list, two transports.** Nothing about a tool is written twice. The
catalogue carries the name, the description and the schema; one function shapes
it for a chat completions request and another for an MCP `tools/list`, and a
test asserts the two agree on membership, order, description and schema. That
failure would otherwise be silent: a tool added for the chat window that no MCP
client can see compiles and passes everything.

**Two processes, because stdio means the client starts a program.** The tools
need the running Sill and nothing else will do: the index took a scan to build,
the clipboard history is a database one process has open, the window list is
about this moment, and the approval card has to appear in front of the person
answering it. So `sill.exe --mcp` is a bridge rather than a server. It connects
back to the running Sill over the loopback interface and copies bytes. It
parses no JSON and knows no methods, so there is no second implementation of
the protocol living in a process nothing tests.

**What stops anything else connecting.** A loopback port is reachable by
everything else on the machine, so the first line down the socket has to be a
secret minted when Sill started. The honest boundary is the one `secrets.rs`
already describes: this does not defend against a process already running as
this user, and it does defend against everything that cannot read that user's
files.

**The approval card is not optional here.** Nothing in the MCP layer runs an
action. It reaches `tools::run`, which reaches the same registry, the same
declared `Capability` and the same paused turn the chat window waits on. What
did change is that a card now raises a window to appear in when none of Sill's
are on screen. Over HTTP the window asking the question was visible by
definition; over MCP the caller may have no window at all, and a card nobody
can see is not a gate, it is a refusal ninety seconds later that reads as the
tool being broken.

**The permission mode had to be answered, not assumed.** `--permission-mode
dontAsk` is documented by the CLI itself as "don't prompt for permissions, deny
if not pre-approved", so the tools were denied until they were named. They are
named one at a time, `mcp__sill__<tool>`, derived from the catalogue, in a
single comma separated argument because the flag is variadic and separate
arguments would go on swallowing the session id after them. `--strict-mcp-config`
goes with it: the empty working directory already keeps a project's own servers
out, and this keeps out the ones configured for the user.

Naming an acting tool there is not the same as allowing the action. It permits
the request to reach Sill; the file still does not move until somebody presses
Enter on Sill's own card.

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

**All of it shares this project's author.** AuraKey and Aura Battlemate are the
same author's, and Battlemate is the project AuraKey grew out of, so there is
no third party anywhere in the lineage and nothing to clear. Code can move into
Sill and be published under MIT because the copyright is held here.

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

Decided 2026-08-31 and **built the same day**: esbuild, and the 10.1 MB per
platform is accepted. What it transpiles is code arriving from outside, where a
parser's long tail of silent wrongness produces a broken extension with nothing
to report.

The install-size question turned out to be the only one open. esbuild was
already what `scripts/build-extension.mjs` used, so choosing it cost no new
dependency, only a resource entry: `host/node_modules/@esbuild/win32-x64/`
ships as `esbuild/esbuild.exe` beside the host, found through the same three
candidates `host.js` uses.

`extension_install.rs` is the build itself rather than a wrapper around the
script. Everything that decides anything is a function over data and only two
functions touch the machine, which is what makes a manifest whose command
redeclares one of the extension's preferences a value in a test rather than a
directory somebody has to make. Installing merges, so installing one extension
never uninstalls another, and installing the same one twice updates it.

The store, which that note deferred, is **P3.12 above and built**. It goes
through this same function rather than around it: acquiring an extension and
building one are separate problems, and a second installer would be two answers
to what installed means with nothing keeping them in step.

**Re-indexing cost.** About a tenth of one core while files are changing in an
indexed folder, and zero at rest. The real answer is patching the index for the
file that changed rather than walking everything again. See `docs/budgets.md`.
