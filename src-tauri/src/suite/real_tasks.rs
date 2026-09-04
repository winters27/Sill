//! Against the Task Scheduler this machine actually runs.
//!
//! [`crate::automation`]'s fixtures cover the whole decision: what may be
//! scheduled, what the command line says, and whether a task in the folder is
//! one Sill will vouch for. What they cannot say is whether Windows accepts
//! the document, because the fixtures compare Sill's XML against Sill's idea
//! of the schema and the scheduler has its own.
//!
//! Ignored, because it registers a real task on the machine it runs on:
//!
//! ```text
//! cargo test --lib real_tasks -- --ignored --nocapture --test-threads=1
//! ```
//!
//! **One at a time, and the flag is not optional.** Both tests assert that no
//! task they did not create changed, and they share one folder, so run in
//! parallel each one sees the other's task appear and disappear underneath it
//! and both fail on a machine where nothing is wrong. Run together they also
//! enumerate the folder while the other is registering into it, which Task
//! Scheduler answers with `0x80070002` rather than an empty list.
//!
//! **It cleans up after itself, and the assertion order is why.** The removal
//! happens before anything that could fail after registration, so a broken
//! reading leaves a failed test rather than a task nobody knows about. The
//! name says what it is and what put it there, so one left behind by a panic
//! is still obviously deletable by a person.

/// One trigger, all the way into Windows and out again.
///
/// Three things this proves that no fixture can. Task Scheduler accepts the
/// document, which is a claim about a schema written by somebody else. The
/// task lands in Sill's own folder and [`crate::automation::held`] finds it
/// there. And what comes back out is the same ask that went in, having been
/// through Windows' own serialisation rather than only Sill's.
#[test]
#[ignore]
#[cfg(windows)]
fn one_trigger_goes_in_and_comes_out() {
    use crate::automation::{self, Trigger, When};

    // Named so that one left behind by a crash is obviously a test's and
    // obviously removable. Nothing else in the folder is touched.
    let name = "Sill P8-02 verification (safe to delete)";

    let exe = std::env::current_exe().expect("a test binary knows where it is");

    let trigger = Trigger {
        name: name.to_string(),
        action: "sill.copyPath".to_string(),
        target: r"C:\Users\Public\a folder with spaces\notes & drafts.txt".to_string(),
        kind: None,
        argument: None,
        when: When::Daily { hour: 4, minute: 7 },
    };

    let before = automation::held().expect("the folder can be read");
    let others: Vec<String> = before
        .iter()
        .filter(|task| task.name != name)
        .map(|task| task.name.clone())
        .collect();

    let xml = automation::definition(&exe, &trigger).expect("that trigger is fine");
    automation::register(name, &xml).expect("Windows accepted the document");

    let during = automation::held().expect("the folder can be read");
    let found = during
        .iter()
        .find(|task| task.name == name)
        .cloned()
        .expect("the task Sill just wrote is in Sill's folder");

    println!("next run: {:?}", found.next);
    println!("enabled:  {}", found.enabled);

    // Removed before the reading is checked. A wrong reading should leave a
    // red test and a clean machine, not a task somebody has to go and find.
    automation::forget(name).expect("Windows removed it");

    let after = automation::held().expect("the folder can be read");
    assert!(
        !after.iter().any(|task| task.name == name),
        "the test's own task is still in {}",
        automation::FOLDER_PATH,
    );

    // Nothing else moved. The one thing this test must never do is disturb a
    // task it did not create, and the folder is the only place it can reach.
    let still: Vec<String> = after.iter().map(|task| task.name.clone()).collect();
    assert_eq!(still, others, "a task this test did not create changed");

    // What Windows handed back, read on Sill's terms.
    let ask = automation::read_back(&exe, &found.xml).expect("Sill's own task reads back");

    assert_eq!(ask.action, "sill.copyPath");
    assert_eq!(ask.target, trigger.target);
    assert_eq!(ask.trust, crate::reach::Trust::Shell);
}

/// A one-off timer, all the way into Windows and out again.
///
/// What no fixture can say: whether Task Scheduler accepts a `TimeTrigger`
/// with an end boundary and a `DeleteExpiredTaskAfter` beside it, and whether
/// what it hands back still carries both. That pair is the whole of why a
/// timer leaves nothing behind, and it is written in somebody else's schema.
///
/// The moment is a day out, so nothing here depends on how long the test takes
/// and nothing fires while it runs. The removal happens before the reading is
/// checked, for the reason the test above gives.
#[test]
#[ignore]
#[cfg(windows)]
fn a_timer_goes_in_as_a_task_that_removes_itself() {
    use crate::automation::{self, Trigger, When};
    use crate::timers;

    let name = "Sill P3-11 timer verification (safe to delete)";
    let exe = std::env::current_exe().expect("a test binary knows where it is");

    let at = timers::fires_at(timers::now(), std::time::Duration::from_secs(24 * 60 * 60));

    let trigger = Trigger {
        name: name.to_string(),
        action: "sill.reminder.show".to_string(),
        target: "Take the bread out & call Sam".to_string(),
        kind: Some("reminder".to_string()),
        argument: None,
        when: When::Once { at },
    };

    let before = automation::held().expect("the folder can be read");
    let others: Vec<String> = before
        .iter()
        .filter(|task| task.name != name)
        .map(|task| task.name.clone())
        .collect();

    let xml = automation::definition(&exe, &trigger).expect("that trigger is fine");
    automation::register(name, &xml).expect("Windows accepted the document");

    let during = automation::held().expect("the folder can be read");
    let found = during
        .iter()
        .find(|task| task.name == name)
        .cloned()
        .expect("the timer Sill just wrote is in Sill's folder");

    println!("next run: {:?}", found.next);
    println!("boundary: {}", at.boundary());

    automation::forget(name).expect("Windows removed it");

    let after = automation::held().expect("the folder can be read");
    assert!(
        !after.iter().any(|task| task.name == name),
        "the test's own timer is still in {}",
        automation::FOLDER_PATH,
    );

    let still: Vec<String> = after.iter().map(|task| task.name.clone()).collect();
    assert_eq!(still, others, "a task this test did not create changed");

    /*
     * The two elements the whole claim rests on, read out of what Windows
     * handed back rather than out of what Sill sent.
     *
     * Task Scheduler rewrites the document it is given, so a setting it does
     * not understand can simply be absent on the way out. That is exactly the
     * failure worth catching here: it would leave a dead task in the folder
     * for every reminder anybody ever set, and nothing about the feature
     * working would look wrong.
     */
    assert!(
        found.xml.contains("DeleteExpiredTaskAfter"),
        "Windows dropped the setting that removes the task: {}",
        found.xml
    );
    assert!(
        found.xml.contains("EndBoundary"),
        "Windows dropped the end boundary, so the task can never expire: {}",
        found.xml
    );

    // And it is still Sill's own reminder on the way back.
    let ask = automation::read_back(&exe, &found.xml).expect("Sill's own timer reads back");

    assert_eq!(ask.action, "sill.reminder.show");
    assert_eq!(ask.target, "Take the bread out & call Sam");
    assert_eq!(ask.kind.as_deref(), Some("reminder"));
    assert_eq!(ask.trust, crate::reach::Trust::Shell);
}
