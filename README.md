<!-- markdownlint-disable MD013 MD033 MD041 -->
<p align="center">
  <img src="docs/media/logo.png" width="128" alt="Sill">
</p>

<h1 align="center">Sill</h1>

<p align="center">
  <strong>An open-source Windows launcher and command palette.</strong>
</p>

<p align="center">
  Launch apps, search files, manage windows and clipboard history, run scripts
  and extensions, dictate text, and use AI from one keyboard-first interface.
  Sill is built in Rust for Windows 11 and supports Raycast extensions.
</p>

<p align="center">
  <a href="https://github.com/winters27/Sill/actions/workflows/verify.yml"><img
    src="https://img.shields.io/github/actions/workflow/status/winters27/Sill/verify.yml?branch=main&label=verify"
    alt="verify"></a>
  <a href="https://github.com/winters27/Sill/releases"><img
    src="https://img.shields.io/github/v/release/winters27/Sill?include_prereleases&label=release"
    alt="release"></a>
  <a href="https://github.com/winters27/Sill/releases"><img
    src="https://img.shields.io/github/downloads/winters27/Sill/total"
    alt="downloads"></a>
  <a href="LICENSE"><img
    src="https://img.shields.io/github/license/winters27/Sill"
    alt="AGPL-3.0"></a>
  <a href="https://github.com/winters27/Sill/stargazers"><img
    src="https://img.shields.io/github/stars/winters27/Sill?style=flat"
    alt="stars"></a>
  <a href="docs/benchmark.md"><img
    src="https://img.shields.io/badge/built%20in-Rust-CE422B?logo=rust&logoColor=white"
    alt="Built in Rust"></a>
  <img src="https://img.shields.io/badge/Windows-11-0078D4?logo=windows11&logoColor=white"
    alt="Windows 11">
</p>

<p align="center">
  <a href="https://github.com/winters27/Sill/releases/latest">Download</a>
  &nbsp;·&nbsp;
  <a href="docs/guide.md">Guide</a>
  &nbsp;·&nbsp;
  <a href="docs/extensions.md">Extensions</a>
  &nbsp;·&nbsp;
  <a href="docs/mcp.md">MCP</a>
  &nbsp;·&nbsp;
  <a href="docs/benchmark.md">Performance</a>
</p>

<p align="center">
  <img src="docs/media/hero.png" width="920"
    alt="The Sill launcher over a desktop, searching for an application">
</p>

Sill is a keyboard-first app launcher for Windows 11. Press **Alt+Space** to
open it, then search applications, files, Windows settings, open windows,
snippets, quicklinks, clipboard history, emoji, and system controls. Open the
selected result with Enter or use its action menu for additional commands.

The core is written in Rust and the interface in Svelte. App search, clipboard
history, screenshots, OCR, system controls, and offline dictation run locally.
AI providers are configurable, and actions that change the system require
explicit approval.

## What Sill does

<table>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/media/files.png" alt="File search with a preview pane">
      <p><b>App, file, and system search.</b> Search Start Menu apps, Windows
      settings, files, open windows, emoji, and system controls from one
      field. Use <code>ext:md</code>, <code>size:>1mb</code>, and
      <code>date:week</code> to filter files, preview the selected file, and
      open its actions to rename, move, hash, or manage it.</p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/media/clipboard.png" alt="Clipboard history">
      <p><b>Clipboard history.</b> Search copied text, images, and files by
      content or source application. Pin entries, configure retention, and
      exclude applications. Sill does not record content marked confidential
      by password managers and filters text that looks like a secret.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/media/ask.png" alt="A question answered in the launcher">
      <p><b>Quick AI.</b> Type a question and press Tab to answer it inside
      the launcher. Escape returns to the original search, and a separate chat
      window provides conversation history, attachments, and formatted
      responses.</p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/media/ai-settings.png"
        alt="AI providers in Settings">
      <p><b>Multiple AI providers.</b> Use OpenAI, Anthropic, Google, xAI,
      Groq, OpenRouter, a local Ollama server, or the Claude Code CLI. Windows
      protects stored API keys. Models can read approved local context, while
      actions that change the machine require confirmation.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/media/chat.png" alt="The chat window with a conversation open">
      <p><b>AI chat.</b> Continue conversations in a dedicated window with a
      sidebar, attachments, streaming responses, and local context you choose
      to share. Token use and estimated cost are shown for each response.</p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/media/dictation-settings.png"
        alt="Dictation settings with statistics and the trigger key">
      <p><b>Local and cloud dictation.</b> Hold a key to record and release it
      to insert the transcription at the cursor. Run whisper.cpp locally or
      use OpenAI, Groq, or another compatible endpoint. Settings include custom
      vocabulary, usage statistics, and transcript history.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/media/calculator.png" alt="A unit conversion">
      <p><b>Calculator and conversions.</b> Evaluate expressions, convert
      units and data sizes, and work with dates directly in search. Enter
      copies the result.</p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/media/switches.png"
        alt="A Windows switch drawn as a switch">
      <p><b>Windows system controls.</b> Change dark mode, Wi-Fi, Bluetooth,
      master volume, per-app volume, and audio output without leaving the
      launcher. Stateful controls show their current setting.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/media/extension.png"
        alt="An extension drawing its list inside Sill">
      <p><b>Raycast-compatible extensions.</b> Install and run supported
      Raycast extensions without source changes. Extension views use Sill's
      native lists, grids, forms, details, actions, and keyboard controls.</p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/media/store.png" alt="The extension store inside the launcher">
      <p><b>Built-in extension store.</b> Search published extensions, review
      their install size and permissions, and install them from the launcher.
      Installed commands are available without restarting Sill.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/media/themes.png" alt="Seven themes in Settings">
      <p><b>Themes and widgets.</b> Choose from seven translucent themes and
      pin widgets for clocks, weather, CPU, memory, and temperature to the
      launcher footer. Sill also includes snippets, quicklinks, web search,
      reminders, workspaces, screenshots, OCR, and text-to-speech.</p>
    </td>
    <td width="50%" valign="top">
      <img src="docs/media/permission.png"
        alt="An extension asking to be allowed to use the network">
      <p><b>Permissions and approvals.</b> Extensions request access to files,
      the network, and other applications when first needed. Permissions are
      visible and revocable in Settings. AI actions that modify files, launch
      programs, inject input, or change the system require approval.</p>
    </td>
  </tr>
</table>

## Performance and resource use

Sill tracks launch latency, search latency, memory, CPU, network activity, and
idle work with repeatable measurements. The generated
[benchmark report](docs/benchmark.md) records the build, machine, date, budget,
and command behind each result.

## Download and install

**[Download the latest release][latest-release]**
for Windows 11. Run the installer, press **Alt+Space**, and start typing.
[Everything](https://www.voidtools.com/) is optional and improves full-disk
file search; Sill can install it when needed.

To build it yourself, with the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/), Rust
stable and Node 20 or newer:

```bash
npm install
npm run host:build
npm run tauri dev
```

`npm install` also fetches the interface font, which is not distributed in
the repository. See [Developing Sill](docs/developing.md) for verification,
release, signing, and screenshot instructions.

## Extensions and automation

Sill exposes four ways to add commands. Each uses the same action system as a
keyboard command.

**Extensions** can be installed from the built-in store or loaded from a local
folder. The [extension guide](docs/extensions.md) documents supported APIs.

**Scripts** can use PowerShell, cmd, bash, Python, or a bare executable. A
small header makes a script searchable and defines its arguments:

```powershell
# @raycast.schemaVersion 1
# @raycast.title Greeting
# @raycast.mode fullOutput
# @raycast.argument1 { "type": "text", "placeholder": "Name" }
Write-Output "Hello, $($args[0])"
```

**MCP servers** can add tools to Sill's action panel. Add a server command in
Settings and use Check to validate it. The [MCP guide](docs/mcp.md) documents
setup, lifecycle, permissions, and resource use.

**Deep links and the command line** can request actions from other programs.
Requests that require approval show the full action and target first:

```text
sill://run/sill.launch?target=C:\Users\me\Notes
sill run sill.file.recycle C:\Users\me\old.txt
```

## Documentation

- [User guide](docs/guide.md): search, actions, clipboard history, dictation,
  AI, extensions, and settings
- [Extension guide](docs/extensions.md): Raycast API compatibility and Sill's
  extension APIs
- [MCP guide](docs/mcp.md): connecting MCP servers and using Sill as an MCP
  server
- [Benchmarks](docs/benchmark.md): measured results and the budgets in
  [budgets.md](docs/budgets.md)
- [Development guide](docs/developing.md): building, testing, measuring, and
  releasing Sill
- [Changelog](CHANGELOG.md): user-visible changes by release

Press `?` with an empty search field to open the generated keyboard reference,
including customized shortcuts.

## Star history

<a href="https://star-history.com/#winters27/Sill&Date">
  <img src="https://api.star-history.com/svg?repos=winters27/Sill&type=Date"
    width="600" alt="Star history">
</a>

## Contributing

Issues and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md)
for the architecture and [docs/developing.md](docs/developing.md) for build and
verification instructions. Changes must pass `npm run verify`.

## License

Sill 0.4.0 and later are licensed under AGPL-3.0-only. Versions up to and
including 0.3.0 remain available under the MIT License. See
[LICENSE](LICENSE) and [resources/NOTICE](resources/NOTICE) for the project and
third-party license terms.

Copyright (c) 2026 Brandon Winters.

[latest-release]: https://github.com/winters27/Sill/releases/latest
