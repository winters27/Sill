/**
 * The launcher's own action-panel entries.
 *
 * ## Why these are here rather than in the window
 *
 * Two of the launcher's surfaces put entries in the panel that no extension
 * declared: the clipboard, which offers merging, collecting, pasting as plain
 * text and everything the registry can do to a piece of text, and every
 * surface with row actions, which offers what the registry says can be done to
 * the row plus the two names an alias is. Both were `$derived` bodies in
 * `+page.svelte`, where nothing can call them and no test can reach them, and
 * both are functions over values: given what is picked and what the registry
 * answered, the list of entries is fixed.
 *
 * The window still decides which of them applies, because that is a question
 * about the mode, and running one is still the window's `runAction`. This is
 * only what the panel says.
 *
 * ## The one rule that has been broken before
 *
 * A registry action reaches the panel carrying **its own shortcut**, resolved
 * in Rust. The window used to write the clipboard's four chords by hand and
 * pass `shortcut: undefined` for everything else, so an action that arrived
 * any other way had no key at all and the panel drew nothing beside it.
 * `verify:source` checks every place an `ActionInfo` becomes an entry for
 * exactly this, because dropping the field type-checks.
 */
import type { ActionEntry } from "$lib/exthost/actions";
import type { ActionInfo, RankedCommand } from "$lib/exthost/commands";

/** The collection open in the clipboard history, when one is. */
export interface OpenCollection {
  id: number;
  name: string;
}

export interface ClipboardPanel {
  /** Which entries are picked for merging. */
  picked: number[];
  /** Whether the highlighted entry kept a formatted version. */
  rich: boolean;
  openCollection: OpenCollection | null;
  /** What the registry says can be done to a piece of text. */
  registry: ActionInfo[];
}

/**
 * What the clipboard history offers.
 *
 * The order is deliberate: what applies only right now first, then the four
 * things the view itself does, then what the registry can do to the text.
 * An action that is always listed and almost never applicable teaches people
 * to scroll past the whole panel.
 */
export function clipboardPanel(from: ClipboardPanel): ActionEntry[] {
  // Merging is only offered once there is something to merge.
  const merging: ActionEntry[] =
    from.picked.length > 1
      ? [
          {
            id: -30,
            title: `Merge ${from.picked.length} Entries`,
            tag: "Sill.ClipboardMerge",
            props: {},
            shortcut: { modifiers: ["ctrl"], key: "m" },
          },
          {
            id: -31,
            title: `Merge ${from.picked.length} on One Line`,
            tag: "Sill.ClipboardMergeInline",
            props: {},
            shortcut: undefined,
          },
        ]
      : [];

  // Only for an entry that actually kept formatting. Offering it on a line of
  // terminal output would be offering to do nothing.
  const plain: ActionEntry[] = from.rich
    ? [
        {
          id: -32,
          title: "Paste as Plain Text",
          tag: "Sill.ClipboardPastePlain",
          props: {},
          shortcut: { modifiers: ["ctrl", "shift"], key: "enter" },
        },
      ]
    : [];

  const collecting: ActionEntry[] = [
    ...(from.picked.length
      ? [
          {
            id: -33,
            title: `Add ${from.picked.length} to a Collection`,
            tag: "Sill.ClipboardCollect",
            props: {},
            shortcut: undefined,
          },
        ]
      : []),
    ...(from.openCollection
      ? [
          {
            id: -34,
            title: `Remove from ${from.openCollection.name}`,
            tag: "Sill.ClipboardUncollect",
            props: {},
            shortcut: undefined,
          },
          {
            id: -35,
            title: `Delete the ${from.openCollection.name} Collection`,
            tag: "Sill.ClipboardForgetCollection",
            props: {},
            shortcut: undefined,
          },
        ]
      : []),
  ];

  return [
    ...merging,
    ...collecting,
    {
      id: -10,
      title: "Paste",
      tag: "Sill.ClipboardPaste",
      props: {},
      shortcut: { modifiers: [], key: "enter" },
    },
    ...plain,
    {
      id: -11,
      title: "Copy",
      tag: "Sill.ClipboardCopy",
      props: {},
      shortcut: { modifiers: ["ctrl"], key: "c" },
    },
    {
      id: -12,
      title: "Pin or Unpin",
      tag: "Sill.ClipboardPin",
      props: {},
      shortcut: { modifiers: ["ctrl"], key: "p" },
    },
    {
      id: -13,
      title: "Next Type",
      tag: "Sill.ClipboardFilter",
      props: {},
      shortcut: { modifiers: ["ctrl"], key: "t" },
    },
    {
      id: -14,
      title: "Delete",
      tag: "Sill.ClipboardDelete",
      props: {},
      // Ctrl, because the search field has focus and a bare Delete while
      // filtering used to destroy the row under the cursor instead of the
      // character being typed. With nothing typed the bare key still works;
      // the panel advertises the one that always does.
      shortcut: { modifiers: ["ctrl"], key: "delete" },
    },
    // What can be done to the text itself, from the same registry the root
    // list draws from. Paste, pin, filter and delete above act on the list;
    // these act on the content.
    //
    // The registry's primary for a clipboard row is a plain Copy, which this
    // view already offers above. Showing it twice under two shortcuts is
    // worse than either.
    ...from.registry
      .filter((action) => !action.primary)
      .map((action, index) => ({
        id: -20 - index,
        title: action.title,
        tag: `Sill.Action:${action.id}`,
        props: {},
        // The action's own, from Rust. These used to arrive with none at all,
        // so Read Aloud sat on the clipboard list advertising nothing while
        // the four rows above it advertised chords written by hand.
        shortcut: action.shortcut,
      })),
  ];
}

/**
 * Whether a name given to this row would still mean something tomorrow.
 *
 * An alias points at a command id and is matched against the index, so it is
 * only worth offering on a row whose id survives a restart. A calculator
 * answer exists for as long as it is on screen, a window's id is a handle that
 * stops being valid when it closes, and a program's audio session carries the
 * process number in it, so naming one would be naming this morning's copy of
 * that program. A running process is that last one exactly: its id **is** the
 * process number.
 *
 * A conversation is not in the index, so a name given to one would find
 * nothing however carefully it was chosen. Nor is an extension in the store,
 * for a stronger version of the same reason: it may not be installed at all,
 * so there is nothing on this machine for a name to point at. Once it is
 * installed its commands are in the index and each can be named there.
 *
 * Written as the kinds that cannot rather than the kinds that can, so a kind
 * added later is namable by default and reads oddly rather than vanishing.
 */
const UNNAMABLE = new Set([
  "answer",
  "window",
  // A tab, for the same reason a window is: its id holds a window handle and
  // the browser's own identifier for the tab, and both stop meaning anything
  // when the tab closes. A name given to today's tab would point at nothing
  // tomorrow.
  "browser-tab",
  "audio-session",
  "process",
  // One row for whatever is playing, built by the search and gone the moment
  // the music stops. Its id is fixed, so a name given to it would survive and
  // point at nothing, which is worse than not offering one.
  "media",
  "conversation",
  "past-conversation",
  "store-listing",
]);

export function namable(row: RankedCommand | undefined): boolean {
  return !!row && !UNNAMABLE.has(row.mode);
}

/**
 * What the registry says can be done to the selected row, plus its name.
 *
 * This used to be two entries written by hand, which meant the panel and the
 * Enter key were two separate opinions about what a result supports.
 */
export function rowPanel(registry: ActionInfo[], row: RankedCommand | undefined): ActionEntry[] {
  // Naming a result is offered on the result, not buried in settings. An alias
  // nobody can reach is one nobody sets, and the launcher is where you are
  // when you notice you want one.
  const naming: ActionEntry[] =
    row && namable(row)
      ? [
          {
            id: -40,
            title: row.alias ? `Rename "${row.alias}"` : "Give It a Name",
            tag: "Sill.SetAlias",
            props: {},
            shortcut: undefined,
          },
          ...(row.alias
            ? [
                {
                  id: -41,
                  title: `Forget the Name "${row.alias}"`,
                  tag: "Sill.ClearAlias",
                  props: {},
                  shortcut: undefined,
                },
              ]
            : []),
        ]
      : [];

  return [
    ...registry.map((action, index) => ({
      id: -1 - index,
      title: action.title,
      tag: `Sill.Action:${action.id}`,
      props: {},
      /*
       * Enter for the primary one, and otherwise whatever the action says.
       *
       * Enter stays written here rather than being declared in Rust, because
       * for the primary action it is not a shortcut: it is the `open`
       * movement, handled by the chord map with everything the launcher does
       * on the way out. Declaring it as a shortcut as well would put two
       * handlers on one key.
       */
      shortcut: action.primary ? { modifiers: [], key: "enter" } : action.shortcut,
    })),
    ...naming,
  ];
}
