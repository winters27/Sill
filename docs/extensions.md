# Writing an extension for Sill

Sill runs extensions written for the Raycast API. An extension is a small
Node program that draws a list, a grid, a form or a page inside the launcher
and acts on whatever the reader picks. If you have written one before, the
same code runs here; if you have not, the API is documented at
[developers.raycast.com](https://developers.raycast.com/).

Coverage is partial and this page says exactly how partial, name by name. It
is checked against the code by `npm run verify:source`, so a name that
appears here and nowhere in the host, or the other way round, is a failure
rather than a page nobody re-read.

## What Sill does with an extension

Each command runs in its own Node worker thread. The worker renders with
React into a tiny reconciler, and what crosses to the window is a stream of
patches rather than HTML: create this node, set these props, put it there.
The window applies them and draws the result with the launcher's own list,
grid, form and detail views, so an extension looks like the rest of Sill
rather than like a page inside it.

That has two consequences worth knowing before you start.

- **You are not drawing.** There is no DOM in the worker and no way to reach
  the window's markup. Components describe rows; the launcher decides what a
  row looks like.
- **Callbacks stay in the worker.** An `onAction` never crosses the wire.
  What crosses is an id, and pressing the row sends that id back.

## Running one without building Sill

Two scripts, both plain Node, and neither needs the application running.

```bash
node scripts/build-extension.mjs <extension-dir> [command-name]
node scripts/run-extension.mjs extensions/build/<name>/<command>.js <name>
```

The first bundles one command with esbuild, leaving `@raycast/api` and
`react` external because the host supplies both. The second loads the bundle
against the real host, prints every API call the command makes, and applies
the op stream with the window's own view tree so the rendered structure can
be read as text.

The flags that matter most while writing:

| Flag | What it does |
| --- | --- |
| `--no-view` | Load the command as a `no-view` entry point rather than as a component |
| `--grant fileRead,fileWrite,network,processLaunch` | Hand the worker permissions, the way accepting an install would |
| `--assets <dir>` | Point `environment.assetsPath` at the extension's own assets |
| `--seed key=json` | Pre-fill `LocalStorage`, so a history view has history |
| `--type <text>` | Type into the search field and show what came back |
| `--on '{"kind":"file","target":"C:/notes/todo.md"}'` | Run it as though somebody had picked it out of a file's action panel |

Every call the command makes that Sill does not answer is printed as an
explicit gap. That is the point of the runner: it tells you what your
extension actually needs.

### Watch mode

The loop is one command. It bundles, runs, prints what came back, and does the
whole thing again every time you save.

```bash
node scripts/build-extension.mjs <extension-dir> <command-name> --watch
```

Anything after a bare `--` goes to the run rather than to the build, which is
where the permissions and the thing to act on go:

```bash
node scripts/build-extension.mjs my-extension search --watch -- --grant network
```

One run at a time. A save that lands while the previous run is still going
ends it, because a watch that queues runs prints a stale answer after the edit
that made it wrong. Ctrl+C stops both.

To run the same end-to-end checks the project runs:

```bash
npm run extensions:fetch
npm run gate:views
```

The first fetches the real extensions the gate draws against, which are a
sparse checkout of somebody else's repository and are not committed here. The
second builds and renders them, plus fixtures for the parts no single real
extension exercises at once.

## The manifest

`package.json` is read at install. Two things about it are stricter here than
you may expect, and both fail loudly at install rather than quietly later.

- **A command's `mode` must be `view` or `no-view`.** A `menu-bar` command is
  a status item beside the clock and a launcher has nowhere to put one, so it
  is refused by name. A mode this build has never heard of is refused as a
  fact about Sill rather than about your extension.
- **Preferences are read and honoured.** `type`, `required` and the default
  are all used, a password preference is sealed on disk and only ever opened
  for the worker, and `getPreferenceValues()` returns what somebody entered.

Declared command `arguments` are read and recorded, and every one of them
gets a key so destructuring does not yield `undefined`. Nothing collects
them from the reader yet, so treat them as always empty for now.

`environment.assetsPath` and `environment.supportPath` both point inside the
installed extension's own folder.

## What an extension is allowed to reach

An extension starts with nothing beyond drawing in the window. Everything
else is a permission somebody agrees to, either on the screen that installs
it or on a card the first time the extension reaches for it, and can take
back in Settings, under Extensions. Taking one back reaches a command that is
already running, not only the next launch.

The card is the same one Sill's own AI is answered on, and it is raised at
the moment it matters: an extension that requires `http` without holding the
network is stopped inside that `require`, the card names the permission, and
the extension carries on or is refused depending on the answer. A yes is
remembered; a no is not, so the same command asks again next time rather
than staying broken with no way back but Settings.

**Node's own modules are an allowlist, not a blocklist.** A built-in is handed
over if it is named free below, needs its permission if it is named gated, and
is **refused otherwise, whatever anybody has granted**. That way round on
purpose: a list of dangerous modules is a list every Node release adds a hole
to, and this one had five holes in it before it was inverted.

Gated by the first part of the name, so `node:fs/promises` and `fs` are the
same request:

<!-- coverage:gated -->

| Module | What it asks to be allowed to do |
| --- | --- |
| `child_process` | start other programs |
| `cluster` | start other programs |
| `dgram` | open network connections |
| `dns` | open network connections |
| `fs` | read and change files directly |
| `http` | make web requests |
| `http2` | make web requests |
| `https` | make web requests |
| `inspector` | start other programs |
| `net` | open network connections |
| `tls` | open network connections |
| `worker_threads` | start other programs |

<!-- /coverage:gated -->

These come free, because none of them reaches past the worker it runs in:

<!-- coverage:free -->

`assert`, `async_hooks`, `buffer`, `console`, `constants`, `crypto`,
`diagnostics_channel`, `domain`, `events`, `module`, `os`, `path`,
`perf_hooks`, `process`, `punycode`, `querystring`, `readline`, `stream`,
`string_decoder`, `sys`, `timers`, `tty`, `url`, `util`, `zlib`.

<!-- /coverage:free -->

Everything else Node ships is refused, including `node:vm`, `node:v8`,
`node:sqlite`, `node:wasi` and whatever the next release adds. No permission
turns those on. If an extension genuinely needs one, that is a gap in Sill
rather than a setting.

Every route to a built-in meets the same answer: `require`, `Module._load`,
`module.createRequire`, `process.getBuiltinModule`, `process.binding`, and a
dynamic `import()` along with anything that import pulls in behind it.
`process.dlopen`, which loads a native addon, is refused for everybody, because
native code runs outside every permission there is and there is nothing to
grant.

`fetch`, `WebSocket`, `XMLHttpRequest` and `EventSource` are globals rather
than modules, and they ask for the same network permission, on the same card. So do
`process.kill`, which signals other programs, and `process.report.writeReport`,
which writes a file wherever it is pointed. A refusal names the permission and
says where to grant it, rather than failing as though the extension were
broken.

Extensions need **Node 22.15 or newer**. That is the release
`module.registerHooks` arrived in, and without it a dynamic `import()` walks
past all of the above, so a worker on an older Node refuses to run a command
rather than running it with a hole in it.

### The honest limit of that

This is a permission boundary. It is **not** a container for hostile code,
and Sill's own interface says so rather than implying otherwise.

What it holds: every route to a built-in listed above arrives at the same
answer, so an extension cannot reach a file, a socket, another program, or a
built-in nobody put on the list without somebody having agreed to it, and
taking the agreement back reaches a command already on screen rather than only
the next launch. That is a boundary an ordinary extension meets. It is not a
guarantee against code that goes looking for a way round rather than asking.

What it does not: a permission is granted whole, so `fileRead` is every file
you can open rather than the extension's own. A dependency shares the worker
and does whatever the extension does. Starting another program puts what that
program does outside all of this. `process.env` is readable, and Sill's own
environment is in it.

`eval`, `new Function` and `WebAssembly` were listed here as ways out, and
they are not, which is worth saying plainly because this page said otherwise
until now. Generated code cannot see `require` at all, because `require` is a
parameter of the module scope rather than a global; a direct `eval` sees the
gated one; and every global route generated code does have is wrapped. What
generated code defeats is the **description** rather than the gate: the store
reads an extension's source to say what it appears to use, and it cannot see a
module name assembled at runtime. An extension that assembles one is refused at
runtime and says which permission it wanted.

## Contributing an action to Sill's own rows

Everything above is Raycast's API, and an extension written to it is a set of
commands somebody opens. This part is Sill's own, and it is the other
direction: a command that appears in the action panel of a file, a folder, a
window or anything else Sill can act on, so somebody who has already found the
thing can do your thing to it without leaving the row.

Declare it on the command, under a `sill` key:

```json
{
  "name": "copy-what-it-is",
  "title": "Copy What It Is",
  "mode": "no-view",
  "sill": { "actionOn": ["file", "folder"] }
}
```

Raycast ignores keys it does not know, so a manifest carrying this still builds
and installs there. Nothing is forked to work here.

Four things about that are decided by Sill rather than by you, and all four
are refusals somebody could otherwise be surprised by.

- **The mode must be `no-view`.** An action is a verb somebody picks out of a
  panel; a view is a screen they open. There is nowhere in a panel to draw a
  screen, so a `view` command declaring `actionOn` is refused at install with
  that sentence.
- **The kinds must be kinds Sill has.** They are the names in the table below.
  One this build has never heard of is refused at install, by name, with the
  full list; the alternative is an action that is silently never offered and
  no way to find out why.
- **Enter is not yours.** Enter on a file opens it, and that does not change.
  Your action is drawn in the panel, below everything Sill itself offers.
- **The id is Sill's**, `extension.<your-extension>.<your-command>`. Two
  extensions with a command of the same name get different ids, and nobody can
  claim one of Sill's own.

Nothing here asks for a permission. Running your action starts your command,
in a worker, with exactly the permissions your extension already had: it can
do what you may do and no more, and somebody revoking one in Settings reaches
it the same way it reaches everything else.

An action cannot be put on a schedule either. Starting a program is one of the
things `P8-02`'s triggers refuse outright, because a trigger fires with nobody
there to be asked, and running somebody else's code unattended is squarely
that.

## The Sill API

`@sill/api` is Sill's own module, beside the Raycast one. It is small on
purpose: three things Sill already knows and your extension had no way to see.
Importing it is you saying out loud that this command will not run anywhere
else, so import it only in the commands that need it.

```ts
import { actionTarget, holds } from "@sill/api";

export default async function main() {
  const on = actionTarget();
  if (!on) return;                       // opened from the root list
  if (!holds("clipboardWrite")) return;  // say so in your own words
  // ...
}
```

<!-- coverage:sill -->

| Name | What it is |
| --- | --- |
| `actionTarget` | What this command was run on, or `undefined`. Only ever a value when the command was reached through an action panel, so it is also how a command tells the two apart |
| `capabilities` | Everything this extension has been allowed to reach, read at the moment you ask rather than captured at launch |
| `holds` | Whether it holds one particular capability. A reading, not a request: the gate still refuses what you were not granted, whether or not you checked |
| `apiVersion` | The version of this API the host implements. One number, because there is one publisher and the only useful question is whether what you were written against is here |

<!-- /coverage:sill -->

Everything above is a plain function, never a method, and that is deliberate:
the published `@raycast/utils` hands a class method to `useSyncExternalStore`
unbound, and every extension using `useCachedState` died on its first render
until Sill worked around it. Destructure these and pass them anywhere.

### What an object is

`actionTarget()` gives you four strings and a kind.

| Field | What it is |
| --- | --- |
| `kind` | What sort of thing it is, from the table below |
| `id` | Its stable identity, the string Sill ranks and remembers it by |
| `target` | The part to act on: a path for a file, a handle for a window, the text itself for a clipboard row |
| `title` | What to call it in front of somebody |
| `mode` | How Sill found it, when it came out of the index. Two modes can share a kind, so read `kind` unless you need the difference |

The kinds, which are also the words `actionOn` takes: `application`, `file`,
`folder`, `extensionCommand`, `systemSetting`, `setting`, `builtin`,
`systemControl`, `snippet`, `quicklink`, `script`, `answer`, `clipboardEntry`,
`text`, `emoji`, `window`, `browserTab`, `search`, `url`, `audioSession`,
`nowPlaying`, `process`, `screenControl`, `workspace`, `conversation`,
`storeListing`, `terminalProfile`.

The names are checked against Rust's own by `npm run verify:source`, in both
directions, for the reason the coverage tables exist: a kind spelled two ways
is an action that is never offered and nothing anywhere saying so.

### The limit of `actionOn`

It is per **kind**, not per object. You can say "every file" and you cannot yet
say "every `.png`", because Sill builds the panel once for the kind rather than
once for the thing selected. Filter inside your command and say why when you
decline.

## What your extension costs, and where somebody sees it

Settings, under Extensions, says how long each extension took to open and how
much memory it was holding. It is the screen somebody goes to when their
launcher got slower after they installed four things, and the line at the top
of it names one extension.

Two numbers for opening, because they are two different things. **Cold** is
with the Node process that runs extensions having to start first, which is
about half a second and is almost entirely Node rather than anything you
wrote. **Warm** is with it already up, which is what somebody gets for every
open after the first, and it is the one your code decides. Across the five
real extensions Sill tests against, warm openings run from 36 ms to 114 ms.

Memory is read from inside your worker, live while a command is loaded and
once more as it closes. The same five run from 11 MB to 63 MB, and about
11 MB of that is what an empty worker costs before your bundle is loaded at
all.

Two limits sit behind that, and neither is a number to design against.

- **512 MB of heap.** A command that goes past it is stopped where it stands
  and whatever the person was doing in it is lost. Nothing warns first, so if
  you are keeping every row you have ever built, the panel is where that shows
  up long before this does.
- **A whole processor core for thirty seconds** without ever yielding. That is
  a loop rather than work, and the command is stopped with a message saying so.

You can see all of it for one command without building Sill:

```bash
node scripts/run-extension.mjs extensions/build/<name>/<command>.js <name> --measure
```

## What happens when Sill does not cover something

The module an extension receives for `@raycast/api` and `@raycast/utils` is a
proxy. Anything present is handed over. Anything absent throws its own name
at the moment it is touched:

```text
sill: "getFrontmostApplication" is not implemented yet. It is part of the
Raycast API surface Sill has not covered. Please report which extension
needed it.
```

That is deliberate. An absent export reading as `undefined` surfaces three
frames later as "undefined is not a function" from inside a bundle, which
looks like the extension being broken rather than like Sill not covering
something.

A handful of names throw a reason instead, because they are not gaps waiting
to be filled. They are listed below.

## The API, name by name

Everything in this table is answered. Everything not in it throws the message
above.

<!-- coverage:answered -->

| Name | What Sill does with it |
| --- | --- |
| `Action` | The base action. Any child of an `ActionPanel` whose name begins with `Action` becomes a row in the panel |
| `ActionPanel` | The actions a row or a view offers. Enter runs the first one; declaration order is the order drawn |
| `Alert` | `Alert.ActionStyle` only, the three style names |
| `Cache` | Raycast's synchronous cache, over storage that is not synchronous |
| `Clipboard` | `copy`, `paste`, `clear`, `read`, `readText`. Each asks for a clipboard permission |
| `Color` | The nine colour names. Seven map onto Sill's palette; `Magenta` and `Purple` have no colour of their own and draw in the ordinary text colour rather than in a nearby hue |
| `Detail` | A markdown page, with an optional metadata panel beside it |
| `Form` | A form. The fields it supports are in the component table below |
| `Grid` | A grid of tiles, with sections, an empty view and a picker beside the search field |
| `Icon` | Every name round-trips. Which names have a mark is decided in `src-tauri/src/exthost/icons.rs` and nowhere else; anything with no mark gets a lettered tile |
| `Image` | `Image.Mask` only |
| `Keyboard` | `Keyboard.Shortcut.Common` only. A `cmd` modifier is drawn and matched as Ctrl |
| `LaunchType` | The two launch kinds, for an entry point that branches on how it was started |
| `List` | A list, with sections, rows, an empty view, a detail pane and a picker beside the search field |
| `LocalStorage` | `getItem`, `setItem`, `removeItem`, `clear`, `allItems`. Cleared when the extension is uninstalled |
| `PopToRootType` | The three values `showHUD` and `closeMainWindow` accept |
| `Toast` | `Toast.Style` only |
| `ToastHandle` | The live handle `showToast` returns. Assigning `title`, `message` or `style` updates the toast in place, and a button's callback is handed this same handle |
| `closeMainWindow` | Puts the launcher away. Asks to be allowed to dismiss the launcher, which is a permission of its own |
| `confirmAlert` | A dialog with a primary and a dismiss button, both titled by the extension |
| `environment` | Read from the launch payload, including `assetsPath` and `supportPath` |
| `getApplications` | The installed applications Sill knows about. Raycast's optional path argument is accepted and ignored, so the answer is always the whole list |
| `getDefaultApplication` | What Windows would open a given file with |
| `getPreferenceValues` | What somebody entered for the preferences the manifest declares |
| `getSelectedText` | The selection in whichever program is in front. Asks to be allowed to read a selection |
| `open` | Opens a file or an address, optionally with a named application |
| `popToRoot` | Leaves the command and returns to the root search |
| `showHUD` | A short message across the launcher |
| `showToast` | A toast. `primaryAction` and `secondaryAction` are drawn as buttons beside it, with their chords, and pressing one runs the callback through the same activation an action panel row uses |
| `useNavigation` | `push` and `pop` against the worker's own view stack |
| `FormValidation` | `FormValidation.Required`, for `useForm` |
| `createDeeplink` | Builds a `raycast://` link. Sill does not answer that scheme, so this is a string to hand somebody, not a link that works here |
| `getAvatarIcon` | A coloured circle with initials, built as a data URI rather than fetched |
| `getFavicon` | A site's icon, through Google's favicon service |
| `getProgressIcon` | A ring filled to a fraction, built as a data URI |
| `runPowerShellScript` | Runs PowerShell. Asks to be allowed to start programs, at the moment it is called |
| `showFailureToast` | A failure toast built from a thrown value |
| `useCachedPromise` | `usePromise` with the last answer standing in until the new one lands |
| `useCachedState` | State that survives a re-render and a re-launch |
| `useFetch` | A fetch as a hook, JSON unless told otherwise. Asks for the network permission |
| `useForm` | Values, errors, validation and `itemProps`. `focus()` does nothing, because focus belongs to the window |
| `useFrecencySorting` | Sorts a list by how often each item has been picked |
| `useLocalStorage` | One `LocalStorage` key as a hook |
| `usePromise` | A promise as a hook, with a stale-result guard so an older call cannot land last |
| `withCache` | Remembers what a function returned. A rejection is never remembered |

<!-- /coverage:answered -->

### Names that throw a reason

These exist so the failure explains itself. None of them is waiting to be
filled in by covering more of the API.

<!-- coverage:refused -->

| Name | Why not |
| --- | --- |
| `executeSQL` | Sill does not open SQLite databases on an extension's behalf |
| `runAppleScript` | AppleScript is macOS only, so an extension built around it needs a different path for whatever it was doing |
| `useAI` | Sill's AI is reached through its own commands rather than from inside an extension |
| `useExec` | Running an arbitrary command line is not offered to extensions. `runPowerShellScript` is, and asks permission |
| `useSQL` | Sill does not open SQLite databases on an extension's behalf |
| `useStreamJSON` | Streaming a file from disk is not implemented here yet |

<!-- /coverage:refused -->

## The components, tag by tag

Every tag below reaches the window. What each column says is what the window
does with it when it gets there.

<!-- coverage:tags -->

| Tag | What the window does with it |
| --- | --- |
| `Action` | A row in the action panel. Runs its `onAction` |
| `Action.CopyToClipboard` | Sill copies the `content` itself, so no callback is needed |
| `Action.CreateQuicklink` | A row that runs its `onAction`. Sill does not create the quicklink for it |
| `Action.CreateSnippet` | A row that runs its `onAction`. Sill does not create the snippet for it |
| `Action.Open` | Sill opens the `target` itself, through the same scheme check every other opened address goes through |
| `Action.OpenInBrowser` | Sill opens the `url` itself |
| `Action.OpenWith` | A row that runs its `onAction`. Sill does not offer the application picker for it |
| `Action.Paste` | Sill writes the `content` and types it into whatever is in front |
| `Action.PickDate` | A row that runs its `onAction`. Sill does not draw a date picker for it |
| `Action.Push` | Renders its `target` as a new screen, and only then. The target is never mounted while the row is merely on offer |
| `Action.ShowInFinder` | A row that runs its `onAction`. Sill does not reveal the file for it |
| `Action.SubmitForm` | A row in a form's panel. Runs `onSubmit` with the form's values |
| `Action.ToggleQuickLook` | A row that runs its `onAction`. Sill has no quick look to toggle |
| `Action.Trash` | A row that runs its `onAction`. Sill does not delete the file for it |
| `ActionPanel` | The panel itself |
| `ActionPanel.Section` | A heading over the actions declared inside it |
| `ActionPanel.Submenu` | Read as a section rather than as a submenu, so its actions are in the panel under its title |
| `Detail` | A markdown page. The text may be the `markdown` prop or the children |
| `Detail.Metadata` | The panel beside the page |
| `Detail.Metadata.Label` | A labelled value, with an optional icon |
| `Detail.Metadata.Link` | A labelled link |
| `Detail.Metadata.Separator` | A rule between groups |
| `Detail.Metadata.TagList` | A labelled row of pills |
| `Detail.Metadata.TagList.Item` | One pill, with its own colour and icon |
| `Form` | The form |
| `Form.Checkbox` | A switch |
| `Form.DatePicker` | A date field |
| `Form.Description` | Text between fields |
| `Form.Dropdown` | A picker |
| `Form.Dropdown.Item` | One option |
| `Form.Dropdown.Section` | Its options appear; the section's own title is not drawn |
| `Form.FilePicker` | A button that opens Windows' own dialog, and the names of whatever was chosen. Nothing is read: the field's value is paths, and opening one still needs the filesystem permission |
| `Form.LinkAccessory` | Not drawn |
| `Form.PasswordField` | A field that hides what is typed |
| `Form.Separator` | A rule between fields |
| `Form.TagPicker` | A set of values, several selectable at once |
| `Form.TagPicker.Item` | One of them |
| `Form.TextArea` | A multi-line field |
| `Form.TextField` | A single-line field |
| `Grid` | The grid |
| `Grid.Dropdown` | The picker beside the search field, when passed as `searchBarAccessory` |
| `Grid.Dropdown.Item` | One option |
| `Grid.Dropdown.Section` | Groups options under a heading |
| `Grid.EmptyView` | The extension's own words for an empty grid |
| `Grid.Item` | One tile |
| `Grid.Section` | A heading over the tiles inside it |
| `List` | The list |
| `List.Dropdown` | The picker beside the search field, when passed as `searchBarAccessory` |
| `List.Dropdown.Item` | One option |
| `List.Dropdown.Section` | Groups options under a heading |
| `List.EmptyView` | The extension's own words for an empty list |
| `List.Item` | One row. Title and subtitle sit side by side, the icon leads, the accessories are pushed right, and `keywords` are matched rather than drawn |
| `List.Item.Detail` | The pane beside the rows, shown when the list sets `isShowingDetail` |
| `List.Item.Detail.Metadata` | The metadata panel inside that pane |
| `List.Item.Detail.Metadata.Label` | A labelled value, with an optional icon |
| `List.Item.Detail.Metadata.Link` | A labelled link |
| `List.Item.Detail.Metadata.Separator` | A rule between groups |
| `List.Item.Detail.Metadata.TagList` | A labelled row of pills |
| `List.Item.Detail.Metadata.TagList.Item` | One pill |
| `List.Section` | A heading over the rows inside it |

<!-- /coverage:tags -->

### The search field

A list or a grid gets the field above it, and four props decide what happens
when somebody types.

- **`filtering`** hands the narrowing to Sill. A list that declares neither
  `filtering` nor `onSearchTextChange` is narrowed by Sill anyway, which is
  Raycast's own default.
- **`onSearchTextChange`** relays what was typed to the extension, whenever
  the handler is registered. An extension that both fetches and wants to be
  narrowed gets both.
- **`throttle`** is 200 ms, on the leading edge with a trailing call, which
  is above a typist's own rhythm so the prop does not lie.
- **`isLoading`** draws the loading state.

### Icons

An `icon` prop is allowed to be four different things and all four are read:
a name, a URL or data URI, `{ source, tintColor, mask }`, and
`{ light, dark }`. A character or an emoji is printed as itself.

One shape does not draw: a path relative to the extension's own assets. The
window has no idea where an installed extension lives on disk, so those rows
get the lettered tile rather than a broken image.

A name is looked up in one table, `src-tauri/src/exthost/icons.rs`. Several
names are one picture there, because they are: `Icon.Cog` and `Icon.Gear` are
the same drawing, and so are `Icon.Warning` and `Icon.ExclamationMark`. A name
that table does not carry draws the first letter of the name on a tile, which
is what the root list already does for an application whose icon Windows will
not produce.

### Choosing a file

`Form.FilePicker` draws a button. Pressing it opens the dialog Windows draws,
somebody chooses in it, and the field's value becomes the paths they chose.

**That is the whole of what a picker gives an extension**, and it is worth
saying plainly, because a file picker looks like a way round the filesystem
permission and is not. Nothing about the field reads a file, lists a folder or
asks for a size. Choosing a folder yields the folder's own path and not its
contents. Opening any of those paths afterwards is `fs` inside the worker,
which is refused without the filesystem permission exactly as it would be for
a path the extension typed out itself.

So the field costs no permission to draw, and the dialog runs in Rust rather
than in the window: the launcher window is deliberately not allowed to open
one, because it is the window an extension draws into.

## Keeping this page honest

The two coverage tables are guarded, not maintained. `npm run verify:source`
reads the host's own exports and component declarations and compares them to
the rows here. Adding an API without a row fails; leaving a row behind after
removing an API fails; listing something as answered when it throws its own
reason fails.

The reason is not tidiness. This project has lost several sessions to a
hand-kept list quietly disagreeing with the thing it describes, and a
coverage table is the worst place for that to happen, because the person
reading it has no way to tell.

The prose in the right-hand column is not guarded and cannot be. If you
change what a component draws, change the sentence too.

### What the tables cannot tell you

They describe Sill against itself: every name here is one the host answers or
the window draws. Nothing in them is compared to what `@raycast/api` actually
declares, so a component or a prop Sill has never heard of is absent from both
the code and the table, and absent twice looks like agreement.

Two reports answer that, and neither is part of `npm run verify`: both need
the sparse checkout the view gate draws, and one needs a network.

```bash
npm run extensions:fetch
node scripts/audit-api.mjs --props
```

`audit-api.mjs` reads Raycast's own type declarations out of the checkout and
reports, component by component, which of its props are read anywhere in the
window or the host, and how much of each enum has somewhere to land. A prop it
calls missing is missing. **A prop it calls read may still be read wrongly**:
the test is that the name appears, which over-reports on purpose, because the
alternative is a second implementation of the renderer's own logic that would
agree with itself and with nothing on screen.

```bash
node scripts/audit-extensions.mjs
```

That is the other half: what real extensions reach for, ranked by how many of
them want each one. It covers APIs the host does not answer, permissions
refused at load, icon names drawn as letters, and assets the window cannot
resolve.

**"APIs the host does not answer" is measured against the runner, not against
Rust.** `run-extension.mjs` has no Rust behind it and serves the API from its
own table, so a method Rust answers and that table does not is reported as a
hole in Sill. It said `UI/getSelectedText`, which had been implemented for a
week, along with `Storage/clear` and `Application/getDefault`. A report that
invents missing features is worse than no report, because the next reader
builds something that is already there.

`the_view_gate_stub_answers_everything_rust_answers` in
`src-tauri/tests/exthost.rs` is what stops that. It dispatches every method the
host can call against the real implementation and fails if Rust answers one the
runner's table has no case for. Its sibling guards the opposite direction,
which is the one that shipped a bug.

**An icon name with no drawing is the one gap nothing else can see.** It is
not a failed call and raises nothing: it falls back to a letter tile and the
extension runs perfectly, so an extension can be reported as supported by
every other measure while every row of it draws a letter.

That is why the icon table is complete rather than sampled. It held the 106
names the store's extensions were measured asking for, which is a real answer
to a real question and the wrong place to stop: an extension is written
against the whole vocabulary, so the next one installed brings back the
letters. All 469 names Raycast publishes now have a mark, and
`src-tauri/src/exthost/icons.rs` is the list. A name still reaching the letter
tile is a relative path into an extension's own assets, which is a different
gap with a different fix.
