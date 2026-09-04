# MCP servers in Sill

Sill speaks MCP in both directions.

- **As a server.** Sill's own tools are available to any MCP client on this
  machine, which is what lets `claude -p` and an editor look at the index, the
  clipboard, the windows and the screen. Nothing on this page is about that
  half; `src-tauri/src/ai/mcp/` documents it.
- **As a client**, which is this page. A server you already have contributes
  its tools to Sill's action panel, so a file found by search offers an action
  the server added, beside everything Sill itself can do to a file.

## Setting one up

Settings, under MCP Servers. Give it a name, paste the command line you would
otherwise put in an editor's config, and press **Check**: Sill starts it, asks
what it has, and closes it again. What comes back fills the tool picker, so you
do not have to read the server's source to find out what it is called.

Then add an action. Three things say what it is:

| Field | What it is |
| --- | --- |
| **Tool** | The tool to call, from what Check found |
| **Shown as** | What the action panel shows. The tool's own name if left blank |
| **Passed as** | The tool argument that receives the thing you ran it on |
| **Kinds** | What it can be run on: a file, a folder, some text, a window |

**Passed as** is the only one that is not obvious, and it is the whole binding.
An action runs on something, and the server has to be told which of its
arguments that something goes in. For the reference filesystem server reading a
file, that is `path`. Leave it blank for a tool that takes nothing.

The action is then in the panel under `mcp.<server>.<tool>`, which is also the
name a keyboard shortcut refers to in the Shortcuts panel.

### What one costs

**A call is a process start.** Sill starts the program, does the handshake,
calls the one tool, reads the answer and closes it, every time. Nothing is kept
open between actions, so a server you have not used today has not been started
today, and a server you set up and forgot costs a few lines in a settings file.

Measured on one Windows machine against the reference filesystem server, from
the person pressing the row to the answer arriving:

| How it is started | What one call costs |
| --- | --- |
| `node ...\dist\index.js` | 0.8 to 2.5 seconds |
| `npx -y @modelcontextprotocol/server-filesystem ...` | 5 to 6 seconds |

The gap is `npx` resolving a package name on every start, so **name the entry
point directly if you have it**. Both are the same design and the difference is
entirely somebody else's program starting.

If it does not answer at all, Sill stops waiting, closes it, and says which
server it was: ten seconds for the handshake, a minute for the call itself.

## What it cannot do

Four refusals, and none of them are about MCP specifically. They fall out of
the one thing an MCP action declares about itself, which is that **it runs an
arbitrary program on this machine**. That is what starting somebody else's
command line is, so that is what it says, and every rule Sill already had about
running a program then applies to it unchanged.

- **Enter is not yours.** Enter on a file opens it. An MCP action is drawn in
  the panel, below everything Sill itself offers and below anything an
  extension contributed.
- **A scheduled trigger cannot use one.** Triggers refuse everything that would
  stop and ask, and a trigger fires with nobody there to ask.
- **Sill's own AI has to prove somebody is at the machine.** Running one from
  the model reaches Windows Hello, the same gate a shell command reaches, and
  on a machine with no enrolled Hello credential it reaches the approval card
  plus the reason. A model chaining one server's answer into another server's
  tool is the case that gate exists for.
- **A `sill://` link cannot run one silently.** It stops at the approval card.

Running one yourself, out of the action panel, just runs it. You configured the
server, its name is on the row, and what happened is written to Activity.

## What is deliberately missing

- **No environment variables.** A server wanting an API key would put it in the
  settings file in the clear, where every backup takes a copy. Until that goes
  through the same sealing the AI providers use, a server needing a secret is
  started through a wrapper that holds it.
- **No resources, prompts, sampling or roots.** Sill asks a server for its
  tools and calls one. The rest of the protocol answers questions a launcher's
  action panel does not ask.
- **The kinds are per kind, not per object.** You can say "every file" and not
  yet "every `.png`", which is the same limit `actionOn` has for extensions and
  for the same reason: the panel is built once for the kind rather than once
  for the thing selected.
