/**
 * How a worker asks for a permission it does not hold, and waits for the
 * answer without letting go of the thread.
 *
 * The module gate runs inside `require`, which is synchronous: an extension
 * that needs `http` needs it in the middle of a call that cannot yield. The
 * ordinary channel to the manager is a message port, and a message port is
 * answered on the event loop, which is exactly the thing `require` is holding.
 * So the answer comes back through shared memory instead. The worker posts
 * the question, blocks on `Atomics.wait`, and the manager writes what the
 * extension now holds into the buffer and wakes it.
 *
 * The buffer is a small fixed layout. Two `Int32` slots at the front: the
 * first is the state, zero while waiting and one once answered; the second is
 * how many bytes of answer follow. The answer itself is the JSON list of
 * capability names Rust spells, from byte eight onwards.
 *
 * `Atomics.wait` is only allowed off the main thread, which is where this
 * runs. The manager side only ever stores and notifies.
 */

/** How much shared memory one worker gets for answers. Plenty for a list of names. */
export const ASK_BYTES = 4096;

/**
 * How long a worker waits for an answer before treating silence as no.
 *
 * Longer than the card's own patience on the Rust side, which is ninety
 * seconds, so it is Rust that decides an unanswered card counts as refused
 * and the worker hears that decision rather than giving up first.
 */
export const ASK_PATIENCE_MS = 100_000;

const STATE = 0;
const LENGTH = 1;
const ANSWER_AT = 8;

/** Where the answer goes, and where it is read. */
export function askBuffer(): SharedArrayBuffer {
  return new SharedArrayBuffer(ASK_BYTES);
}

/**
 * The worker's half. Returns what the extension holds now, or `null` when
 * nobody answered in time.
 *
 * `send` carries the question to the manager and must not block: it is the
 * message port, and the reply to it arrives here through the buffer rather
 * than back on the port.
 */
export function makeAsker(
  shared: SharedArrayBuffer,
  send: (needs: string[], plainly: string) => void,
): (needs: string[], plainly: string) => string[] | null {
  const state = new Int32Array(shared, 0, 2);

  return (needs, plainly) => {
    Atomics.store(state, STATE, 0);
    send(needs, plainly);

    // "not-equal" means the answer landed between the store and the wait,
    // which is an answer all the same.
    if (Atomics.wait(state, STATE, 0, ASK_PATIENCE_MS) === "timed-out") return null;

    const length = Atomics.load(state, LENGTH);
    if (length <= 0 || length > shared.byteLength - ANSWER_AT) return null;

    // Copied out first: a decoder will not read a shared buffer directly.
    const bytes = new Uint8Array(length);
    bytes.set(new Uint8Array(shared, ANSWER_AT, length));

    try {
      const parsed: unknown = JSON.parse(new TextDecoder().decode(bytes));
      return Array.isArray(parsed) ? parsed.map(String) : null;
    } catch {
      return null;
    }
  };
}

/**
 * The manager's half. Writes what the extension holds and wakes the worker.
 *
 * Harmless when nothing is waiting: the state is set and nobody reads it
 * until the next question stores zero over it again. Returns false only when
 * the list would not fit, which a list of a dozen names never does.
 */
export function answer(shared: SharedArrayBuffer, capabilities: readonly string[]): boolean {
  const bytes = new TextEncoder().encode(JSON.stringify(capabilities));
  if (bytes.length > shared.byteLength - ANSWER_AT) return false;

  new Uint8Array(shared, ANSWER_AT, bytes.length).set(bytes);

  const state = new Int32Array(shared, 0, 2);
  Atomics.store(state, LENGTH, bytes.length);
  Atomics.store(state, STATE, 1);
  Atomics.notify(state, STATE);
  return true;
}
