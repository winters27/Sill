# Developing Sill

How the repository is built, checked and released. Using Sill is in
[guide.md](guide.md) and writing an extension for it is in
[extensions.md](extensions.md).

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
without it. See [resources/NOTICE](../resources/NOTICE).

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
The budgets it checks against are in [budgets.md](budgets.md).

What Sill costs is the pitch, so it is written down where anybody can check
it: [benchmark.md](benchmark.md) carries every reading with the machine, the
build and the day it came from, and the command that takes the same reading
on your own machine. Nothing on that page is typed. It is generated from what
the measuring scripts wrote down, and `npm run verify` fails if the committed
copy is not what they say.

```bash
npm run benchmark
```

## Releasing

A tag builds the installers. Pushing `v0.2.0` runs
[`.github/workflows/release.yml`](../.github/workflows/release.yml) on a
Windows runner and attaches the NSIS setup and the MSI to a draft release.

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

## Screenshots

The pictures in the README are taken by a script rather than by hand, so they
can be retaken after the interface changes:

```powershell
pwsh -File scripts/shoot.ps1
python scripts/shoot-compose.py
```

The first drives a running Sill over a generated backdrop and writes raw
captures to `docs/media/raw/`. The second crops them, makes the logo and the
social preview, and writes `docs/media/`. Both refuse to run against a Sill
they did not start, and the raw captures are meant to be looked at before
they are committed: the shoot runs against a real install, and a real install
has real windows and a real clipboard.

## Layout

| Path | What is there |
| --- | --- |
| `src-tauri/src` | Rust. Search, actions, clipboard, snippets, dictation, AI, system control |
| `src` | Svelte. The launcher, settings, chat, and the preview routes |
| `host` | The Node process that runs Raycast-compatible extensions ([README](../host/README.md)) |
| `scripts` | Build, verify, measurement and screenshot scripts |
| `docs` | The user guide, the extension guide and the performance budgets |

## Design

`src/lib/theme/theme.css` owns every size and colour. A literal pixel font size
or an inline accent colour in a component fails `npm run verify:source`. The
accent is for selection, matches, focus and affirmative state, and nothing else.
