/*!
A diagnostic bundle somebody can send.

Sill has no telemetry and is not going to have any, which leaves one way to
find out what went wrong on a machine that is not this one: ask the person to
send what their copy knows. That only works if the answer is safe to send, and
the reason a launcher's diagnostics are usually not safe to send is that a
launcher touches everything.

## What this holds back, and why that is the feature

Sill has, on disk, in one folder: provider keys sealed with DPAPI in
`preferences.json`, the loopback token the MCP bridge authenticates with,
clipboard history including entries marked confidential, each extension's
`LocalStorage` and its own preferences with sealed passwords in them, and a
file index which is a list of everything on the machine.

**None of it is in here, and none of it is left out by remembering to leave it
out.** The bundle is assembled from what the running process already has in
memory for its own settings screens, plus the log, and nothing in [`assemble`]
can reach a file. Adding a field that leaks would mean first passing that field
in, which is a thing somebody has to write on purpose.

The log is the exception, because the log is written by the whole application
and nobody can promise what a future line will put in it. So it goes through
[`Scrub`] on the way in: home directory paths, sealed values, and runs of text
that look like a key or a token are replaced with a marker. The scrub errs
towards taking too much out, since an over-redacted log costs a round trip and
an under-redacted one costs a credential.

`a_bundle_carries_no_secret` is the test that holds this. It seeds a log with a
sealed value, four shapes of real provider key and a home path, builds a
bundle, and fails if any of them survive.

## What it costs at rest

Nothing. There is no collector, no ring buffer beyond the twenty summons
`timing` already kept, and no periodic anything. A bundle is assembled when
somebody presses a button and the string is dropped as soon as it has been
written.
*/

use std::fmt::Write as _;

use crate::status::Trouble;
use crate::timing::Report;

/// How much of the log the bundle carries.
///
/// The log is allowed to reach two megabytes and the tail is what explains a
/// fault, so the bundle takes the end of it. A quarter of a megabyte is a long
/// session's worth of an application that logs on events rather than on a
/// timer, and it is small enough to attach to a message.
const LOG_TAIL_BYTES: usize = 256 * 1024;

/// What a redacted run is replaced with.
///
/// Says which rule fired rather than just blanking the text, so somebody
/// reading the bundle can tell a scrubbed key from a line that was always
/// empty.
const MASK: &str = "[redacted]";

/// The marker `secrets.rs` puts in front of anything it sealed.
const SEALED: &str = "dpapi:v1:";

/// Everything the bundle is built from.
///
/// A struct of borrowed pieces rather than an application handle, so
/// [`assemble`] has nothing it could read that it was not handed. That is the
/// property the module note is about, and it is worth the extra lifetime.
pub struct Parts<'a> {
    pub version: &'a str,
    /// When it was taken, already formatted.
    pub when: &'a str,
    /// What the log is set to say, so a thin log is not read as a quiet
    /// application.
    pub level: crate::log::Level,
    pub facts: &'a [(&'a str, String)],
    pub budgets: &'a [Budget],
    /// How many entries each source contributed.
    pub by_source: &'a [(String, usize)],
    /// Installed extensions: id, title, how many commands.
    pub extensions: &'a [(String, String, usize)],
    pub timings: &'a Report,
    /// Whatever Sill is currently failing to do, from the status surface.
    pub troubles: &'a [Trouble],
    /// The crash file, if there has been one.
    pub crash: Option<&'a str>,
    /// The log. Scrubbed here, not by the caller.
    pub log: &'a str,
    pub scrub: &'a Scrub,
}

/// One thing Sill is allowed to cost, and what it cost on this run.
///
/// `allowed` is quoted from `docs/budgets.md`, which is the contract; only the
/// rows a running Sill can measure about itself are here. `verify-source.mjs`
/// fails if one of these allowances stops appearing in that document, because
/// two numbers that must agree with nothing making them agree is how they come
/// apart.
pub struct Budget {
    pub what: &'static str,
    /// What `docs/budgets.md` allows, where it names a figure.
    pub allowed: Option<&'static str>,
    pub measured: Option<String>,
}

/// The budgets a running Sill can answer about itself.
///
/// Deliberately short. Most of the table in `docs/budgets.md` needs a release
/// build, a stopwatch or a second process, and a bundle that guessed at those
/// would be worse than one that says where to find them.
pub fn budgets(private_bytes: Option<u64>, timings: &Report) -> Vec<Budget> {
    vec![
        Budget {
            what: "Rust core, idle, home folder indexed",
            allowed: Some("40 MB"),
            measured: private_bytes.map(|bytes| format!("{:.1} MB private", megabytes(bytes))),
        },
        Budget {
            what: "Cold start to the hotkey answering",
            allowed: None,
            measured: timings.cold_start_ms.map(|ms| format!("{ms} ms")),
        },
        Budget {
            what: "Summon, hotkey to being able to type",
            allowed: None,
            measured: timings.median_ms.map(|ms| format!("{ms} ms median")),
        },
        /*
         * What a keystroke cost this person, on their machine.
         *
         * The one number in this list that nobody else can take for them.
         * Ranking is measured in a test and idle memory in a script, but how
         * long a letter took to reach a screen depends on the display, the
         * graphics driver and what else is drawing, and a bundle is the only
         * place any of that is ever visible.
         *
         * The answered figure rather than the presented one, because the
         * budget is about the part Sill does. See `timing::Painted`.
         */
        Budget {
            what: "Keystroke to the frame that draws the answer",
            allowed: Some("16 ms"),
            measured: timings
                .paints
                .iter()
                .find(|cost| cost.name == crate::timing::Painted::KeystrokeAnswered.as_str())
                .map(|cost| {
                    format!(
                        "{:.1} ms mean, {:.1} ms worst, over {}",
                        cost.average_us() as f64 / 1000.0,
                        cost.slowest_us as f64 / 1000.0,
                        cost.count,
                    )
                }),
        },
    ]
}

fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Now, with the date, unlike the time on a log line.
///
/// The log stamps only the time because it is read within a session. A bundle
/// is read by somebody else, days later, beside another bundle, so the date is
/// the part that matters.
#[cfg(windows)]
pub fn when() -> String {
    let now = local_time();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.0, now.1, now.2, now.3, now.4, now.5
    )
}

/// The same moment, as something that can be a file name.
#[cfg(windows)]
pub fn filed() -> String {
    let now = local_time();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.0, now.1, now.2, now.3, now.4, now.5
    )
}

#[cfg(windows)]
fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: fills an owned struct and takes nothing.
    let now = unsafe { GetLocalTime() };

    (
        now.wYear,
        now.wMonth,
        now.wDay,
        now.wHour,
        now.wMinute,
        now.wSecond,
    )
}

#[cfg(not(windows))]
pub fn when() -> String {
    String::new()
}

#[cfg(not(windows))]
pub fn filed() -> String {
    String::new()
}

/// This process's private bytes.
///
/// The same figure `scripts/device-tests.ps1` holds the 40 MB budget against,
/// which is `PrivateMemorySize64` there and `PagefileUsage` here: the two are
/// the same counter. A bundle reporting a different one would look like a
/// regression every time.
#[cfg(windows)]
pub fn private_bytes() -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };

    // SAFETY: fills an owned structure whose size it is told, and the
    // pseudo-handle for this process needs no closing.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    }
    .is_ok();

    ok.then_some(counters.PagefileUsage as u64)
}

#[cfg(not(windows))]
pub fn private_bytes() -> Option<u64> {
    None
}

/// Builds the bundle.
///
/// Takes only what it was handed. See the module note: that is what makes the
/// "no secrets" claim something other than a promise.
pub fn assemble(parts: &Parts) -> String {
    let mut out = String::with_capacity(64 * 1024);

    let _ = writeln!(out, "Sill diagnostics");
    let _ = writeln!(
        out,
        "Taken {} from Sill {}, logging at the {} level.",
        parts.when,
        parts.version,
        parts.level.name()
    );
    let _ = writeln!(
        out,
        "\nMeant to be sent to somebody. What it holds back is listed at the end."
    );

    section(&mut out, "This machine");
    for (name, value) in parts.facts {
        let _ = writeln!(out, "  {name:<34}{}", parts.scrub.text(value));
    }

    section(&mut out, "Budgets");
    let _ = writeln!(out, "  {:<38}{:<22}{}", "What", "Measured", "Allowed");
    for budget in parts.budgets {
        let _ = writeln!(
            out,
            "  {:<38}{:<22}{}",
            budget.what,
            budget.measured.as_deref().unwrap_or("not measured yet"),
            budget.allowed.unwrap_or("see docs/budgets.md"),
        );
    }

    section(&mut out, "Index, by source");
    if parts.by_source.is_empty() {
        let _ = writeln!(out, "  nothing indexed");
    }
    for (mode, count) in parts.by_source {
        let _ = writeln!(out, "  {mode:<34}{}", thousands(*count as u64));
    }

    section(&mut out, "Extensions");
    if parts.extensions.is_empty() {
        let _ = writeln!(out, "  none installed");
    }
    for (id, title, commands) in parts.extensions {
        let _ = writeln!(out, "  {id:<44}{title:<34}{commands} commands");
    }

    section(&mut out, "Timings");
    let _ = writeln!(
        out,
        "  {:<34}{}",
        "Summons measured",
        parts.timings.summons.len()
    );
    for summon in &parts.timings.summons {
        let _ = writeln!(
            out,
            "    shown in {} ms, painted in {}",
            summon.shown_ms,
            summon
                .painted_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "never".to_string()),
        );
    }

    costs(&mut out, "Search sources", &parts.timings.sources);
    costs(&mut out, "Extensions opened", &parts.timings.extensions);
    costs(&mut out, "Drawn by the window", &parts.timings.paints);

    section(&mut out, "Not working");
    if parts.troubles.is_empty() {
        let _ = writeln!(out, "  nothing is being reported");
    }
    for trouble in parts.troubles {
        let _ = writeln!(
            out,
            "  {}: {}",
            trouble.id,
            parts.scrub.text(&trouble.message)
        );
    }

    if let Some(crash) = parts.crash {
        section(&mut out, "Last crash");
        let _ = writeln!(out, "{}", parts.scrub.text(crash));
    }

    section(&mut out, "Log");
    let tail = tail(parts.log, LOG_TAIL_BYTES);
    if tail.len() < parts.log.len() {
        let _ = writeln!(out, "  (the end of it; the earlier part is in sill.log)\n");
    }
    let _ = writeln!(out, "{}", parts.scrub.text(tail));

    section(&mut out, "What this holds back");
    for line in HELD_BACK {
        let _ = writeln!(out, "  {line}");
    }

    out
}

/// What the bundle says it left out.
///
/// Written down rather than merely done, because the reader is being asked to
/// send a file about their own machine to a stranger. A list of what is not in
/// it is the only thing that makes that a reasonable request, and it is also
/// the review checklist for anybody adding a section above.
const HELD_BACK: &[&str] = &[
    "Preferences, including the provider keys and the MCP token sealed in them.",
    "Clipboard history, including anything Sill marked confidential.",
    "Extension storage and extension preferences, passwords in them included.",
    "The file index, which is a list of what is on this machine.",
    "Anything typed into the launcher, and anything asked of the assistant.",
    "Which files, windows or pages were opened.",
    "In the log above: home folder paths, sealed values, and anything shaped",
    "like a key or a token, each replaced with a marker.",
];

fn section(out: &mut String, name: &str) {
    let _ = write!(out, "\n{name}\n{}\n", "-".repeat(name.len()));
}

fn costs(out: &mut String, name: &str, costs: &[crate::timing::Cost]) {
    let _ = writeln!(out, "  {name}");

    if costs.is_empty() {
        let _ = writeln!(out, "    nothing measured this session");
        return;
    }

    for cost in costs {
        let _ = writeln!(
            out,
            "    {:<32}{:>6} calls{:>12} us average{:>12} us worst",
            cost.name,
            cost.count,
            cost.average_us(),
            cost.slowest_us,
        );
    }
}

/// The last `most` bytes, cut at a line boundary so nothing starts mid-word.
fn tail(text: &str, most: usize) -> &str {
    if text.len() <= most {
        return text;
    }

    let from = text.len() - most;
    // Forward to the next line, and to a character boundary either way: the
    // cut lands wherever it lands in a file of arbitrary bytes.
    match text[from..].find('\n') {
        Some(at) => &text[from + at + 1..],
        None => text.get(from..).unwrap_or(text),
    }
}

/// `12,043`, because an index size is read rather than compared.
fn thousands(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);

    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }

    out
}

/**
Takes the identifying and the secret-shaped out of text.

Two jobs, and the second is the one that has to be paranoid.

**Who this machine belongs to.** The data folder, and every path in the log
underneath it, begins with the home directory, which contains the account name.
That is replaced first, so the account name is gone from the paths before the
bare-name pass even looks.

**Anything shaped like a credential.** The log is written by every part of the
application and nobody can promise what a line added next year will put in it,
so this does not work from a list of the things Sill happens to log today. It
looks at every run of credential-shaped characters and takes out the ones that
could be a key: the marker `secrets.rs` seals with, the prefixes the providers
use, and anything long enough and mixed enough to be an opaque token.

It takes too much rather than too little, deliberately. A twenty-six character
identifier with a digit in it is redacted along with the key it resembles, and
a bundle where somebody has to ask what a masked run was costs a message. The
other way costs a credential.
*/
pub struct Scrub {
    /// The home directory, lowercased for matching.
    home: Option<String>,
    /// The account name, when it is long enough to match on safely.
    ///
    /// A two-letter account name appears inside ordinary words, and a bundle
    /// with every "jo" replaced is unreadable. Short names lose the bare-name
    /// pass and keep the path pass, which is where they actually appear.
    user: Option<String>,
}

impl Scrub {
    /// Builds one from the home directory, which is where both facts are.
    pub fn new(home: Option<&std::path::Path>) -> Self {
        let home = home.map(|path| path.to_string_lossy().to_ascii_lowercase());

        let user = home
            .as_deref()
            .and_then(|home| home.rsplit(['\\', '/']).next())
            .filter(|name| name.len() >= 3)
            .map(str::to_string);

        Self { home, user }
    }

    /// The text, with the identifying and the secret-shaped taken out.
    pub fn text(&self, text: &str) -> String {
        let without_home = self.paths(text);
        let without_names = self.names(&without_home);
        secrets(&without_names)
    }

    /// Home directory paths, replaced wherever they appear.
    ///
    /// Case-insensitively and on both separators, because Windows writes a
    /// path either way and matching only the exact spelling would let the
    /// other one through.
    fn paths(&self, text: &str) -> String {
        let Some(home) = self.home.as_deref() else {
            return text.to_string();
        };

        // Lowercased as ASCII only, deliberately. A general lowercasing can
        // change a string's length, and every offset below is a byte offset
        // into the original: one Turkish dotted capital in a log line would
        // slice the text apart in the wrong places.
        let looking = text.to_ascii_lowercase().replace('/', "\\");
        let home = home.replace('/', "\\");

        let mut out = String::with_capacity(text.len());
        let mut at = 0;

        while let Some(found) = looking[at..].find(&home) {
            let from = at + found;
            out.push_str(&text[at..from]);
            out.push_str("%USERPROFILE%");
            at = from + home.len();
        }

        out.push_str(&text[at..]);
        out
    }

    /// The account name on its own, for the places a path does not reach.
    fn names(&self, text: &str) -> String {
        let Some(user) = self.user.as_deref() else {
            return text.to_string();
        };

        let looking = text.to_ascii_lowercase();
        let mut out = String::with_capacity(text.len());
        let mut at = 0;

        while let Some(found) = looking[at..].find(user) {
            let from = at + found;
            let to = from + user.len();

            // Only where it is a word of its own. Otherwise an account name
            // that happens to be a common word rewrites half the log.
            let bounded = !boundary(&looking, from.wrapping_sub(1)) && !boundary(&looking, to);

            out.push_str(&text[at..from]);
            out.push_str(if bounded { "<user>" } else { &text[from..to] });
            at = to;
        }

        out.push_str(&text[at..]);
        out
    }
}

/// Whether the byte at `at` is part of a word, treating out of range as not.
fn boundary(text: &str, at: usize) -> bool {
    text.as_bytes()
        .get(at)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

/// Characters a credential is made of.
///
/// No `.` and no `:`, which is what keeps file names, module paths and version
/// numbers readable: they break a run into pieces too short to look like
/// anything. `-` and `_` are in, because real keys use both.
fn credentialish(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'_' | b'-')
}

/// Everything that looks like a key, a token or a sealed value, masked.
fn secrets(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;

    while at < bytes.len() {
        // A sealed value keeps its marker so the reader knows what was there.
        if text[at..].starts_with(SEALED) {
            out.push_str(SEALED);
            at = mask_run(bytes, at + SEALED.len(), &mut out);
            continue;
        }

        // `Bearer <anything>` is a token by definition, whatever it looks
        // like, so it does not have to earn its way past the shape rules.
        if bytes.len() >= at + 7 && bytes[at..at + 7].eq_ignore_ascii_case(b"bearer ") {
            out.push_str(&text[at..at + 7]);
            at = mask_run(bytes, at + 7, &mut out);
            continue;
        }

        if !credentialish(bytes[at]) {
            // Pushed as a str slice so multi-byte characters survive whole.
            let width = char_width(bytes, at);
            out.push_str(&text[at..at + width]);
            at += width;
            continue;
        }

        let mut to = at;
        while to < bytes.len() && credentialish(bytes[to]) {
            to += 1;
        }

        let run = &text[at..to];
        out.push_str(if looks_like_a_secret(run) { MASK } else { run });
        at = to;
    }

    out
}

/// Masks the credential run starting at `at`, and says where it ends.
///
/// For the two cases where what came before is proof enough: a sealed marker
/// and a `Bearer`. Neither has to look like anything.
fn mask_run(bytes: &[u8], at: usize, out: &mut String) -> usize {
    let mut to = at;
    while to < bytes.len() && credentialish(bytes[to]) {
        to += 1;
    }

    if to > at {
        out.push_str(MASK);
    }

    to
}

/// How many bytes the character starting here occupies.
fn char_width(bytes: &[u8], at: usize) -> usize {
    match bytes[at] {
        byte if byte < 0x80 => 1,
        byte if byte >> 5 == 0b110 => 2,
        byte if byte >> 4 == 0b1110 => 3,
        _ => 4,
    }
}

/// Prefixes that are a credential whatever follows them.
///
/// Matched case-sensitively, because each of these is issued in exactly this
/// spelling and matching loosely would catch ordinary words.
const KEY_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "pk_",
    "rk_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "AIza",
    "AKIA",
    "ASIA",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xoxs-",
    "xapp-",
    "eyJ",
    "hf_",
    "gsk_",
    "csk-",
    "sq0atp-",
    "SG-",
    "SG.",
];

/// The shortest run that can be mistaken for a key.
///
/// Twenty, because that is the length of an AWS access key id, which is the
/// shortest thing here anybody would call a credential.
const SHORTEST_KEY: usize = 20;

/// The shortest run of hex that is worth treating as a token.
const SHORTEST_HEX: usize = 32;

/// Whether a run of credential characters could be a credential.
fn looks_like_a_secret(run: &str) -> bool {
    // A named prefix is enough on its own, as long as something follows it.
    if KEY_PREFIXES
        .iter()
        .any(|prefix| run.len() > prefix.len() + 4 && run.starts_with(prefix))
    {
        return true;
    }

    let digits = run.bytes().any(|b| b.is_ascii_digit());
    let letters = run.bytes().any(|b| b.is_ascii_alphabetic());

    /*
     * Long, with letters and digits mixed into each other.
     *
     * The false positive worth avoiding is prose: words joined by underscores,
     * a module path, a long English identifier. Those have no digit in them,
     * which is what the digit requirement is doing here. A bare number is not
     * a key either, which is what the letter requirement is doing.
     *
     * What survives both is an identifier with a digit in it, and a
     * twenty-six character one of those does get masked. That is the trade
     * this file states at the top: over-redaction costs a question, and
     * under-redaction costs a credential.
     */
    if run.len() >= SHORTEST_KEY && digits && letters {
        return true;
    }

    // A long run of hex is a hash or a token either way.
    run.len() >= SHORTEST_HEX && run.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrub() -> Scrub {
        Scrub::new(Some(std::path::Path::new("C:\\Users\\brandon")))
    }

    /// Four shapes of real credential, and none of them survives.
    #[test]
    fn every_shape_of_key_is_taken_out() {
        let seeded = [
            "sk-ant-api03-Zx91RtqLm4Vb8NcPw2KdHs7Yj0Fa5Gu3",
            "ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8",
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyD9x3Kq7Lm2Nv8Pw4Rt6Yu1Bc5Ef0Gh2Ij",
            "xoxb-1234567890-0987654321-AbCdEfGhIjKlMnOpQrSt",
            "a3f5b8c2d1e409876543210fedcba98765432100a",
            // No prefix anybody has written down, and not hex either. This is
            // the one the shape rule has to catch on its own, and without it
            // every other entry here is caught by a name in `KEY_PREFIXES` and
            // the shape rule is never exercised at all.
            "Xq7Lm2Nv8Pw4Rt6Yu1Bc5Ef0Gh3Jk",
        ];

        for key in seeded {
            let scrubbed = scrub().text(&format!("the provider said {key} was wrong"));

            assert!(
                !scrubbed.contains(key),
                "{key} came through the scrub as {scrubbed}"
            );
            assert!(scrubbed.contains(MASK), "nothing was masked in {scrubbed}");
        }
    }

    /// A sealed value keeps its marker and loses its payload.
    ///
    /// The marker is worth keeping: "there was a sealed key here" is a useful
    /// thing for a bundle to say, and it is not the key.
    #[test]
    fn a_sealed_value_keeps_its_marker_and_nothing_else() {
        let scrubbed = scrub().text("read dpapi:v1:AQAAANCMnd8BFdERjHoAwE7Cl+sBAAAAxyz from disk");

        assert!(scrubbed.contains("dpapi:v1:"));
        assert!(!scrubbed.contains("AQAAANCMnd8"));
    }

    /// Even a short sealed value, which is too short to look like anything.
    #[test]
    fn a_short_sealed_value_goes_too() {
        let scrubbed = scrub().text("dpapi:v1:AQAAAA==");

        assert!(!scrubbed.contains("AQAAAA"), "{scrubbed}");
    }

    #[test]
    fn a_bearer_token_goes_however_short_it_is() {
        let scrubbed = scrub().text("Authorization: Bearer abc123");

        assert!(!scrubbed.contains("abc123"), "{scrubbed}");
    }

    /// Whose machine it is does not go either.
    #[test]
    fn the_home_folder_and_the_account_name_both_go() {
        let scrubbed = scrub().text(
            "could not read C:\\Users\\Brandon\\AppData\\Roaming\\app.winters.sill, \
             owner brandon",
        );

        assert!(!scrubbed.to_lowercase().contains("brandon"), "{scrubbed}");
        assert!(scrubbed.contains("%USERPROFILE%"));
        assert!(scrubbed.contains("<user>"));
        // The rest of the path is what makes the line worth having.
        assert!(scrubbed.contains("AppData\\Roaming\\app.winters.sill"));
    }

    /// A path written the other way round is still a path.
    #[test]
    fn a_forward_slash_home_path_is_matched_too() {
        let scrubbed = scrub().text("watching C:/Users/Brandon/Documents");

        assert!(!scrubbed.to_lowercase().contains("brandon"), "{scrubbed}");
    }

    /// Ordinary log text survives, or the bundle is worth nothing.
    #[test]
    fn a_log_that_says_something_still_says_it() {
        let ordinary = "summon 31 ms (9 to show, 22 to paint), \
                        could not start the clipboard watcher: access denied, \
                        index rebuilt with 12043 entries, sill.previous.log";

        assert_eq!(scrub().text(ordinary), ordinary);
    }

    /// A word made of words is not a key, however long it is.
    #[test]
    fn a_long_identifier_is_not_mistaken_for_a_key() {
        let ordinary = "a_second_press_does_not_steal_the_first_ones_paint";

        assert_eq!(scrub().text(ordinary), ordinary);
    }

    /// Non-ASCII text is copied through whole rather than cut mid-character.
    #[test]
    fn a_line_with_accents_in_it_survives() {
        let ordinary = "could not open Café Résumé — naïve";

        assert_eq!(scrub().text(ordinary), ordinary);
    }

    /**
    The bundle carries no secret, whatever the log had in it.

    The one test this feature exists for. A bundle is a file somebody sends to
    somebody else, so the question is not whether the code meant to include a
    credential; it is whether one could arrive through the only part of the
    bundle nobody controls, which is the log.

    So the log here is seeded with what actually lives on this machine: a
    DPAPI-sealed value of the shape `preferences.json` holds, the loopback
    token the MCP bridge uses, four provider keys in their real spellings, and
    a home path with the account name in it.
    */
    #[test]
    fn a_bundle_carries_no_secret() {
        let sealed = "dpapi:v1:AQAAANCMnd8BFdERjHoAwE7Cl+sBAAAAlom2z";
        let anthropic = "sk-ant-api03-Zx91RtqLm4Vb8NcPw2KdHs7Yj0Fa5Gu3";
        let github = "ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8";
        let mcp = "b7d41f0a9c2e83615d4a70b8e9f2c1a3";
        let aws = "AKIAIOSFODNN7EXAMPLE";
        // A provider nobody has added a prefix for yet, which is the case the
        // shape rule exists for and the only entry here that reaches it.
        let unknown = "Xq7Lm2Nv8Pw4Rt6Yu1Bc5Ef0Gh3Jk";
        let home = "C:\\Users\\brandon\\AppData\\Roaming";

        let log = format!(
            "read {sealed} from preferences\n\
             the bridge token is {mcp}\n\
             provider refused {anthropic}\n\
             store refused {github}\n\
             upload refused {aws}\n\
             some other provider refused {unknown}\n\
             could not write {home}\\sill.log\n"
        );

        let timings = Report {
            cold_start_ms: Some(846),
            summons: Vec::new(),
            median_ms: None,
            sources: Vec::new(),
            extensions: Vec::new(),
            paints: Vec::new(),
        };

        let scrub = Scrub::new(Some(std::path::Path::new("C:\\Users\\brandon")));

        let built = assemble(&Parts {
            version: "0.1.0",
            when: "2026-09-03 17:40",
            level: crate::log::Level::Normal,
            facts: &[("Data folder", format!("{home}\\app.winters.sill"))],
            budgets: &budgets(Some(24 * 1024 * 1024), &timings),
            by_source: &[("file".to_string(), 12_043)],
            extensions: &[("raycast/clipboard".to_string(), "Clipboard".to_string(), 4)],
            timings: &timings,
            troubles: &[Trouble {
                id: "clipboard:blob".to_string(),
                message: format!("could not write a copied picture to {home}"),
                section: None,
            }],
            crash: Some(&format!("panicked reading {sealed}")),
            log: &log,
            scrub: &scrub,
        });

        for secret in [sealed, anthropic, github, mcp, aws, unknown] {
            assert!(
                !built.contains(secret),
                "the bundle carries {secret}, which is the one thing it must not"
            );
        }

        // The payload alone, in case a marker was kept and the value with it.
        assert!(!built.contains("AQAAANCMnd8"), "a sealed payload survived");
        assert!(
            !built.to_lowercase().contains("brandon"),
            "the bundle names whose machine it is"
        );

        // And it is still worth sending.
        assert!(built.contains("12,043"));
        assert!(built.contains("846 ms"));
        assert!(built.contains("raycast/clipboard"));
        assert!(built.contains("clipboard:blob"));
    }

    /// A bundle says what it left out, which is what makes it sendable.
    #[test]
    fn a_bundle_says_what_it_held_back() {
        let timings = Report {
            cold_start_ms: None,
            summons: Vec::new(),
            median_ms: None,
            sources: Vec::new(),
            extensions: Vec::new(),
            paints: Vec::new(),
        };

        let scrub = Scrub::new(None);
        let built = assemble(&Parts {
            version: "0.1.0",
            when: "now",
            level: crate::log::Level::Normal,
            facts: &[],
            budgets: &budgets(None, &timings),
            by_source: &[],
            extensions: &[],
            timings: &timings,
            troubles: &[],
            crash: None,
            log: "",
            scrub: &scrub,
        });

        for expected in ["Clipboard history", "file index", "Preferences"] {
            assert!(
                built.contains(expected),
                "the bundle does not say it holds back {expected}, so nobody \
                 reading it can tell whether it is safe to send"
            );
        }
    }

    /// A long log is cut to its end, which is the part that explains a fault.
    #[test]
    fn only_the_end_of_a_long_log_is_carried() {
        let long = format!("{}\nthe last line\n", "filler line\n".repeat(40_000));
        let carried = tail(&long, LOG_TAIL_BYTES);

        assert!(carried.len() <= LOG_TAIL_BYTES);
        assert!(carried.contains("the last line"));
        assert!(carried.starts_with("filler line"), "cut mid-line");
    }

    #[test]
    fn a_short_log_is_carried_whole() {
        assert_eq!(tail("all of it\n", LOG_TAIL_BYTES), "all of it\n");
    }

    #[test]
    fn an_index_size_is_written_to_be_read() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(12_043), "12,043");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }
}
