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

**Search knows what changed instead of looking again.** A file created,
renamed or deleted patches the index in place, so a folder somebody is working
in stays current without the walk that used to follow every change. The index
and the ranker now agree on what counts as a match, which was the cause of
results that were found but sorted as though they were not.

**Tab finishes a path.** Typing part of a path and pressing Tab completes it as
far as every match agrees, the way a shell does, with a trailing separator on a
folder so the next Tab reads inside it.

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

**A string somebody else wrote is not an address Sill will open.** Text arriving
from a file, a clipboard entry or an extension is no longer treated as a link
just because it looks like one.

**Cloud files keep their icons without being downloaded.** A OneDrive
placeholder used to be fetched twice over just to draw a row. Paths longer than
260 characters are indexed rather than silently dropped, and a network folder
that is not there no longer holds up indexing while SMB gives up.
