//! What the window is offered for each kind of thing.

#[test]
#[ignore = "reports what the action registry offers"]
fn report_actions_per_kind() {
    let registry = sill_lib::actions::builtins();

    for mode in ["clipboard", "app", "file", "url", "websearch", "setting", "system", "text"] {
        let kind = sill_lib::object::ObjectKind::from_mode(mode);
        let Some(kind) = kind else {
            println!("{mode:12} (no kind)");
            continue;
        };

        let offered: Vec<String> = registry
            .for_kind(kind)
            .into_iter()
            .map(|a| format!("{}{}", a.title(), if a.is_primary(kind) { "*" } else { "" }))
            .collect();

        println!("{mode:12} {}", offered.join(", "));
    }
}
