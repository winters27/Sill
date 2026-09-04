#!/usr/bin/env bash
# Every view type Sill can draw, checked against a real extension where one
# exists and a fixture where none is cheaply available.
#
# ## Ten real extensions, and why every one of them is required
#
# Two used to be optional, skipped with a reason when the sparse checkout did
# not have them, and that was right when the checkout held three directories
# and those two were extra. It stopped being right the moment the number was
# the point: `P4-01` is done when this draws ten of the top-downloaded store
# extensions, and a gate that reaches nine and says nothing about the tenth
# cannot answer that. `npm run extensions:fetch` reads this file for the paths,
# so naming one here is what fetches it, and the count at the bottom is what
# says the answer is still ten.
#
# What they are chosen for is coverage of the things a row can draw rather than
# a ranking: icons and accessories in quantity, a real dropdown, a real detail
# pane beside a list, a real EmptyView, a real toast with a button on it, a
# form, a no-view command, and both halves of the search field.
set -e

# Every real extension this draws, so the last line can say how many.
DREW=()
drew() { DREW+=("$1"); }

SEED=$(python -c "
import json
hist = [
  {'uuid':'3f2b1c9e-7a4d-4e11-9c33-8b5a2d6f0e77','type':'v4','timestamp':1756300000000},
  {'uuid':'01J9XKQ4ZP7M2N8V3R5T6W1Y0B','type':'ulid','timestamp':1756310000000},
  {'uuid':'018f1b2c-3d4e-7f80-9a1b-2c3d4e5f6071','type':'v7','timestamp':1756320000000},
]
print('uuidHistory=' + json.dumps(json.dumps(hist)))
")

echo "--- List: uuid-generator viewHistory (real extension) ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/uuid-generator viewHistory > /dev/null
node scripts/run-extension.mjs extensions/build/uuid-generator/viewHistory.js uuid-generator \
  --seed "$SEED" --expect-root List --expect-items 3 --expect-actions 4
drew uuid-generator

# The half of the search field Sill owns. This list declares no `filtering`
# and registers no `onSearchTextChange`, which in Raycast's rules means the
# launcher narrows it, so typing part of one of the three seeded ids has to
# leave exactly that one on screen. Both halves ran through the same flattening
# the window draws from, so a filter that narrowed only what is drawn and not
# what Enter runs fails here.
echo
echo "--- List filtering: uuid-generator viewHistory narrowed by Sill ---"
node scripts/run-extension.mjs extensions/build/uuid-generator/viewHistory.js uuid-generator \
  --seed "$SEED" --type 01J9 --expect-filtering sill --expect-rows 1

echo
echo "--- Form: password-generator (real extension) ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/password-generator generate-random-password > /dev/null
node scripts/run-extension.mjs extensions/build/password-generator/generate-random-password.js password-generator \
  --expect-root Form
drew password-generator

# Everything a row draws that is not its title: icons in all three shapes an
# extension is allowed to pass, text and tag accessories, a dropdown beside the
# field, a detail pane with every kind of metadata row, and the extension's own
# words for an empty list. A fixture because no single real extension uses all
# of them at once, and the point of this line is that they hold together.
echo
echo "--- Icons, accessories, dropdown, detail, EmptyView: fixture ---"
node scripts/run-extension.mjs host/test/fixture/draws-everything.js draws-everything \
  --expect-root List --expect-icons 3 --expect-accessories 4 --expect-dropdown 2 \
  --expect-detail --expect-empty-view

# The same parts on somebody else's extensions rather than on ours, and this is
# the half that cannot be faked. Every count below is a floor, because the
# numbers belong to authors who are free to add a row tomorrow; what must not
# change is that they are drawn at all.
#
# Kill Process reads the real process table, so its counts are the lowest a
# machine can honestly have.
echo
echo "--- Rows of a real extension: kill-process ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/kill-process index > /dev/null
node scripts/run-extension.mjs extensions/build/kill-process/index.js kill-process \
  --grant fileRead,fileWrite,network,processLaunch \
  --expect-icons 5 --expect-accessories 5 --expect-dropdown 2
drew kill-process

echo
echo "--- Rows of a real extension: hacker-news ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/hacker-news frontpage > /dev/null
node scripts/run-extension.mjs extensions/build/hacker-news/frontpage.js hacker-news \
  --grant fileRead,fileWrite,network,processLaunch --expect-dropdown 10
drew hacker-news

# Twenty-five rows, each with an icon and a pair of accessories, out of data the
# extension ships rather than fetches. The heaviest real icon count here that
# does not need a network.
echo
echo "--- Icons and accessories of a real extension: pokedex natures ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/pokedex nature > /dev/null
node scripts/run-extension.mjs extensions/build/pokedex/nature.js pokedex \
  --grant fileRead,fileWrite,network,processLaunch \
  --assets extensions/raycast-src/extensions/pokedex/assets \
  --expect-root List --expect-icons 20 --expect-accessories 40
drew pokedex

# A detail pane beside a list, on somebody else's extension. Every other check
# of `List.Item.Detail` in this file is against a fixture, and a fixture agrees
# with the reader by construction: this list sets `isShowingDetail` itself and
# hangs a metadata panel off each row, which is the shape half the store uses.
echo
echo "--- Detail pane of a real extension: pokedex weakness ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/pokedex weakness > /dev/null
node scripts/run-extension.mjs extensions/build/pokedex/weakness.js pokedex \
  --grant fileRead,fileWrite,network,processLaunch \
  --assets extensions/raycast-src/extensions/pokedex/assets \
  --expect-root List --expect-detail --expect-dropdown 15

# A Grid, an EmptyView and a dropdown, on a real extension. Every other Grid
# check here is a fixture, and this one is a store extension drawing tiles: it
# has nothing to show without a network and says so in its own words, which is
# the state being drawn.
echo
echo "--- Grid, dropdown and EmptyView of a real extension: gif-search ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/gif-search search > /dev/null
node scripts/run-extension.mjs extensions/build/gif-search/search.js gif-search \
  --grant fileRead,fileWrite,network,processLaunch \
  --assets extensions/raycast-src/extensions/gif-search/assets \
  --expect-root Grid --expect-empty-view --expect-dropdown 5
drew gif-search

echo
echo "--- A real extension that needs nothing Sill lacks: search-npm ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/search-npm index > /dev/null
node scripts/run-extension.mjs extensions/build/search-npm/index.js search-npm \
  --grant fileRead,fileWrite,network,processLaunch \
  --expect-root List --expect-empty-view
drew search-npm

# A toast with a button on it, put there by somebody else's extension. It fails
# to read this machine's VS Code state and offers to copy the log, which is the
# commonest shape a toast action takes in the store: something went wrong, and
# here is the one thing to do about it.
#
# `showFailureToast` from the published `@raycast/utils` builds it, so this also
# says that the buttons survive being made by a package Sill does not own.
echo
echo "--- Toast actions of a real extension: visual-studio-code-recent-projects ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/visual-studio-code-recent-projects index > /dev/null
node scripts/run-extension.mjs extensions/build/visual-studio-code/index.js visual-studio-code \
  --grant fileRead,fileWrite,network,processLaunch \
  --expect-root List --expect-dropdown 5 --expect-toast-actions 1
drew visual-studio-code-recent-projects

# The same thing under this project's own control, and the part a real
# extension cannot be relied on to do: pressing the button. Raycast hands the
# handler the live toast, so a button that rewrites its own message proves both
# that the window can reach it and that what it was given was the toast on
# screen rather than a copy of it.
echo
echo "--- Toast actions, pressed: fixture ---"
node scripts/run-extension.mjs host/test/fixture/toasts-with-a-button.js toast-fixture --press-toast \
  --expect-root List --expect-toast-actions 2 --expect-toast-said "Trying again"

# The field that was declared by the API layer and drawn by nothing. What is
# checked here is that it reaches the window; that the window draws it is
# `verify:source`, which holds every `Form.*` component to an arm in
# `FormView.svelte` so the chain cannot grow a hole again.
echo
echo "--- Form.FilePicker: fixture ---"
node scripts/run-extension.mjs host/test/fixture/picks-a-file.js file-picker-fixture \
  --expect-root Form --expect-field Form.FilePicker=2

# The navigation stack, which is what an extension with more than one screen
# needs to work at all. Three claims, and the third is the one nothing on
# screen would show: the pushed screen must not be mounted until it is pushed,
# or a list of two hundred rows mounts two hundred detail views on its first
# frame. The fixture says so out loud from inside the component.
echo
echo "--- Action.Push and back: fixture ---"
node scripts/run-extension.mjs host/test/fixture/pushes-a-view.js push-fixture --push \
  --expect-root List --expect-pushed Detail --expect-popped List \
  --expect-lazy "second screen mounted"

echo
echo "--- Grid: fixture ---"
node scripts/run-extension.mjs host/test/fixture/grid-command.js grid-fixture --expect-root Grid

echo
echo "--- no-view: uuid-generator generateV7 (real extension) ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/uuid-generator generateV7 > /dev/null
# A no-view command renders nothing by definition, so the only thing to assert
# is that it ran and needed no API we lack. Loading it as a view instead makes
# React call the async entry point repeatedly, which is the bug this catches.
node scripts/run-extension.mjs extensions/build/uuid-generator/generateV7.js uuid-generator --no-view   | tee /tmp/sill-noview.log
grep -q "UI/showHud x1" /tmp/sill-noview.log   && echo "ok   no-view command ran exactly once"   || { echo "FAIL no-view command did not run exactly once"; exit 1; }

# A second no-view command, from somebody else, and it ends the way most of
# them do: something on the clipboard and a toast saying so. The toast is the
# only thing on screen, which is why a no-view command that shows one is worth
# a line of its own.
echo
echo "--- no-view with a toast: lorem-ipsum (real extension) ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/lorem-ipsum words > /dev/null
node scripts/run-extension.mjs extensions/build/lorem-ipsum/words.js lorem-ipsum --no-view \
  --grant fileRead,fileWrite,network,processLaunch | tee /tmp/sill-lorem.log
grep -q "Clipboard/copy x1" /tmp/sill-lorem.log && echo "ok   it put its words on the clipboard once" || { echo "FAIL nothing was copied"; exit 1; }
grep -q "on screen: \"Copied" /tmp/sill-lorem.log && echo "ok   and said so with a toast" || { echo "FAIL no toast was shown"; exit 1; }
drew lorem-ipsum

# The other half of the search field, and it needs an extension that does its
# own searching. Emoji Search sets `onSearchTextChange` and filters in memory
# with Fuse, so a word typed here reaches the extension and comes back as a
# different list. Its grants are wide because its dependency tree pulls in
# `child_process` and `http` at module load whether or not it ever uses them,
# and its assets are where its emoji data lives.
#
# Last on purpose. It is by far the heaviest case here, two thousand rows built
# in a worker on a machine that may also be compiling Rust, and ahead of a
# lighter one it left the next command short of its settle window and failed a
# step that was fine.
echo
echo "--- List onSearchTextChange: emoji (real extension) ---"
node scripts/build-extension.mjs extensions/raycast-src/extensions/emoji emoji > /dev/null
node scripts/run-extension.mjs extensions/build/emoji/emoji.js emoji \
  --grant fileRead,fileWrite,network,processLaunch \
  --assets extensions/raycast-src/extensions/emoji/assets \
  --type tada --expect-filtering extension --expect-heard
drew emoji

# Sill's own API, and the only extension here that is Sill's own.
#
# `host/test/extension/file-tools` is a real extension directory rather than a
# pre-bundled fixture, and that is the point: its `package.json` is what
# `extension_install.rs` parses into a contributed action and what
# `tests/actions.rs` reads, and its source is what runs here. One file on disk,
# read by both halves, so the manifest an author writes and the action Sill
# offers cannot drift apart.
#
# Three runs, because the three answers fail separately and the two refusals
# are the ones worth having.
echo
echo "--- @sill/api: an extension run on a file (Sill's own extension) ---"
node scripts/build-extension.mjs host/test/extension/file-tools copy-what-it-is > /dev/null
node scripts/run-extension.mjs extensions/build/file-tools/copy-what-it-is.js file-tools \
  --no-view --grant clipboardWrite --on '{"kind":"file","target":"C:/notes/todo.md"}' \
  --expect-hud "Copied the file todo.md (sill api 1)" | tee /tmp/sill-on-file.log
grep -q "Clipboard/copy x1" /tmp/sill-on-file.log \
  && echo "ok   it copied what it was run on, once" \
  || { echo "FAIL nothing was copied"; exit 1; }

# The same command with nothing to act on, which is what somebody picking it
# off the root list gets. `actionTarget()` has to be absent rather than an
# object full of empty strings, or every contributed command would act on
# whatever an empty path means.
echo
echo "--- @sill/api: the same command run on nothing ---"
node scripts/run-extension.mjs extensions/build/file-tools/copy-what-it-is.js file-tools \
  --no-view --grant clipboardWrite \
  --expect-toast-title "Nothing to copy" | tee /tmp/sill-on-nothing.log
grep -q "Clipboard/copy" /tmp/sill-on-nothing.log \
  && { echo "FAIL it copied something with nothing to act on"; exit 1; } \
  || echo "ok   it copied nothing"

# And with something to act on but nothing granted. This is the half that says
# `capabilities()` reads the same list the module gate reads: the extension is
# refusing itself, in its own words, before `Clipboard` would have thrown.
echo
echo "--- @sill/api: the same command with the permission withheld ---"
node scripts/run-extension.mjs extensions/build/file-tools/copy-what-it-is.js file-tools \
  --no-view --on '{"kind":"folder","target":"C:/notes"}' \
  --expect-toast-title "Not allowed to write the clipboard" | tee /tmp/sill-on-ungranted.log
grep -q "Clipboard/copy" /tmp/sill-on-ungranted.log \
  && { echo "FAIL an ungranted extension reached the clipboard"; exit 1; } \
  || echo "ok   it asked for nothing it had not been allowed"

# How many of somebody else's extensions this actually drew.
#
# The checklist item is a number, so the gate says the number. Counting the
# echo lines would count the fixtures too and a fixture proves nothing about
# the ecosystem, so each real one is recorded where it is drawn: adding a line
# without recording it undercounts, and recording one twice is caught by the
# sort below.
echo
WANTED=10
UNIQUE=$(printf '%s\n' "${DREW[@]}" | sort -u | wc -l)
echo "--- Real store extensions drawn: $UNIQUE ---"
printf '  %s\n' $(printf '%s\n' "${DREW[@]}" | sort -u)

if [ "$UNIQUE" -lt "$WANTED" ]; then
  echo "FAIL only $UNIQUE real extension(s) drawn, and P4-01 is done at $WANTED"
  exit 1
fi

echo "ok   $UNIQUE real store extensions rendered"
