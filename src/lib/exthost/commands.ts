/**
 * The launcher's own commands, as opposed to the extension API.
 *
 * Thin wrappers so components call typed functions rather than stringly-typed
 * `invoke` calls scattered through markup.
 */

import { invoke } from "@tauri-apps/api/core";
import { orElse, silently } from "$lib/status";
// The one shape a chord is written in, shared with the matcher that reads it.
import type { Shortcut } from "./actions";

export interface RankedCommand {
  id: string;
  extension: string;
  extensionTitle: string;
  title: string;
  subtitle: string;
  /**
   * Where the entry came from.
   *
   * "app" is an installed application, "exe" a bare executable found on PATH,
   * and the other two are extension commands.
   */
  mode:
    | "view"
    | "no-view"
    | "app"
    | "exe"
    | "setting"
    | "file"
    | "builtin"
    | "answer"
    | "snippet"
    | "sill-setting"
    /** A quicklink that opens straight away. */
    | "quicklink"
    /** A quicklink with `{query}` in it, which takes over the field first. */
    | "quicklink-arg"
    | "script"
    | "script-arg"
    /** A window that is open right now. Never from the index. */
    | "window"
    /**
     * A web address a browser remembers. Never from the index either.
     *
     * Read out of a browser's own database when the query was typed, and gone
     * again afterwards, so like a window it is opened through the action
     * registry rather than launched by id.
     */
    | "url"
    /**
     * Words to look up on the web.
     *
     * Not an address yet. Which engine turns them into one is a setting read
     * when the row is chosen, so the window carries only what was typed.
     */
    | "websearch"
    /**
     * A switch belonging to Windows: the volume, the theme, the lock screen.
     *
     * Its own kind so it groups apart and wears Windows' own icon. A row that
     * changes the machine should not look like one of Sill's own commands.
     */
    | "system"
    /**
     * The row standing in for files that could not be searched.
     *
     * Not a thing to launch. Choosing it fixes what it names, and it only
     * exists while there is something to fix.
     */
    | "file-setup"
    /** One emoji. Its own corpus, reached through its own command. */
    | "emoji"
    /** One program's own volume, while it is playing something. */
    | "audio-session"
    /** One running program, while it is running. */
    | "process"
    /**
     * What is playing, as one row with play, pause and next on it.
     *
     * Not an index entry and never more than one: the search builds it when
     * the query is one of the words that asks for it, and only if something is
     * actually playing. Its actions act on whatever Windows says the current
     * session is, which is the session a media key on the keyboard would reach.
     */
    | "media"
    /** A folder offered as somewhere to move something into. */
    | "destination"
    /**
     * The conversation you left, offered back.
     *
     * Not an index entry and not launched: choosing it reopens a mode in this
     * window. It exists only while the conversation is recent, and there is
     * never more than one.
     */
    | "conversation"
    /**
     * One row in the list of everything that has been asked.
     *
     * Its own kind rather than reusing `conversation`, which is the single row
     * offering back the one you left. These are the whole history, they carry
     * a delete, and they never appear in the root list.
     */
    | "past-conversation"
    /**
     * One extension as the store lists it, installed here or not.
     *
     * Its own kind rather than `view` or `no-view`, which are extension
     * *commands*: those are installed and can be run, and a listing may have
     * no files on this machine at all. Its entrypoint is the extension's name,
     * because that is what every store operation takes.
     *
     * Never in the root list. It exists so the action panel can ask what can
     * be done to the row the store has under its cursor, which it could not do
     * while the store's rows were the one shape nothing outside that view
     * could name.
     */
    | "store-listing";
  entrypoint: string;
  /** A file to take an icon from, when it differs from the launch target. */
  icon?: string | null;
  /**
   * The settings panel this belongs to, for anything Sill owns.
   *
   * Set for Sill's own commands and for individual settings, so both arrive
   * in the launcher wearing the mark they wear in settings. Rust decides it,
   * because the answer is a fact about the command rather than a rendering
   * choice, and a copy of the mapping here would drift.
   */
  panel?: string | null;
  /**
   * Whether this row is a switch, and which way it is set.
   *
   * Absent for everything that is not one, which is nearly everything. A row
   * that carries it draws as a control: pressing it flips the thing and leaves
   * the launcher where it is, so the state can be watched changing.
   */
  toggle?: boolean;
  /** Indices into `title` that matched the query, for highlighting. */
  matched: number[];
  /**
   * Whether the query named this rather than merely fitting it.
   *
   * Absent on most rows. The root list merges more than one search, and this
   * is how it tells a result somebody typed the name of from one that only
   * happens to contain the same letters in the same order.
   */
  strong?: boolean;
  /** The name the user gave this, when they gave it one. */
  alias?: string;
}

export interface LaunchedCommand {
  session: string;
  title: string;
  extensionTitle: string;
  /** "no-view" runs and exits; "app" and "exe" are launched by the shell. */
  mode:
    | "view"
    | "no-view"
    | "app"
    | "exe"
    | "setting"
    | "file"
    | "builtin"
    | "answer"
    | "snippet"
    | "sill-setting"
    /** A quicklink that opens straight away. */
    | "quicklink"
    /** A quicklink with `{query}` in it, which takes over the field first. */
    | "quicklink-arg"
    | "script"
    | "script-arg"
    /** A window that is open right now. Never from the index. */
    | "window"
    /**
     * A web address a browser remembers. Never from the index either.
     *
     * Read out of a browser's own database when the query was typed, and gone
     * again afterwards, so like a window it is opened through the action
     * registry rather than launched by id.
     */
    | "url"
    /**
     * Words to look up on the web.
     *
     * Not an address yet. Which engine turns them into one is a setting read
     * when the row is chosen, so the window carries only what was typed.
     */
    | "websearch"
    /** One emoji. Its own corpus, reached through its own command. */
    | "emoji"
    /** One program's own volume, while it is playing something. */
    | "audio-session"
    /** One running program, while it is running. */
    | "process"
    /**
     * A switch belonging to Windows.
     *
     * Missing here for as long as these have existed, which made a Windows
     * switch a thing that could be shown and, as far as the types knew, never
     * launched. The window read that as "there is more to show" and put up an
     * extension screen with no extension in it, named after the switch.
     *
     * This union is a second copy of the one above it and they drifted. It is
     * still two lists, and the next thing added to one has to be added to the
     * other.
     */
    | "system";
  /** What the action said it did, in one line. */
  message: string;
  /**
   * Where a switch ended up, when the thing run was one.
   *
   * A row carrying this stays on screen showing its new state instead of the
   * launcher closing, so the thing being switched can be watched changing.
   */
  toggle?: boolean;
}

export function searchCommands(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_commands", { query });
}

/**
 * Where the given switches are set, right now.
 *
 * Asked after one has been pressed, because pressing one can move another:
 * the audio outputs are a single choice spread across several rows. Answers in
 * the order it was asked, with `null` for anything that is not a switch.
 */
/**
 * Every program playing something right now, with its own volume.
 *
 * Its own call rather than part of the root search, because enumerating the
 * audio sessions costs about three milliseconds and the root list runs on
 * every keystroke whether or not anything about sound was typed.
 */
/**
 * The folders something could be moved into.
 *
 * With nothing typed these are the folders already used and then the standard
 * places; once something is typed it is a folder search. `source` is what is
 * being moved, so the folder it is already in is never offered.
 */
export function searchDestinations(
  query: string,
  source: string,
): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_destinations", { query, source });
}

export function searchAppVolume(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_app_volume", { query });
}

/**
 * Everything running right now, heaviest first.
 *
 * Its own call rather than part of the root search, and for a stronger version
 * of the reason the volume list has one: this walks every process on the
 * machine and opens each of them to read what it costs, and the root list runs
 * on every keystroke whether or not anybody asked about processes.
 */
export function searchProcesses(query: string): Promise<RankedCommand[]> {
  return invoke<RankedCommand[]>("search_processes", { query });
}

/**
 * Tells Rust the window has painted, which is when a summon is actually over.
 *
 * Rust can see when it told the window to show itself and not when this page
 * finished drawing, and the drawing is the half somebody waited for.
 */
/**
 * A picture of one open window, for the switcher.
 *
 * Asked for the selected row only, never for the list. `null` when the window
 * has closed, is minimized, or refuses to be photographed, which is not an
 * error: a switcher with no picture is still a switcher.
 */
export function windowPreview(id: string): Promise<string | null> {
  return invoke<string | null>("window_preview", { id });
}

/** Drops every window picture. Called when the switcher closes. */
export function forgetPreviews(): Promise<void> {
  return invoke<void>("forget_previews");
}

/** Something handed to the model along with a question. */
export interface AiAttached {
  name: string;
  /** `image` or `text`. */
  kind: string;
  /** A data URI for a picture; the text itself for a text file. */
  body: string;
  /** How big the original was. */
  bytes: number;
}

/** One thing said, in a conversation with a model. */
export interface AiTurn {
  /** "user" or "assistant". */
  role: string;
  text: string;
  /** What was handed over with it, so a reopened conversation still shows it. */
  attachments: AiAttached[];
}

/** Whether asking would work, and who would answer. */
export interface AiReady {
  ready: boolean;
  /** Which provider, so the window can draw its mark. */
  id: string;
  /** What the chosen one is called. */
  name: string;
  /**
   * The model as it is read, which is not the id it is asked for.
   *
   * Shortened in Rust so the chip and the settings window agree. Empty when
   * the provider decides for itself, which is what Claude Code does.
   */
  model: string;
  /**
   * Where the answer comes from: `local`, `cli` or `key`.
   *
   * Three kinds rather than seven names, because the useful distinction is
   * whose machine answers and who pays: this one, a subscription through a
   * tool already signed in, or a key.
   */
  kind: string;
  /** Why not, when not. Empty when it is ready. */
  whyNot: string;
}

/**
 * Whether anything is set up to answer.
 *
 * Asked before offering the question rather than after: inviting somebody to
 * press Tab and then saying "no provider" wastes the keystroke and the
 * sentence they typed.
 */
/**
  * Finishes a path somebody is part way through typing.
  *
  * `null` when the folder has nothing to add, which is the ordinary answer
  * while a folder that does not exist yet is being typed.
  *
  * Silent on failure for the same reason the searches are: this is a key
  * press, and the cost of a failure is that Tab does nothing once.
  */
/** One line of the keyboard reference. */
export type KeyLine = {
  chord: string;
  does: string;
  changed: boolean;
  contested: boolean;
};

/** A group of lines under a heading. */
export type KeySection = { title: string; keys: KeyLine[] };

/**
 * The keyboard reference, assembled in Rust from the keys that actually run.
 *
 * Not silent: this is a page somebody opened on purpose, so a failure has
 * somewhere to be shown and should be.
 */
export function keyboardReference(): Promise<KeySection[]> {
  return invoke<KeySection[]>("keyboard_reference");
}

export function completePath(typed: string): Promise<string | null> {
  return invoke<string | null>("complete_path", { typed }).catch(silently(null));
}

export function aiReady(): Promise<AiReady> {
  return invoke<AiReady>("ai_ready");
}

/**
 * Asks, and streams the answer back as `sill://ai-said` events.
 *
 * Resolves with the whole answer as well, so a caller that only wants the text
 * does not have to reassemble it from the events.
 */
export function aiAsk(question: string, attachments: AiAttached[] = []): Promise<string> {
  return invoke<string>("ai_ask", { question, attachments });
}

/**
 * Reads a file into something that can be handed to a model.
 *
 * One at a time, and each answers for itself: choosing five files where one is
 * an archive should attach four and explain one, rather than attaching nothing.
 */
export function aiAttach(path: string): Promise<AiAttached> {
  return invoke<AiAttached>("ai_attach", { path });
}

/**
 * Stops whatever is being written.
 *
 * Not a cancel. What has already arrived is kept and becomes the answer,
 * because somebody who stops a reply has usually read enough of it.
 */
export function aiStop(): Promise<void> {
  return invoke<void>("ai_stop");
}

/** How big anything handed over may be. */
export interface AiLimits {
  image: number;
  text: number;
}

/**
 * The ceilings, asked for rather than repeated.
 *
 * A picture pasted from the clipboard never touches the disk, so it cannot go
 * through the reader that knows these numbers. Asking keeps one definition.
 */
export function aiLimits(): Promise<AiLimits> {
  return invoke<AiLimits>("ai_limits");
}

/**
 * Asks the next thing in the conversation already open.
 *
 * Its own call rather than a flag on the one above. Appending to whatever came
 * before, forever, is the behaviour these two replace, and a boolean argument
 * is a thing a call site can get wrong. Two names cannot be.
 */
export function aiFollowUp(
  question: string,
  attachments: AiAttached[] = [],
): Promise<string> {
  return invoke<string>("ai_follow_up", { question, attachments });
}

/**
 * Sets the open conversation aside so the next question begins its own.
 *
 * Not `aiClear`, which forgets every conversation. The one set aside is still
 * offered back from the root list until it goes stale.
 */
export function aiNew(): Promise<void> {
  return invoke<void>("ai_new");
}

/** Reopens a conversation, and answers with everything said in it. */
export function aiResume(id: string): Promise<AiTurn[]> {
  return invoke<AiTurn[]>("ai_resume", { id });
}

/** Something the model wants to do, waiting on a decision. */
export interface AiAsking {
  id: string;
  /** The action, as the panel would title it. */
  title: string;
  /** What it is about to act on. */
  subject: string;
  /** What it touches, in words somebody deciding would use. */
  touches: string;
}

/**
 * Answers a card.
 *
 * The turn is sitting waiting for this: the model asked, the loop paused, and
 * nothing else happens until it arrives.
 */
export function aiDecide(id: string, allowed: boolean): Promise<void> {
  return invoke<void>("ai_decide", { id, allowed });
}

/** Refuses everything still waiting, which is what leaving means. */
export function aiRefusePending(): Promise<void> {
  return invoke<void>("ai_refuse_pending");
}

/** One tool the model reached for, as the window draws it. */
export interface AiStep {
  tool: string;
  /** What it was used on. Empty for the tools that take no arguments. */
  subject: string;
}

/** One conversation, as the list of them draws it. */
export interface AiConversation {
  id: string;
  /** The question it opened with. */
  title: string;
  replies: number;
  /** Seconds since it was last spoken to. */
  age: number;
  /** Whether this is the one currently open. */
  open: boolean;
}

/** Every conversation, newest first. */
export function aiConversations(): Promise<AiConversation[]> {
  return invoke<AiConversation[]>("ai_conversations");
}

/**
 * Forgets one, and answers with what is left.
 *
 * The list comes back rather than the caller working out what removing one
 * did, which is the difference between a list that is right and a list that
 * agrees with itself.
 */
export function aiForget(id: string): Promise<AiConversation[]> {
  return invoke<AiConversation[]>("ai_forget", { id });
}

/** Forgets all of them. */
export function aiForgetAll(): Promise<void> {
  return invoke<void>("ai_forget_all");
}

/** Everything said so far, for a window that has just opened. */
export function aiTranscript(): Promise<AiTurn[]> {
  return invoke<AiTurn[]>("ai_transcript");
}

/** Forgets the conversation. */
export function aiClear(): Promise<void> {
  return invoke<void>("ai_clear");
}

export function summonPainted(): Promise<void> {
  return invoke<void>("summon_painted");
}

export function systemStates(ids: string[]): Promise<(boolean | null)[]> {
  return invoke<(boolean | null)[]>("system_states", { ids });
}

/**
 * Runs an indexed command, and tells Sill what was typed to reach it.
 *
 * The query is what makes an abbreviation learnable. Choosing Gmail after
 * typing `ggm` says something the id alone cannot: not that Gmail is popular,
 * but that `ggm` means Gmail.
 */
export function launchCommand(id: string, query?: string): Promise<LaunchedCommand> {
  return invoke<LaunchedCommand>("launch_command", { id, query });
}

/**
 * Counts a use of something the window opened by itself.
 *
 * The clipboard history becomes a view rather than a launch, and a quicklink
 * with a hole in it takes over the field. Neither reaches `launch_command`,
 * so without this neither is visible to ranking at all: `sill:clipboard` had
 * never been recorded once, however often it was opened.
 */
export function recordUse(
  id: string,
  query?: string,
  history = true,
): Promise<void> {
  // Silent. It answers with nothing, it is called on every launch, and all
  // that is lost is one use going uncounted towards ranking. There is no
  // screen it makes wrong and nothing anybody could do about it.
  return invoke<void>("record_use", { id, query, history }).catch(silently(undefined));
}

/**
 * What was typed before, most recent first.
 *
 * Only queries that reached something. A shell recalls everything typed
 * including the typos; a launcher offering back the half-finished strings
 * somebody abandoned would mostly be offering them their mistakes.
 */
export function queryHistory(): Promise<string[]> {
  // Silent. An empty list means Up recalls nothing, which announces itself to
  // the person pressing Up in the instant they press it. This is also re-read
  // on every summon, so a report would be a sentence about a key somebody has
  // already discovered does not work.
  return invoke<string[]>("query_history").catch(silently([]));
}

export function unloadExtension(session: string): Promise<boolean> {
  return invoke<boolean>("unload_extension", { session });
}

export function activateHandler(
  session: string,
  handler: string,
  args: unknown[] = [],
): Promise<unknown> {
  return invoke("activate_handler", { session, handler, args });
}

/**
 * Runs an action Raycast implements itself, e.g. Action.CopyToClipboard.
 *
 * The session goes with it because these reach the same capabilities the
 * extension's own API calls do, and Rust asks the same question about them.
 * It is an id to be looked up, not a claim to be believed.
 */
export function performBuiltin(
  session: string,
  tag: string,
  props: Record<string, unknown>,
): Promise<string> {
  return invoke<string>("perform_builtin", { session, tag, props });
}

/**
 * The icon for a launchable, as a data URI.
 *
 * Cached here as well as in Rust: a row re-renders on every keystroke while
 * filtering, and an await per row per frame would be a lot of IPC for an
 * answer that cannot change.
 *
 * ## What is not cached
 *
 * A failure. It used to be, because the `catch` produced a resolved promise
 * that went straight into the map, so a file that was locked for a moment, or
 * an executable being replaced by an installer as the list was drawn, had no
 * icon for the rest of the session and no way to try again.
 *
 * ## Why it is bounded
 *
 * Rust holds the same answers, so this is a second copy of them, and each one
 * is a data URI of a few kilobytes. Unbounded, it grows towards one entry for
 * every application on the machine whether or not any of them is on screen.
 * The oldest go first, which for a launcher is the right end: what somebody is
 * looking at now is what they asked for most recently.
 */
const ICONS_KEPT = 400;

/**
 * One remembered answer, and whether it has arrived yet.
 *
 * The answer was held only as a promise, and a promise cannot be read without
 * waiting: a row drawn for a path this session had already resolved still had
 * to await it, so it rendered its lettered tile first and swapped the icon in
 * afterwards. That is the flash. Typing makes new rows every keystroke and
 * nearly all of them are applications that were on screen a moment ago, so it
 * was a flash per row per keystroke for icons already sitting in this map.
 *
 * `uri` is written when the promise settles, which is what makes the answer
 * readable without waiting. `undefined` there means "not answered yet", which
 * is a third state and is why this is a record rather than a nullable string:
 * `null` is a real answer meaning the file has no icon.
 */
interface Held {
  asked: Promise<string | null>;
  uri?: string | null;
}

const iconCache = new Map<string, Held>();

/**
 * The icon for a path, if the answer is already here.
 *
 * `undefined` when nothing is known yet, which a caller draws as a reserved
 * empty tile rather than as a guess. Synchronous on purpose: a component that
 * has to await cannot help but paint something first, and what it painted was
 * wrong.
 */
/**
 * Whether the shell can make an icon out of this at all.
 *
 * Extension commands have no icon of their own yet, so only applications are
 * worth asking about. A packaged app is identified by an AppUserModelID rather
 * than a file and the shell cannot make an icon out of that, so those are
 * known to have none without a round trip, and a row drawing one gets its
 * lettered tile on the first frame rather than after a refusal.
 */
export function hasShellIcon(path: string, resolvable: boolean): boolean {
  return resolvable && Boolean(path) && !path.startsWith("shell:AppsFolder");
}

export function knownIcon(path: string): { uri: string | null } | undefined {
  const held = iconCache.get(path);
  return held && "uri" in held ? { uri: held.uri ?? null } : undefined;
}

export function appIcon(path: string): Promise<string | null> {
  const held = iconCache.get(path);
  if (held) return held.asked;

  const pending = invoke<string | null>("app_icon", { path }).catch((reason) => {
    // Forgotten rather than remembered as "no icon", so the next row that
    // asks tries again.
    //
    // The reason then goes no further, and that is the judgment rather than an
    // oversight: this is asked once per row drawn, so a report would land on
    // the path that paints the list, and a row without an icon is one somebody
    // can still read and still run.
    iconCache.delete(path);
    return silently<string | null>(null)(reason);
  });

  const entry: Held = { asked: pending };
  iconCache.set(path, entry);

  void pending.then((uri) => {
    // Only if this entry is still the one being held. A failure deletes itself
    // above and a later ask makes a new record, so writing the answer onto
    // whatever is in the map now would revive an entry that was dropped.
    if (iconCache.get(path) === entry) entry.uri = uri;
  });

  // A `Map` iterates in insertion order, so the first key is the oldest.
  while (iconCache.size > ICONS_KEPT) {
    const oldest = iconCache.keys().next();
    if (oldest.done) break;
    iconCache.delete(oldest.value);
  }

  return pending;
}

export interface FileHit {
  name: string;
  path: string;
  isDir: boolean;
}

/** Why file search cannot answer, or nothing when it can. */
export type FileSearchMissing = "indexing" | "absent" | "asleep";

/**
 * What is standing between a typed query and a list of files.
 *
 * Asked on summon rather than per keystroke. The answer only changes when a
 * program starts or stops, which is not something typing does.
 */
export function fileSearchMissing(): Promise<FileSearchMissing | null> {
  /*
   * Reported, because `null` here is a positive claim.
   *
   * Every other value names something standing in the way and the launcher
   * shows a row saying so. `null` means nothing is, and the row is not drawn.
   * A failure that produces `null` therefore tells somebody who is typing a
   * filename and seeing nothing that file search is working fine, which is the
   * one answer that stops them looking further.
   */
  return invoke<FileSearchMissing | null>("file_search_missing").catch(
    orElse("launcher", "what is stopping file search from answering", null, "files"),
  );
}

/** Does whatever the thing standing in the way needs. */
export function startFileSearch(): Promise<string> {
  return invoke<string>("start_file_search");
}

/** A drive that could be indexed. */
export interface Drive {
  root: string;
  label: string;
  kind: "fixed" | "removable" | "network" | "optical";
  indexed: boolean;
}

/**
 * Every mounted drive, and whether Sill reads it.
 *
 * Reported when it fails. The settings pane draws this as the list of drives
 * on the machine, and no machine has none, so an empty one is never the truth.
 * It is also the pane somebody opens precisely because file search is not
 * finding things, which is the worst moment to be shown a blank list.
 */
export function listDrives(): Promise<Drive[]> {
  return invoke<Drive[]>("list_drives").catch(
    orElse("settings", "which drives are on this machine", [], "files"),
  );
}

/**
 * Starts or stops indexing one folder.
 *
 * Answers with the folders indexed afterwards, so the caller does not have to
 * guess what its own change produced.
 */
export function indexFolder(path: string, wanted: boolean): Promise<string[]> {
  return invoke<string[]>("index_folder", { path, wanted });
}

/**
 * Everything the index does not hold: files and pages a browser remembers.
 *
 * One call rather than two. They used to be separate commands awaited one
 * after the other, so a keystroke that got past the debounce cost two round
 * trips and the browser search did not start until the file search had
 * finished. Rust runs them at the same time now.
 */
export interface Elsewhere {
  files: FileHit[];
  pages: BrowserHit[];
}

export function searchElsewhere(query: string): Promise<Elsewhere> {
  // Silent, and the reason is shared with the two searches below it. This runs
  // once per keystroke, so a report would be written and overwritten faster
  // than anybody could read it, and it would put an extra message on the path
  // whose whole job is to answer before the next character arrives. What a
  // failure costs is one query's results, and the next keystroke asks again.
  return invoke<Elsewhere>("search_elsewhere", { query }).catch(
    silently({ files: [], pages: [] }),
  );
}

/**
 * The open windows matching a query.
 *
 * A third corpus beside the index and the filesystem, and the one with the
 * shortest life: it is enumerated fresh on every call, because a window list
 * is wrong the moment anything is opened, closed or renamed. Nothing here is
 * cached for the same reason.
 *
 * Ranked in Rust by the same function as everything else, so these arrive
 * already in order and merge straight into the list.
 */
/**
 * Emoji matching a query.
 *
 * Its own corpus rather than part of the index. Three thousand seven hundred
 * entries would nearly quadruple an index that is ranked on every keystroke,
 * so that typing "smile" could find an emoji as well as an application.
 */
export function searchEmoji(query: string, inline = false): Promise<RankedCommand[]> {
  // Per keystroke. See `searchElsewhere`.
  return invoke<RankedCommand[]>("search_emoji", { query, inline }).catch(silently([]));
}

export function searchWindows(query: string): Promise<RankedCommand[]> {
  // Per keystroke. See `searchElsewhere`.
  return invoke<RankedCommand[]>("search_windows", { query }).catch(silently([]));
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface MonitorInfo {
  index: number;
  full: Rect;
  work: Rect;
  primary: boolean;
}

export function listMonitors(): Promise<MonitorInfo[]> {
  // Silent, and nothing in the window calls this today: it is here for the
  // extension surface. A wrapper nobody draws from cannot mislead anybody, and
  // whoever gives it a caller can decide then whether an empty list would.
  return invoke<MonitorInfo[]>("list_monitors").catch(silently([]));
}

export function openPath(path: string): Promise<void> {
  return invoke("open_path", { path });
}

/**
 * Presents a file as a row.
 *
 * Files reuse the command row rather than getting a list of their own, so
 * selection, windowing and the keyboard all work unchanged. Everything has
 * already ranked them, so `score` is 0 and they simply follow the commands.
 */
export function fileAsCommand(hit: FileHit): RankedCommand {
  return {
    id: `file:${hit.path}`,
    extension: "file",
    extensionTitle: hit.isDir ? "Folder" : "File",
    title: hit.name,
    subtitle: hit.path,
    mode: "file",
    entrypoint: hit.path,
    icon: hit.path,
    matched: [],
  };
}

/** A page a browser remembers. */
export interface BrowserHit {
  title: string;
  url: string;
  /** Which browser it came from, so two copies of a page are tellable apart. */
  browser: string;
  /** Saved rather than merely visited. */
  bookmark: boolean;
  visits: number;
  /** The program behind the browser it came from, for the row's icon. */
  icon: string | null;
}

/**
 * A page as a result row.
 *
 * Reuses the command row, exactly as files do, so selection, grouping and the
 * keyboard all work without knowing what a browser is.
 *
 * The address is the subtitle rather than the title because it is what tells
 * two pages of the same name apart, and the title is what somebody is typing
 * at. The browser it came from goes in the group heading, not the row: with
 * one browser installed it would be the same word on every line.
 */
export function browserAsCommand(hit: BrowserHit): RankedCommand {
  return {
    id: `browser:${hit.url}`,
    extension: "browser",
    extensionTitle: hit.bookmark ? "Bookmarks" : "History",
    title: hit.title,
    subtitle: hit.url,
    mode: "url",
    entrypoint: hit.url,
    // The browser it came out of, not Sill. A page from Edge and a page from
    // Zen are told apart at a glance, and neither is dressed as one of Sill's
    // own commands, which is the same rule the Windows switches follow.
    icon: hit.icon ?? undefined,
    matched: [],
  };
}

/**
 * Reads the words out of the last picture copied.
 *
 * Returns what happened, to be shown as it is: how many words were found, or
 * that the picture had none, or why it could not be read.
 */
export function extractTextFromLastImage(): Promise<string> {
  return invoke<string>("extract_text_from_last_image");
}

/**
 * The program that opens a web address on this machine.
 *
 * Asked once, on the way in, rather than per keystroke: the default browser
 * does not change while somebody is typing.
 */
export function defaultBrowser(): Promise<string | null> {
  // Silent. `null` is what this answers on a machine with no default browser
  // set, and the row that reads it falls back to saying "browser" rather than
  // naming one. The offer still works and still opens the address.
  return invoke<string | null>("default_browser").catch(silently(null));
}

/**
 * The row that offers to look up what was typed.
 *
 * Built here rather than asked for, because asking Rust to compose one row per
 * keystroke is exactly the chatter rule 18 is about, and there is nothing to
 * decide until it is chosen: the address is not built until then.
 *
 * It carries the words, not a URL. Which engine turns them into one is a
 * setting, and it can change between this being offered and being picked.
 *
 * `browser` is the program the search will open in, and it is what the row
 * wears. Searching the web is not something Sill does; it is Sill handing the
 * question to that browser, and a row marked with Sill's own gear would say
 * otherwise. It is the same rule the Windows switches follow.
 */
export function webSearchRow(query: string, browser?: string): RankedCommand {
  return {
    id: "websearch:query",
    extension: "websearch",
    extensionTitle: "Web Search",
    title: `Search for ${query}`,
    subtitle: "",
    mode: "websearch",
    entrypoint: query,
    icon: browser,
    matched: [],
  };
}

/**
 * The row that offers to open what was typed, when it is an address.
 *
 * Above the web search row, because an address is a destination rather than a
 * question: typing `https://example.com` and pressing Enter used to search the
 * web for the words "https://example.com", which is nobody's intent.
 *
 * Built here for the same reason the web search row is, and wearing the same
 * browser mark, because opening it is Sill handing the address to that browser
 * rather than something Sill does itself.
 */
export function urlRow(address: string, browser?: string): RankedCommand {
  return {
    id: "url:typed",
    extension: "websearch",
    extensionTitle: "Browser",
    title: `Open ${address}`,
    subtitle: "",
    mode: "url",
    entrypoint: address,
    icon: browser,
    matched: [],
  };
}

/**
 * The row that offers to open what was typed, when it is a path.
 *
 * `C:\Users\Brandon` did nothing at all: it is not in the index, it is not a
 * command, and the file search matches names rather than whole paths. Somebody
 * who typed a whole path has already said exactly what they want.
 */
export function pathRow(path: string): RankedCommand {
  return {
    id: "file:typed",
    extension: "files",
    extensionTitle: "Files",
    title: `Open ${path}`,
    subtitle: "",
    mode: "file",
    entrypoint: path,
    matched: [],
  };
}

export function dismiss(): Promise<void> {
  return invoke("dismiss");
}

/**
 * Summons the launcher, optionally with a command to run once it is up.
 *
 * For callers outside the launcher window, which today means the
 * notification-area menu. The launcher hears `sill://run` on arrival and
 * decides what the command looks like; the caller only states the intent.
 */
export function summonWith(command?: string): Promise<void> {
  return invoke("summon_with", { command: command ?? null });
}

/** One thing that can be done to a result, as the registry describes it. */
export interface ActionInfo {
  id: string;
  title: string;
  /** What Enter does. Exactly one per kind. */
  primary: boolean;
  /**
   * The chord that runs it, after whatever the person has set in Settings.
   *
   * From Rust rather than written beside the row here, which is what it was:
   * the launcher hardcoded the clipboard's chords, so an action reached
   * through any other list arrived with none and the panel drew nothing.
   * Already resolved, so this is the key that fires rather than the one the
   * action shipped with.
   */
  shortcut?: Shortcut;
}

export interface ActionOutcome {
  message: string;
  /**
   * Which entry in the activity log this became, when it can be taken back.
   *
   * An id rather than a description of how to reverse it. The window used to
   * be handed the reversal itself and to hand it back on Ctrl+Z, which never
   * told the log anything, so the same action stayed undoable and "Undo Last
   * Action" would do it a second time.
   */
  undoneBy?: number;
  session?: string;
}

/**
 * What can be done to a result of this kind.
 *
 * Asked by mode rather than by id, because the answer depends only on what
 * kind of thing it is, and a file result was never in an index to look up.
 */
export function actionsFor(mode: string): Promise<ActionInfo[]> {
  return invoke<ActionInfo[]>("actions_for", { mode });
}

/** The thing an action is being run against. */
export interface ActionTarget {
  id: string;
  /** Which kind of thing it is. Rust maps this to a kind and dispatches. */
  mode: string;
  /** What the action acts on: a path, a panel, a stored id, or the text. */
  target: string;
  title: string;
}

/**
 * Runs one action against one thing.
 *
 * Nothing here decides what an action means or whether it applies; Rust owns
 * both, and rejects a pairing the window got wrong.
 *
 * `argument` is the answer to whatever the action had to ask for first: the
 * new name for a rename, the folder for a move. Left out for everything else,
 * which is nearly everything. Those two used to be commands of their own that
 * did the work themselves, which made them the only actions this window could
 * run and nothing else could.
 */
export function runAction(
  action: string,
  object: ActionTarget,
  argument?: string,
): Promise<ActionOutcome> {
  return invoke<ActionOutcome>("run_action", { action, object, argument });
}

/** A search result, in the shape an action wants. */
export function asTarget(command: RankedCommand): ActionTarget {
  return {
    id: command.id,
    mode: command.mode,
    target: command.entrypoint,
    title: command.title,
  };
}

/**
 * Takes back one thing that was done, by naming its place in the log.
 *
 * The log spends the undo, so asking twice is refused rather than done twice.
 */
export function undoAction(id: number): Promise<string> {
  return invoke<string>("undo_activity", { id });
}

/** A row whose subtitle is a measurement rather than a description. */
export interface LiveRow {
  id: string;
  subtitle: string;
}

/**
 * What the live rows say now, or nothing at all.
 *
 * Nothing means the launcher is not visible. Rust decides that, not the timer
 * here: there are several ways the window can go away and only one of them is
 * this page deciding to, so a timer that stopped itself would be right most of
 * the time and wrong forever when it was not.
 */
export function liveRows(): Promise<LiveRow[]> {
  return invoke<LiveRow[]>("live_rows");
}

/** The named holes in a snippet, in the order to ask for them. */
export function snippetFields(id: string): Promise<string[]> {
  return invoke<string[]>("snippet_fields", { id });
}

/** Expands a snippet with its holes filled in, and pastes it. */
export function pasteSnippetFilled(
  id: string,
  values: Record<string, string>,
): Promise<string> {
  return invoke<string>("paste_snippet_filled", { id, values });
}

/** What a script asks to be told before it runs, in its author's words. */
export function scriptArguments(path: string): Promise<string[]> {
  return invoke<string[]>("script_arguments", { path });
}

/** What a finished script produced. */
export interface Finished {
  job: string;
  title: string;
  stdout: string;
  stderr: string;
  code: number | null;
  ended: "finished" | "timedOut" | "cancelled";
  truncated: boolean;
  tookMs: number;
}

/**
 * Starts a script and answers with the job, not the result.
 *
 * The result arrives on `sill://script-done`. Waiting for the result here
 * would make stopping impossible, because the thing that would stop it would
 * be waiting on it.
 */
export function runScript(path: string, args: string[]): Promise<string> {
  return invoke<string>("run_script", { path, args });
}

/** Stops one. False means it had already finished, which is not a failure. */
export function cancelScript(job: string): Promise<boolean> {
  return invoke<boolean>("cancel_script", { job });
}
