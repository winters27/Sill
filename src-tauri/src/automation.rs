/*!
Triggers, held by the scheduler Windows already has.

Every launcher that grew automations grew a scheduler with it: a thread that
wakes, reads a list, finds nothing due and sleeps again. Rule 23 makes idle
cost a product requirement and `P2-09` measured this one at 0.00% of a core,
so a loop bought for a feature most people use twice a week is a permanent
cost paid for an occasional benefit.

Windows already runs that loop, in a service that is running whether Sill is
or not, and it is better than one written here: it survives a restart, it
knows about sleep and battery, and it is visible in a tool the person already
has. So a trigger **is** a scheduled task, and the whole of Sill's part is
writing one down and reading it back. Nothing in this file starts a thread,
arms a timer or holds a handle. The functions run when somebody opens the
settings panel and at no other moment.

## What runs, and how it gets in

The task's action is `sill.exe run <action> <target>`, which is the command
line [`crate::outside`] already answers to. Windows starts it, the single
instance plugin hands the line to the running Sill, [`crate::reach::asked_of`]
reads it and [`crate::action::ActionRegistry::perform`] runs it. **There is no
second path**, and this module deliberately contains no `perform` of its own:
an automation is a caller of the one door, not a door.

## Which trust a task arrives under, and why it is `Shell`

[`crate::reach::Trust`] has two levels and this adds none. A task is
`Trust::Shell`, and the argument is about authority rather than convenience.

A `sill://` link is written by whoever wrote the page it sits on, which is
somebody who is not at the machine, so it always asks. A task's command line
is written by whatever could register or rewrite a task in this user's own
scheduler, and that is exactly the authority needed to start `sill.exe` at
all. Anything holding it could have run the program itself, which is the
sentence [`crate::reach::may_run`] already uses about a shell. A third trust
level would be a third rule saying the same thing as one of the first two, and
the one that goes stale is always the one nobody reaches.

There is also no honest way to tell them apart. Both arrive as argv, started
by a process running as this user, and a marker in the command line saying
"this one came from a task" would be a claim written in the same text the
attacker is editing.

## Where the Windows Hello gate sits

`P8-03` put `ShellExecution` and `FileWrite` behind Windows Hello for the AI
and MCP callers, falling back to the approval card where Hello is absent. An
automation is a third caller and the honest answer about a gate is that **a
trigger firing at three in the morning has nobody to ask**. Both directions
out of that are bad: running unasked makes the gate theatre, and asking makes
the feature a card nobody answers, refused after ninety seconds and filed in
the status surface as something that failed.

So neither. The gate did not move and was not weakened; the set of things a
trigger may name was narrowed to the complement of the set the gate covers.
[`may_schedule`] refuses any action that [`crate::ai::acting::needs_asking`]
would stop for, and the two capabilities Hello covers are a subset of those,
so **the Hello gate is unreachable from this path by construction**. Two rules
cannot disagree when one of them is never reached.

What that costs is worth being plain about: an automation can open things,
read files and touch the clipboard, and it cannot run a script or write a
file. That is a smaller feature than the one somebody might have wanted, and
it is the only version of it that is true at three in the morning.

## What is read back is not believed

The task store is not Sill's. Its contents are a file the scheduler service
writes on behalf of anything running as this user, so a command line that went
in saying one thing can come out saying another. [`read_back`] treats every
task in the folder as text somebody else wrote: the command must be this
exact `sill.exe`, the arguments must parse back into the same [`Ask`] shape
[`crate::reach`] would produce, and the action must still pass
[`may_schedule`]. Anything else is listed as suspect, with what it actually
says, and offered for removal rather than described in Sill's own words.

The tampering is not, on its own, an escalation. A rewritten task naming
`sill.runScript` reaches `outside.rs` as an ordinary `Trust::Shell` ask, stops
at the card because `ShellExecution` asks, and is refused when nobody answers.
The reason to catch it here anyway is the listing: a panel that laundered a
rewritten task into "Open my notes" would be Sill lying about the machine.

## What Windows holds and Sill does not

Everything. The schedule, the command line, the enabled flag and the next run
time all live in Task Scheduler, and [`held`] asks it rather than keeping a
copy. There is no automation section in preferences and nothing to keep in
step, which is rule 5 arriving from the cheapest possible direction: a second
copy that cannot exist cannot drift.

The cost of that is the one thing this feature genuinely changes about the
machine. A task outlives Sill's process, survives a reboot, and survives
uninstall unless something removes it. `P6-07` has to delete the whole
[`FOLDER`] folder and every task in it.
*/

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::action::Capability;
use crate::reach::{Ask, Trust};

/**
The Task Scheduler folder every trigger Sill makes lives in.

One folder, named after the program, for three reasons. Somebody opening Task
Scheduler and wondering what put a task there can read the answer off the
tree. [`held`] enumerates one folder rather than filtering every task on the
machine by guessing at its command line. And [`forget`] can refuse to touch
anything outside it, so no bug in this file can delete somebody else's task.
*/
pub const FOLDER: &str = "Sill";

/// The folder's full path, which is what Task Scheduler wants.
pub const FOLDER_PATH: &str = r"\Sill";

/**
When Windows should start it.

Three, and all three are things Task Scheduler expresses on its own with no
query language in the middle. An event subscription is the fourth kind the
audit item names and it is deliberately not here yet: its useful form is an
XPath against a named channel, which is a small language Sill would be
handing to a person who has never seen it, and getting it wrong produces a
trigger that silently never fires. A session change is the same family of
thing without that problem.
*/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum When {
    /// Every day at a time.
    Daily { hour: u8, minute: u8 },
    /// When this account signs in.
    AtLogon,
    /// When this account unlocks the machine.
    OnUnlock,
    /**
    Once, at a moment, and then never again.

    The one that leaves nothing behind. A task with no end has to be removed by
    whoever made it, and a timer nobody removes is a folder that grows by one
    every time somebody boils an egg. This carries an end boundary a minute
    after it starts and asks Windows to delete it once that has passed, so the
    scheduler holds a one-off timer for exactly as long as the timer runs.

    See [`crate::timers`], which is the only thing that makes one.
    */
    Once { at: crate::timers::Local },
}

impl When {
    /// The sentence a panel and a task description both use.
    pub fn said(self) -> String {
        match self {
            Self::Daily { hour, minute } => format!("every day at {hour:02}:{minute:02}"),
            Self::AtLogon => "when you sign in".to_string(),
            Self::OnUnlock => "when you unlock this PC".to_string(),
            Self::Once { at } => format!("once, at {}", at.clock()),
        }
    }

    /// Whether a clock time is a clock time.
    ///
    /// Checked here rather than trusted from the window, because the value
    /// reaches Windows as text inside XML and an hour of 99 makes a task that
    /// registers and never runs.
    fn sound(self) -> Result<(), String> {
        match self {
            Self::Daily { hour, minute } if hour > 23 || minute > 59 => {
                Err(format!("There is no {hour:02}:{minute:02} in a day."))
            }
            // The same check, on the moment a one-off fires. It reaches
            // Windows as text inside XML too, and a 32nd of September makes a
            // task that registers and then never runs.
            Self::Once { at } if !at.is_a_moment() => {
                Err(format!("There is no such moment as {}.", at.boundary()))
            }
            _ => Ok(()),
        }
    }
}

/// A trigger, as somebody described it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    /// What it is called, which is also the task's name in the folder.
    pub name: String,
    /// The registry id it runs. Never matched by prefix.
    pub action: String,
    pub target: String,
    pub kind: Option<String>,
    pub argument: Option<String>,
    pub when: When,
}

/**
Whether this action may be put on a schedule at all.

**The whole security decision, and it is a narrowing rather than a gate.** A
gate is something a person answers; there is no person here, which is the
entire difficulty this feature has. So instead of choosing what to do when the
card cannot be shown, nothing that would show one may be scheduled.

Reusing [`crate::ai::acting::needs_asking`] rather than writing a list is the
point, exactly as `outside.rs` does for a shell. A capability added later is
answered by the same rule without anybody remembering this file exists, and a
list of ids kept here would be a list a new action joins by default.
*/
pub fn may_schedule(id: &str, capabilities: &[Capability]) -> Result<(), String> {
    if crate::ai::acting::needs_asking(capabilities) {
        return Err(format!(
            "Sill will not schedule {id}, because it {}. A trigger fires when \
             nobody is at the machine, so an action that stops to ask has \
             nobody to ask.",
            crate::ai::acting::what_it_touches(capabilities),
        ));
    }

    Ok(())
}

/// The characters a task name may not hold.
///
/// One spelling, read by both the check and the repair. Two lists of awkward
/// characters is the shape that lets [`sanitised_name`] produce something
/// [`task_name`] then refuses, which would be a reminder failing to set with a
/// message about a name nobody typed.
const REFUSED: &str = r#"\/:*?"<>|"#;

/// How long a name may be.
///
/// Task Scheduler takes far more. This is short enough to read in a list and
/// long enough for a sentence, and a name is a label rather than a note.
const NAME_AT_MOST: usize = 60;

/**
The task's name in the folder, if this is one.

**A name is a path segment, and that is the whole reason this is checked.** A
name containing a backslash registers a task somewhere other than [`FOLDER`],
which puts it outside the one place `remove` is willing to look and outside
the one place `P6-07` will know to clean up. The rest of the refused
characters are the ones Task Scheduler rejects itself, refused here so the
sentence is Sill's rather than a COM error code.

Leading and trailing whitespace is trimmed rather than refused, because it is
a typo and not a decision.
*/
pub fn task_name(name: &str) -> Result<String, String> {
    let name = name.trim();

    if name.is_empty() {
        return Err("Give the trigger a name.".to_string());
    }

    if name.chars().count() > NAME_AT_MOST {
        return Err(format!(
            "A trigger's name has to be {NAME_AT_MOST} characters or fewer."
        ));
    }

    // A path separator is the one that matters. The rest come with it because
    // Windows refuses them in a task name anyway and a refusal in Sill's own
    // words is worth more than the same refusal from the scheduler.
    if let Some(bad) = name.chars().find(|c| REFUSED.contains(*c)) {
        return Err(format!("A trigger's name cannot contain {bad}."));
    }

    // A control character is invisible on screen, so two names that read the
    // same would be two different tasks and removing one would look broken.
    if name.chars().any(char::is_control) {
        return Err("A trigger's name cannot contain control characters.".to_string());
    }

    Ok(name.to_string())
}

/**
What Windows will start, as the argv it becomes.

The same words somebody would type, which is the point: this is not a private
protocol between the scheduler and Sill, it is `sill run` with a clock in
front of it. The program name is index zero because that is what
[`crate::reach::asked_of`] skips, so the vector this returns is exactly the
one the running Sill will read.
*/
pub fn command_line(exe: &Path, trigger: &Trigger) -> Vec<String> {
    let mut argv = vec![
        exe.display().to_string(),
        "run".to_string(),
        trigger.action.clone(),
        trigger.target.clone(),
    ];

    if let Some(kind) = &trigger.kind {
        argv.push("--kind".to_string());
        argv.push(kind.clone());
    }

    if let Some(argument) = &trigger.argument {
        argv.push("--argument".to_string());
        argv.push(argument.clone());
    }

    argv
}

/**
An XML document that registers the trigger, or the reason it cannot.

Pure, including the date. A daily trigger uses only the time of day out of its
start boundary, so reading the clock would buy nothing and would make every
fixture below a test of what time it is when it runs.
*/
pub fn definition(exe: &Path, trigger: &Trigger) -> Result<String, String> {
    task_name(&trigger.name)?;
    trigger.when.sound()?;

    let argv = command_line(exe, trigger);
    let arguments = joined(&argv[1..]);
    let described = format!("Sill runs {} {}.", trigger.action, trigger.when.said());

    Ok(format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n",
            "<Task version=\"1.2\" ",
            "xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n",
            "  <RegistrationInfo>\n",
            "    <Author>Sill</Author>\n",
            "    <Description>{described}</Description>\n",
            "  </RegistrationInfo>\n",
            "  <Triggers>\n{triggers}  </Triggers>\n",
            "  <Principals>\n",
            "    <Principal id=\"Author\">\n",
            "      <LogonType>InteractiveToken</LogonType>\n",
            "      <RunLevel>LeastPrivilege</RunLevel>\n",
            "    </Principal>\n",
            "  </Principals>\n",
            "  <Settings>\n",
            "    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n",
            "    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n",
            "    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n",
            "    <AllowHardTerminate>false</AllowHardTerminate>\n",
            "    <StartWhenAvailable>false</StartWhenAvailable>\n",
            "    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n",
            "    <AllowStartOnDemand>true</AllowStartOnDemand>\n",
            "    <Enabled>true</Enabled>\n",
            "    <Hidden>false</Hidden>\n",
            "    <RunOnlyIfIdle>false</RunOnlyIfIdle>\n",
            "    <WakeToRun>false</WakeToRun>\n",
            "    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n",
            "{expiry}",
            "    <Priority>7</Priority>\n",
            "  </Settings>\n",
            "  <Actions Context=\"Author\">\n",
            "    <Exec>\n",
            "      <Command>{command}</Command>\n",
            "      <Arguments>{arguments}</Arguments>\n",
            "    </Exec>\n",
            "  </Actions>\n",
            "</Task>\n",
        ),
        described = escaped(&described),
        triggers = when_xml(trigger.when),
        expiry = expiry_xml(trigger.when),
        command = escaped(&argv[0]),
        arguments = escaped(&arguments),
    ))
}

/// The trigger element, indented to sit inside `<Triggers>`.
fn when_xml(when: When) -> String {
    match when {
        /*
         * A start boundary in the past, with the wanted time of day on it.
         *
         * Task Scheduler reads the time out of this and the date only decides
         * when the schedule became active, so any past date does. A fixed one
         * keeps this function pure.
         */
        When::Daily { hour, minute } => format!(
            "    <CalendarTrigger>\n\
             \x20     <StartBoundary>2000-01-01T{hour:02}:{minute:02}:00</StartBoundary>\n\
             \x20     <Enabled>true</Enabled>\n\
             \x20     <ScheduleByDay>\n\
             \x20       <DaysInterval>1</DaysInterval>\n\
             \x20     </ScheduleByDay>\n\
             \x20   </CalendarTrigger>\n",
        ),
        When::AtLogon => {
            "    <LogonTrigger>\n      <Enabled>true</Enabled>\n    </LogonTrigger>\n".to_string()
        }
        When::OnUnlock => "    <SessionStateChangeTrigger>\n      \
                           <Enabled>true</Enabled>\n      \
                           <StateChange>SessionUnlock</StateChange>\n    \
                           </SessionStateChangeTrigger>\n"
            .to_string(),
        /*
         * A moment, and an end a minute after it.
         *
         * The end boundary is not decoration. `DeleteExpiredTaskAfter` only
         * applies to a task that can expire, and a `TimeTrigger` with no end
         * never does, so without this pair a one-off timer would sit in the
         * folder for good. A minute is long enough for Task Scheduler to start
         * the task and short enough that a machine which was asleep at the
         * moment comes back to a reminder that has quietly expired rather than
         * to one that is hours stale.
         */
        When::Once { at } => {
            let ends = crate::timers::fires_at(at, std::time::Duration::from_secs(60));

            format!(
                "    <TimeTrigger>\n\
                 \x20     <StartBoundary>{start}</StartBoundary>\n\
                 \x20     <EndBoundary>{end}</EndBoundary>\n\
                 \x20     <Enabled>true</Enabled>\n\
                 \x20   </TimeTrigger>\n",
                start = at.boundary(),
                end = ends.boundary(),
            )
        }
    }
}

/// The setting that has Windows tidy a one-off away once it has run.
///
/// Empty for everything else, and that is the point rather than an omission: a
/// daily trigger has no end boundary, so it can never expire, and asking for it
/// to be deleted when it does would be a line that never means anything.
fn expiry_xml(when: When) -> &'static str {
    match when {
        When::Once { .. } => "    <DeleteExpiredTaskAfter>PT0S</DeleteExpiredTaskAfter>\n",
        When::Daily { .. } | When::AtLogon | When::OnUnlock => "",
    }
}

/**
A name Task Scheduler will take, out of words somebody wrote.

[`task_name`] refuses rather than repairs, which is right for a name typed into
a form: somebody who put a colon in one should be told so. A reminder's name is
not typed, it is built out of the reminder's own message, so refusing would
mean `timer 5m call the 9:30 client` produced nothing and blamed the message.

Here rather than beside the caller, so that what a task may be called is one
answer in one file. The round trip is what the test holds: whatever this
returns, [`task_name`] accepts.
*/
pub fn sanitised_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if REFUSED.contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();

    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = collapsed.chars().take(NAME_AT_MOST).collect();
    let trimmed = short.trim().to_string();

    if trimmed.is_empty() {
        "Reminder".to_string()
    } else {
        trimmed
    }
}

/// The five characters that would otherwise end an element early.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }

    out
}

/// The same five, read back.
///
/// `&amp;` last on the way in and first on the way out would double-decode
/// `&amp;lt;` into `<`, so the ampersand is handled by scanning rather than by
/// replacing in an order somebody has to remember.
fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];

        let found = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
        ]
        .into_iter()
        .find(|(name, _)| tail.starts_with(name));

        match found {
            Some((name, c)) => {
                out.push(c);
                rest = &tail[name.len()..];
            }
            // Not an entity Sill wrote. Kept as it is rather than dropped,
            // because this text is shown to somebody deciding whether a task
            // is theirs and an ampersand quietly vanishing changes what they
            // are reading.
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }

    out.push_str(rest);
    out
}

/// Several arguments, as the one string `<Arguments>` holds.
///
/// Task Scheduler joins the command and the arguments and hands the result to
/// `CreateProcess`, which parses it again, so each part is quoted the way
/// [`crate::shell`] already quotes one. Writing a second quoter here is how
/// `P0-11` happened: two ideas about one rule, and only one of them fixed.
fn joined(argv: &[String]) -> String {
    argv.iter()
        .map(|one| crate::shell::one_argument(one))
        .collect::<Vec<_>>()
        .join(" ")
}

/**
One string, split back into the arguments Windows will make of it.

The inverse of [`joined`], and it is the parser `CommandLineToArgvW`
documents rather than a guess at it: a run of backslashes is literal unless a
quote follows, in which case each pair becomes one and an odd one left over
escapes the quote. Getting this wrong in the safe direction still matters,
because this is what decides whether a task in Sill's folder is described in
Sill's words or shown as something it cannot read.

The program name is **not** included, because Task Scheduler keeps it in a
separate element and never parses it out of this string.
*/
fn split(line: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut one = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut slashes = 0usize;

    /// The run of backslashes seen so far, none of which escaped anything.
    fn push_slashes(one: &mut String, slashes: &mut usize) {
        for _ in 0..*slashes {
            one.push('\\');
        }
        *slashes = 0;
    }

    for c in line.chars() {
        match c {
            '\\' => {
                slashes += 1;
                started = true;
            }
            '"' => {
                // Pairs collapse; an odd one left over makes this quote
                // literal rather than a delimiter.
                let literal = slashes % 2 == 1;
                slashes /= 2;
                push_slashes(&mut one, &mut slashes);
                started = true;

                if literal {
                    one.push('"');
                } else {
                    quoted = !quoted;
                }
            }
            c if c.is_whitespace() && !quoted => {
                push_slashes(&mut one, &mut slashes);

                if started {
                    argv.push(std::mem::take(&mut one));
                    started = false;
                }
            }
            c => {
                push_slashes(&mut one, &mut slashes);
                one.push(c);
                started = true;
            }
        }
    }

    push_slashes(&mut one, &mut slashes);

    if started {
        argv.push(one);
    }

    argv
}

/**
What a task in Sill's folder actually says it will do.

**Nothing here is believed.** The XML arrives from the scheduler service, and
what the service holds is what anything running as this user last put there.
Three things have to line up before a row is described in Sill's own words:
the command has to be this exact `sill.exe`, the arguments have to read back
as a shell ask, and the caller still has to find the action and check
[`may_schedule`] on it.

Deliberately not a full XML parser. The document comes from Windows and is
well formed; the part that is hostile is the text inside two elements, and
reading them by name cannot be fooled into anything worse than a reading this
function then refuses.
*/
pub fn read_back(exe: &Path, xml: &str) -> Result<Ask, String> {
    let Some(command) = element(xml, "Command") else {
        return Err("it does not run a program".to_string());
    };

    let arguments = element(xml, "Arguments").unwrap_or_default();

    // The path, not the file name. A task pointing at a second copy of
    // sill.exe somewhere else is not this Sill's task, whatever it is called.
    if !command.eq_ignore_ascii_case(&exe.display().to_string()) {
        return Err(format!("it runs {command}, which is not this Sill"));
    }

    let mut argv = vec![command];
    argv.extend(split(&arguments));

    match crate::reach::asked_of(&argv) {
        Some(Ok(ask)) if ask.trust == Trust::Shell => Ok(ask),
        // A `sill://` address in a task's arguments would be read as a link
        // by the Sill that received it, which is a different set of rules to
        // the ones this panel would be describing.
        Some(Ok(_)) => Err("it does not run an action the way sill run does".to_string()),
        Some(Err(why)) => Err(why),
        None => Err(format!(
            "it runs sill.exe {arguments}, which asks for nothing"
        )),
    }
}

/// One element's text, unescaped.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");

    let from = xml.find(&open)? + open.len();
    let to = xml[from..].find(&close)? + from;

    Some(unescaped(&xml[from..to]))
}

/// One task as Task Scheduler holds it, before Sill has decided anything.
///
/// The XML comes back whole rather than pre-read, so the reading and the
/// refusing both happen where they can be tested with a fixture instead of a
/// scheduler.
#[derive(Debug, Clone)]
pub struct Held {
    pub name: String,
    pub enabled: bool,
    /// What Windows says happens next, in Windows' own reckoning.
    ///
    /// `None` for a trigger with no next occurrence, which is what a logon
    /// trigger has: there is no date on which signing in is due.
    pub next: Option<String>,
    pub xml: String,
}

#[cfg(windows)]
mod edge {
    use super::{Held, FOLDER, FOLDER_PATH};

    use windows::core::BSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::TaskScheduler::{
        ITaskFolder, ITaskService, TaskScheduler, TASK_CREATE_OR_UPDATE,
        TASK_LOGON_INTERACTIVE_TOKEN,
    };
    use windows::Win32::System::Variant::{VariantTimeToSystemTime, VARIANT, VT_I4};

    /// Runs some COM work with an apartment around it.
    ///
    /// The same shape as [`crate::audio`]'s, deliberately: an apartment that
    /// was already open answers with a failure code that is not an error, and
    /// only a call that opened one may close it.
    fn with_com<T>(work: impl FnOnce() -> windows::core::Result<T>) -> Result<T, String> {
        // SAFETY: initialised and uninitialised on the same thread around the
        // whole call, and every interface is released by its own Drop.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = work();

            if initialised {
                CoUninitialize();
            }

            result.map_err(|err| format!("Task Scheduler refused: {err}"))
        }
    }

    /// A connection to this machine's own scheduler, as this user.
    ///
    /// Every argument empty, which is what connects locally with the calling
    /// account. Naming a user would mean holding a password.
    unsafe fn service() -> windows::core::Result<ITaskService> {
        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_ALL)?;

        service.Connect(
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
        )?;

        Ok(service)
    }

    /// Sill's own folder, made if it is not there yet.
    ///
    /// Creating it is what a first trigger does, and asking for it again once
    /// it exists answers with an error rather than the folder, so the miss is
    /// the ordinary path rather than the exceptional one.
    unsafe fn folder(service: &ITaskService, make: bool) -> windows::core::Result<ITaskFolder> {
        if let Ok(found) = service.GetFolder(&BSTR::from(FOLDER_PATH)) {
            return Ok(found);
        }

        if !make {
            return service.GetFolder(&BSTR::from(FOLDER_PATH));
        }

        let root = service.GetFolder(&BSTR::from("\\"))?;
        root.CreateFolder(&BSTR::from(FOLDER), &VARIANT::default())
    }

    /// Writes one down, replacing whatever answered to that name before.
    ///
    /// Create-or-update rather than create, because a person editing a
    /// trigger they already made is the ordinary case and a second task with
    /// the same intent and a different name is the confusing one.
    pub fn register(name: &str, xml: &str) -> Result<(), String> {
        with_com(|| {
            // SAFETY: every interface comes from the call above it.
            unsafe {
                let service = service()?;
                let folder = folder(&service, true)?;

                folder.RegisterTask(
                    &BSTR::from(name),
                    &BSTR::from(xml),
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::default(),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::default(),
                )?;

                Ok(())
            }
        })
    }

    /// Everything in Sill's folder, whatever put it there.
    ///
    /// An absent folder is an empty list rather than an error: no trigger has
    /// ever been made on this machine, which is not a fault to report.
    pub fn held() -> Result<Vec<Held>, String> {
        with_com(|| {
            // SAFETY: every interface comes from the call above it, and the
            // collection is indexed from one because that is what a COM
            // collection does.
            unsafe {
                let service = service()?;

                let Ok(folder) = folder(&service, false) else {
                    return Ok(Vec::new());
                };

                let tasks = folder.GetTasks(0)?;
                let mut out = Vec::new();

                for at in 1..=tasks.Count()? {
                    let task = tasks.get_Item(&index(at))?;

                    out.push(Held {
                        name: task.Name()?.to_string(),
                        enabled: task.Enabled()?.as_bool(),
                        next: task.NextRunTime().ok().and_then(readable),
                        xml: task.Xml()?.to_string(),
                    });
                }

                Ok(out)
            }
        })
    }

    /// Removes one, and only from Sill's own folder.
    ///
    /// The folder is fetched rather than the name being pasted onto a path,
    /// so a name that somehow carried a separator past [`super::task_name`]
    /// would still be looked for inside this one folder.
    pub fn forget(name: &str) -> Result<(), String> {
        with_com(|| {
            // SAFETY: the folder comes from the service and the name is one
            // segment, checked before it reaches here.
            unsafe {
                let service = service()?;
                let folder = folder(&service, false)?;
                folder.DeleteTask(&BSTR::from(name), 0)
            }
        })
    }

    /// A collection index, as the VARIANT a COM collection is indexed by.
    ///
    /// Built by hand because the crate generates no conversion for it. A
    /// zeroed VARIANT is `VT_EMPTY`, so the whole of making one is naming the
    /// type and filling in the member that type points at.
    fn index(at: i32) -> VARIANT {
        let mut variant = VARIANT::default();

        // SAFETY: the tag and the union member are set together, which is
        // what makes the union readable, and nothing else reads this value.
        unsafe {
            let inner = &mut *variant.Anonymous.Anonymous;
            inner.vt = VT_I4;
            inner.Anonymous.lVal = at;
        }

        variant
    }

    /// An automation DATE, as a date somebody can read.
    ///
    /// Zero means there is no next run, which is what a logon trigger has.
    fn readable(when: f64) -> Option<String> {
        if when == 0.0 {
            return None;
        }

        let mut at = Default::default();

        // SAFETY: `at` is a stack SYSTEMTIME the call fills in, and nothing
        // is read out of it unless the call said it succeeded. The answer is
        // Windows' own BOOL, so zero is the failure.
        let ok = unsafe { VariantTimeToSystemTime(when, &mut at) } != 0;

        ok.then(|| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                at.wYear, at.wMonth, at.wDay, at.wHour, at.wMinute
            )
        })
    }
}

#[cfg(windows)]
pub use edge::{forget, held, register};

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> std::path::PathBuf {
        std::path::PathBuf::from(r"C:\Program Files\Sill\sill.exe")
    }

    fn trigger() -> Trigger {
        Trigger {
            name: "Morning notes".to_string(),
            action: "sill.launch".to_string(),
            target: r"C:\Users\me\notes.txt".to_string(),
            kind: None,
            argument: None,
            when: When::Daily { hour: 9, minute: 5 },
        }
    }

    /// The rule this whole feature turns on.
    ///
    /// Nothing about the schedule decides it and nothing about the action's
    /// name does either. An action that would stop and ask is refused,
    /// because the asking cannot happen when it fires.
    #[test]
    fn an_action_that_would_ask_cannot_be_scheduled() {
        assert!(may_schedule("sill.runScript", &[Capability::ShellExecution]).is_err());
        assert!(may_schedule("sill.write", &[Capability::FileWrite]).is_err());
        assert!(may_schedule("sill.launch", &[Capability::ProcessLaunch]).is_err());
        assert!(may_schedule("sill.mute", &[Capability::SystemControl]).is_err());

        assert!(may_schedule("sill.read", &[Capability::FileRead]).is_ok());
        assert!(may_schedule("sill.copy", &[Capability::ClipboardWrite]).is_ok());
        assert!(may_schedule("sill.show", &[Capability::Ui]).is_ok());
        assert!(may_schedule("sill.nothing", &[]).is_ok());
    }

    /// The refusal names the capability, because "no" on its own is a bug
    /// report rather than an answer.
    #[test]
    fn the_refusal_says_which_capability_stopped_it() {
        let why = may_schedule("sill.runScript", &[Capability::ShellExecution])
            .expect_err("a shell cannot be scheduled");

        assert!(why.contains("sill.runScript"), "{why}");
        assert!(why.contains("runs a command on this machine"), "{why}");
    }

    /// A name is a path segment inside one folder, and a separator in it puts
    /// the task somewhere `remove` will not look and uninstall will not find.
    #[test]
    fn a_name_cannot_leave_the_folder() {
        assert!(task_name(r"..\Microsoft\Windows\Defrag\ScheduledDefrag").is_err());
        assert!(task_name("a/b").is_err());
        assert!(task_name("a:b").is_err());
        assert!(task_name("a*b").is_err());
        assert!(task_name("with\u{7}bell").is_err());
        assert!(task_name("   ").is_err());
        assert!(task_name(&"x".repeat(NAME_AT_MOST + 1)).is_err());

        assert_eq!(
            task_name("  Morning notes  "),
            Ok("Morning notes".to_string())
        );
    }

    /// What the task runs is what somebody could have typed, and the reading
    /// Windows gives it has to be the reading Sill meant.
    ///
    /// The round trip is the test rather than the shape of either half: a
    /// quoter and a parser that agree with each other and not with Windows
    /// would pass any check written against one of them alone.
    #[test]
    fn what_windows_runs_reads_back_as_the_ask_that_was_meant() {
        let mut t = trigger();
        t.kind = Some("text".to_string());
        t.argument = Some("two words".to_string());

        let xml = definition(&exe(), &t).expect("that trigger is fine");
        let ask = read_back(&exe(), &xml).expect("Sill's own task reads back");

        assert_eq!(ask.trust, Trust::Shell);
        assert_eq!(ask.action, "sill.launch");
        assert_eq!(ask.target, r"C:\Users\me\notes.txt");
        assert_eq!(ask.kind.as_deref(), Some("text"));
        assert_eq!(ask.argument.as_deref(), Some("two words"));
    }

    /// The awkward targets, through the whole round trip.
    ///
    /// Each of these breaks a different half. A trailing backslash escapes
    /// the quote that closes its own token, an embedded quote has to survive
    /// both the command line rules and the XML, and an ampersand is the one
    /// that ends an element early.
    #[test]
    fn an_awkward_target_survives_the_quoting_and_the_xml() {
        for target in [
            r"C:\Users\me\My Documents\",
            r#"C:\a"b\c.txt"#,
            r"C:\notes & drafts\x.txt",
            r"C:\<odd>\'quoted'.txt",
            r"C:\a\\b\\\c.txt",
        ] {
            let mut t = trigger();
            t.target = target.to_string();

            let xml = definition(&exe(), &t).expect("that trigger is fine");
            let ask = read_back(&exe(), &xml).expect("it reads back");

            assert_eq!(ask.target, target, "in {xml}");
        }
    }

    /// A task pointing at a different program is not Sill's, whatever folder
    /// it sits in and whatever it is called.
    #[test]
    fn a_task_running_something_else_is_not_ours() {
        let xml = definition(&exe(), &trigger()).expect("that trigger is fine");
        let moved = xml.replace(
            r"C:\Program Files\Sill\sill.exe",
            r"C:\Users\me\Downloads\sill.exe",
        );

        let why = read_back(&exe(), &moved).expect_err("a different exe is refused");
        assert!(why.contains("Downloads"), "{why}");
    }

    /// The command line is text on disk, so the reading has to survive
    /// somebody rewriting it into something else.
    #[test]
    fn a_rewritten_command_line_is_not_described_as_ours() {
        let xml = definition(&exe(), &trigger()).expect("that trigger is fine");
        let was = element(&xml, "Arguments").expect("it has arguments");

        /// The one element the tampering happens in, replaced whole.
        ///
        /// A `replace` of some words out of the middle passes whether or not
        /// it matched anything, which is a sabotage that quietly tests the
        /// original document. This one panics instead.
        fn instead(xml: &str, was: &str, now: &str) -> String {
            let from = format!("<Arguments>{was}</Arguments>");
            assert!(xml.contains(&from), "no {from} in {xml}");
            xml.replace(&from, &format!("<Arguments>{now}</Arguments>"))
        }

        // Asks for nothing at all.
        let empty = instead(&xml, &was, "--some-flag sill.launch C:\\x.txt");
        assert!(read_back(&exe(), &empty).is_err());

        // A link's rules, smuggled in where a shell's were expected. It
        // parses, as an address, and is refused for being one.
        let link = instead(&xml, &was, "sill://run/sill.launch?target=C:/x.txt");
        assert!(read_back(&exe(), &link).is_err());

        // Still a shell ask, and still read back honestly: naming a heavier
        // action does not fool the reading, it produces an ask the caller's
        // `may_schedule` then refuses.
        let heavier = instead(&xml, &was, "run sill.runScript C:\\evil.ps1");
        let ask = read_back(&exe(), &heavier).expect("it still parses");
        assert_eq!(ask.action, "sill.runScript");
        assert!(may_schedule(&ask.action, &[Capability::ShellExecution]).is_err());
    }

    /// A trigger's cost while nothing is happening is Windows' cost, and the
    /// two settings that would make it Sill's are the ones pinned here.
    ///
    /// `WakeToRun` brings a sleeping machine up to run a launcher errand,
    /// which is the largest idle cost this feature could possibly have. The
    /// execution limit is the other direction: Sill started by a task **is**
    /// Sill when nothing was running, so a limit that expired would have Task
    /// Scheduler terminate the launcher hours later.
    #[test]
    fn a_trigger_never_wakes_the_machine_or_kills_the_launcher() {
        let xml = definition(&exe(), &trigger()).expect("that trigger is fine");

        assert!(xml.contains("<WakeToRun>false</WakeToRun>"), "{xml}");
        assert!(
            xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"),
            "{xml}"
        );
        assert!(
            xml.contains("<AllowHardTerminate>false</AllowHardTerminate>"),
            "{xml}"
        );
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"), "{xml}");
    }

    /// Each schedule produces the element Windows reads it out of, and a time
    /// that is not a time is refused before it becomes a task that registers
    /// and never fires.
    #[test]
    fn each_schedule_writes_its_own_trigger() {
        let daily = definition(
            &exe(),
            &Trigger {
                when: When::Daily {
                    hour: 23,
                    minute: 5,
                },
                ..trigger()
            },
        )
        .expect("that trigger is fine");
        assert!(
            daily.contains("<StartBoundary>2000-01-01T23:05:00</StartBoundary>"),
            "{daily}"
        );

        let logon = definition(
            &exe(),
            &Trigger {
                when: When::AtLogon,
                ..trigger()
            },
        )
        .expect("that trigger is fine");
        assert!(logon.contains("<LogonTrigger>"), "{logon}");

        let unlock = definition(
            &exe(),
            &Trigger {
                when: When::OnUnlock,
                ..trigger()
            },
        )
        .expect("that trigger is fine");
        assert!(
            unlock.contains("<StateChange>SessionUnlock</StateChange>"),
            "{unlock}"
        );

        assert!(definition(
            &exe(),
            &Trigger {
                when: When::Daily {
                    hour: 24,
                    minute: 0
                },
                ..trigger()
            },
        )
        .is_err());
    }

    /// A one-off names a moment and an end, and asks Windows to tidy it away.
    ///
    /// All three together or none of them. `DeleteExpiredTaskAfter` does
    /// nothing to a task that cannot expire, so the end boundary is what makes
    /// it mean anything, and without both the timers feature would leave one
    /// dead task in the folder for every reminder anybody ever set.
    #[test]
    fn a_one_off_removes_itself_once_it_has_run() {
        let xml = definition(
            &exe(),
            &Trigger {
                when: When::Once {
                    at: crate::timers::Local {
                        year: 2026,
                        month: 9,
                        day: 4,
                        hour: 14,
                        minute: 35,
                        second: 0,
                    },
                },
                ..trigger()
            },
        )
        .expect("that trigger is fine");

        assert!(
            xml.contains("<StartBoundary>2026-09-04T14:35:00</StartBoundary>"),
            "{xml}"
        );
        assert!(
            xml.contains("<EndBoundary>2026-09-04T14:36:00</EndBoundary>"),
            "{xml}"
        );
        assert!(
            xml.contains("<DeleteExpiredTaskAfter>PT0S</DeleteExpiredTaskAfter>"),
            "{xml}"
        );
    }

    /// And nothing else asks to be deleted, because nothing else expires.
    #[test]
    fn a_repeating_trigger_is_never_asked_to_delete_itself() {
        for when in [
            When::Daily { hour: 9, minute: 5 },
            When::AtLogon,
            When::OnUnlock,
        ] {
            let xml =
                definition(&exe(), &Trigger { when, ..trigger() }).expect("that trigger is fine");

            assert!(
                !xml.contains("DeleteExpiredTaskAfter"),
                "{when:?} would be deleted, and it has no end to expire at: {xml}"
            );
        }
    }

    /// A moment that does not exist is refused before it becomes a task.
    #[test]
    fn a_one_off_at_no_such_moment_is_refused() {
        let bad = definition(
            &exe(),
            &Trigger {
                when: When::Once {
                    at: crate::timers::Local {
                        year: 2026,
                        month: 9,
                        day: 31,
                        hour: 14,
                        minute: 35,
                        second: 0,
                    },
                },
                ..trigger()
            },
        );

        assert!(bad.is_err(), "September has thirty days");
    }

    /// Whatever `sanitised_name` makes, `task_name` takes.
    ///
    /// Two ideas about what a task may be called is the shape that would let a
    /// reminder fail to set over a colon in the message, blaming a name
    /// nobody typed.
    #[test]
    fn a_repaired_name_is_always_a_name_that_is_accepted() {
        let long = "x".repeat(400);

        for awful in [
            "Reminder at 14:35 call Sam",
            r#"a/b\c*d?e"f<g>h|i"#,
            "with\u{7}bell",
            "   ",
            "",
            long.as_str(),
            "\u{1f600} tea",
        ] {
            let repaired = sanitised_name(awful);

            assert_eq!(
                task_name(&repaired),
                Ok(repaired.clone()),
                "{awful:?} was repaired into {repaired:?}, which is refused"
            );
        }
    }

    /// The splitter is the parser Windows uses, not one that merely agrees
    /// with Sill's own quoter.
    ///
    /// These are the documented cases from `CommandLineToArgvW`, written as
    /// the line rather than built by the quoter, so a matching pair of bugs
    /// cannot pass them both.
    #[test]
    fn a_command_line_splits_the_way_windows_splits_one() {
        assert_eq!(split(r#"a b c"#), vec!["a", "b", "c"]);
        assert_eq!(split(r#""a b" c"#), vec!["a b", "c"]);
        assert_eq!(split(r#"a\\b c"#), vec![r"a\\b", "c"]);
        assert_eq!(split(r#""a\\b" c"#), vec![r"a\\b", "c"]);
        assert_eq!(split(r#"a\"b"#), vec![r#"a"b"#]);
        assert_eq!(split(r#""a\\" b"#), vec![r"a\", "b"]);
        assert_eq!(split(r#""" a"#), vec!["", "a"]);
        assert_eq!(split("   "), Vec::<String>::new());
    }

    /// An ampersand read back twice would turn `&amp;lt;` into `<`, which is
    /// a path that never existed being shown as one that does.
    #[test]
    fn an_entity_is_decoded_once() {
        assert_eq!(unescaped("&amp;lt;"), "&lt;");
        assert_eq!(unescaped(&escaped(r#"a&<>"'b"#)), r#"a&<>"'b"#);
        // Not an entity Sill wrote, and kept rather than swallowed.
        assert_eq!(unescaped("a & b"), "a & b");
    }
}
