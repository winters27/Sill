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

Every call the command makes that Sill does not answer is printed as an
explicit gap. That is the point of the runner: it tells you what your
extension actually needs.

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
else is a permission somebody agrees to when they install it and can take
back in Settings, under Extensions. Taking one back reaches a command that is
already running, not only the next launch.

Node's own modules are gated on the way in, by the first part of the module
name, so `node:fs/promises` and `fs` are the same request:

| Module | Permission it asks for |
| --- | --- |
| `fs` | Reading and changing files, both together |
| `child_process`, `worker_threads`, `inspector` | Starting other programs |
| `net`, `tls`, `dgram` | Opening network connections |
| `http`, `https`, `http2` | Making web requests |

`fetch`, `WebSocket`, `XMLHttpRequest` and `EventSource` are globals rather
than modules, and they ask for the same network permission. A refusal names
the permission and says where to grant it, rather than failing as though the
extension were broken.

### The honest limit of that

This is a permission boundary. It is **not** a container for hostile code,
and Sill's own interface says so rather than implying otherwise.

`eval`, `new Function` and `WebAssembly` need no module at all, and a dynamic
`import()` goes through a loader this does not sit on. An extension
determined to get out can. What the gate gives you is that an ordinary
extension cannot read your disk or reach the network without somebody having
agreed to it, and that revoking works.

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
| `Icon` | Every name round-trips. The window draws the names it has a mark for and a lettered tile for the rest |
| `Image` | `Image.Mask` only |
| `Keyboard` | `Keyboard.Shortcut.Common` only. A `cmd` modifier is drawn and matched as Ctrl |
| `LaunchType` | The two launch kinds, for an entry point that branches on how it was started |
| `List` | A list, with sections, rows, an empty view, a detail pane and a picker beside the search field |
| `LocalStorage` | `getItem`, `setItem`, `removeItem`, `clear`, `allItems`. Cleared when the extension is uninstalled |
| `PopToRootType` | The three values `showHUD` and `closeMainWindow` accept |
| `Toast` | `Toast.Style` only |
| `ToastHandle` | The live handle `showToast` returns. Assigning `title`, `message` or `style` updates the toast in place |
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
| `showToast` | A toast. `primaryAction` and `secondaryAction` are accepted and not yet drawn |
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
| `Form.FilePicker` | Not drawn. A form declaring one is missing that field |
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
