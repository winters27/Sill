#!/usr/bin/env bash
# Every view type Sill can draw, checked against a real extension where one
# exists and a fixture where none is cheaply available.
set -e

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

# The same parts on somebody else's extension rather than on ours.
#
# Skipped with a reason rather than failed when the sparse checkout does not
# have them, which is the same treatment every other non-hermetic probe in this
# project gets: a machine without the working area should not be told its code
# is broken. Kill Process reads the real process table, so the counts are lower
# bounds rather than numbers.
for real in kill-process:index:"--expect-icons 5 --expect-accessories 5 --expect-dropdown 2" \
            hacker-news:frontpage:"--expect-dropdown 10"; do
  name=${real%%:*}
  rest=${real#*:}
  command=${rest%%:*}
  expect=${rest#*:}

  if [ ! -d "extensions/raycast-src/extensions/$name/node_modules" ]; then
    echo
    echo "--- skipped $name: not in the sparse checkout, or its dependencies are not installed ---"
    continue
  fi

  echo
  echo "--- Rows of a real extension: $name ---"
  node scripts/build-extension.mjs "extensions/raycast-src/extensions/$name" "$command" > /dev/null
  node scripts/run-extension.mjs "extensions/build/$name/$command.js" "$name" \
    --grant fileRead,fileWrite,network,processLaunch $expect
done

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
