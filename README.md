# Sill

A launcher for Windows. One hotkey opens a search field, and what it finds is
apps, files, settings, snippets, links, clipboard history, open windows, emoji,
and the machine's own switches. Anything it finds, it can act on.

Rust and Tauri 2 underneath, Svelte 5 on top. MIT licensed.

**Status: early.** Version 0.1.0, no installer published yet. It is used daily
by its author on Windows 11 and nowhere else. Build it from source.

[Using Sill](docs/guide.md) is the guide for somebody with it open.
[Writing an extension](docs/extensions.md) is the one for somebody adding to
it, and carries the coverage table saying which parts of the Raycast API work.
[MCP servers](docs/mcp.md) covers putting a server you already have into the
action panel, and what one costs.
The keyboard reference is not written down anywhere: press `?` in the launcher
with an empty field and Sill builds it from the keys that are actually
registered.

## What it does

**Search.** Start Menu apps, Windows settings pages, files through
[Everything](https://www.voidtools.com/), Sill's own settings, and everything
below. Fuzzy matching with word-boundary and consecutive-run bonuses, ranked by
frecency, with hyphens and dots transparent so `wifi` finds `Wi-Fi`.

**Act on what you find.** Every result is a typed object, and actions are
declared against object kinds rather than hardcoded per result. A file offers
rename, move, trash, copy path. A window offers halves, thirds, quarters and
centre. Destructive actions return an undo where one is honest.

**Clipboard history.** SQLite with full-text search, image and file entries,
source application, pinning, retention, per-app exclusion. It honours the
Windows clipboard confidentiality formats, so password managers are not
recorded, and it detects likely secrets that do not set them.

**Snippets.** Global keyword expansion through a low-level keyboard hook, with
placeholders for the clipboard, the date, a UUID, and where to leave the caret.

**Quicklinks.** Saved URLs with a `{query}` hole, opened in a browser you name.
Only what a placeholder produced is percent-encoded, never the literal URL
around it.

**Calculator.** `fend` behind a gate strict enough that `v1.2.3-rc1` is not a
sum.

**Dictation.** Local speech to text with whisper.cpp. The model downloads on
first use, loads on demand, and the server shuts itself down after idling.

**Screenshots and OCR.** Area, window, whole screen or one display, with a
markup editor that keeps shapes as a list rather than painting them in, so undo
stays cheap. Text is read off the screen with the Windows OCR engine, no model
download.

**System control.** Volume, per-program volume, audio output device, Wi-Fi,
Bluetooth, dark mode, and the rest of the switches Windows keeps behind a
settings page.

**AI.** See below.

**Raycast extensions.** A Node host runs Raycast-compatible extensions against
a `@raycast/api` shim. Partial: enough for List, Grid, Form, Detail and Actions,
not enough for the whole store.

## AI

Ask a question from the launcher for a quick answer, or open the chat window for
a conversation with a sidebar, attachments and markdown.

Providers: OpenAI, Anthropic, Google, xAI, Groq, OpenRouter, Ollama, and the
Claude Code CLI. Keys are sealed with DPAPI rather than left in
`preferences.json`. Models are listed from each provider rather than typed in.

The model gets nine reading tools (search the index, find and read files, list a
directory, read the clipboard, list windows, read the selection, read the
screen, check how the machine is set) plus two acting ones that go through the
same action registry the launcher uses. Nothing is implemented twice.

Anything that writes a file, launches a program, injects input or changes the
machine stops at an approval card and waits for a yes. The same tools are
exposed over MCP, so an editor or another agent reaches them through the same
gate.

## Links and the command line

Something outside Sill can ask it to run an action, and both ways land in the
same action registry as a keypress:

```text
sill://run/sill.launch?target=C:\Users\me\Notes
sill run sill.file.recycle C:\Users\me\old.txt
```

The two are not trusted equally, because they are not the same thing. Anything
that can put a link on a page can write the first one, so an address may name
only the actions on a short list, may reach only the capabilities on a second
list, and **always** stops at a card naming the action and the whole target
before anything happens. `sill run` had to be typed by somebody who could have
run the program themselves, so it reaches every action and asks on the same
terms the model does: reads run, anything that changes something stops at the
card.

The command sends its request to the running Sill and exits, so what happened
appears on screen and in the activity log rather than at the prompt.

Registering `sill://` with Windows is the installer's job, and a build run from
source has not had it done.

## Building

Needs the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
for Windows, Rust stable, and Node 20 or newer.

```bash
npm install
npm run host:build
npm run tauri dev
```

For a release build:

```bash
npm run tauri build
```

`npm install` also fetches the interface font. Satoshi is not open source, and
its licence does not allow the file to be redistributed through a repository,
so it is pulled from [Fontshare](https://www.fontshare.com/fonts/satoshi) per
machine instead. Installing offline is fine: the fetch is not fatal and the
interface falls back to Segoe UI Variable until `npm run fonts` picks it up.
Building offline is not, because a packaged copy can only carry the face that
was on disk when it was built, so `npm run build` stops rather than ship
without it. See [resources/NOTICE](resources/NOTICE).

Optional at runtime: [Everything](https://www.voidtools.com/) for file search.
Without it the launcher offers to install it, and everything else still works.

## Verifying

```bash
npm run verify
```

That runs the extension host tests, `svelte-check`, the web tests, the Rust
suite, a source-shape check, a tree check, and an end-to-end gate that loads
real Raycast extensions.

Those extensions are a sparse checkout of somebody else's repository and are
not committed here, so on a fresh clone the gate has nothing to draw. One
command fetches them:

```bash
npm run extensions:fetch
```

It reads which ones the gate names rather than keeping a list of its own, and
the clone is deliberately not pinned to a commit: a gate that asks whether the
host still renders real extensions cannot answer that from a frozen copy.

`scripts/device-tests.ps1` measures what a build actually costs on a machine.
The budgets it checks against are in [docs/budgets.md](docs/budgets.md).

What Sill costs is the pitch, so it is written down where anybody can check
it: [docs/benchmark.md](docs/benchmark.md) carries every reading with the
machine, the build and the day it came from, and the command that takes the
same reading on your own machine. Nothing on that page is typed. It is
generated from what the measuring scripts wrote down, and `npm run verify`
fails if the committed copy is not what they say.

```bash
npm run benchmark
```

## Releasing

A tag builds the installers. Pushing `v0.2.0` runs
[`.github/workflows/release.yml`](.github/workflows/release.yml) on a Windows
runner and attaches the NSIS setup and the MSI to a draft release.

```bash
npm run version:set 0.2.0     # every file that holds a version number
npm run changelog -- research # the diff, to write the entry from
# write the `## 0.2.0` section of CHANGELOG.md, then commit
git tag v0.2.0 && git push origin v0.2.0
```

The version lives in `package.json` and nowhere else by hand.
`src-tauri/tauri.conf.json` reads it, which is why its `version` is a path
rather than a number; `Cargo.toml`, `host/package.json` and three lock files
carry copies that `npm run verify:source` refuses to let drift.

The changelog is written, not generated. `npm run changelog -- research`
prints where the work went, what appeared and went away, the new IPC surface
and every commit body, and the workflow refuses to build a tag whose section
is missing.

Run the workflow from the Actions tab with no tag for a rehearsal: it builds
everything and creates no release.

**Signing needs two repository secrets**, under Settings > Secrets and
variables > Actions:

| Secret | What it is |
| --- | --- |
| `WINDOWS_CERTIFICATE` | A code-signing `.pfx`, base64 encoded: `base64 -w0 certificate.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | The password that `.pfx` was exported with |

Until both exist the signing steps are skipped and the installers go out
unsigned, which SmartScreen warns about on first run. The certificate
thumbprint is derived on the runner from the certificate itself, so there is no
third secret and nothing about a certificate is committed.

## Layout

| Path | What is there |
| --- | --- |
| `src-tauri/src` | Rust. Search, actions, clipboard, snippets, dictation, AI, system control |
| `src` | Svelte. The launcher, settings, chat, and the preview routes |
| `host` | The Node process that runs Raycast-compatible extensions ([README](host/README.md)) |
| `scripts` | Build, verify and measurement scripts |
| `docs` | The user guide, the extension guide, performance budgets and the roadmap |

## Design

`src/lib/theme/theme.css` owns every size and colour. A literal pixel font size
or an inline accent colour in a component fails `npm run verify:source`. The
accent is for selection, matches, focus and affirmative state, and nothing else.

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) is how Sill is built rather than how
to build it: where a piece of work belongs, why the launcher stays quiet
when nobody is using it, and what to run before opening a pull request.
Assistants working in the repository should read
[CLAUDE.md](CLAUDE.md) as well.

## Licence

MIT. See [LICENSE](LICENSE).

Three third-party works travel with this project under their own terms, and
[resources/NOTICE](resources/NOTICE) names each: a Windows settings catalogue
from Microsoft PowerToys (MIT), the menu glyphs from Phosphor Icons (MIT), and
the Satoshi typeface from Indian Type Foundry, which is not open source and is
fetched rather than committed.
