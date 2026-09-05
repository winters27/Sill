//! Searching, and opening what was found.

use tauri::{AppHandle, Manager, State};

use crate::state::{now_seconds, CatalogState, PrefsState, RegistryState};
use crate::{browsers, calculator, catalog, files, registry, windowing};

/// What the row offering a conversation back says underneath the question.
///
/// Both halves earn their place. The age is why the row is there at all and
/// why it will not be there for long; the count is what distinguishes a
/// conversation worth returning to from one question that got one answer.
fn said_about(offer: &crate::ai::chat::Offer) -> String {
    let when = if offer.age < 60 {
        "Just now".to_string()
    } else {
        let minutes = offer.age / 60;
        format!(
            "{minutes} minute{} ago",
            if minutes == 1 { "" } else { "s" }
        )
    };

    let replies = offer.replies;
    format!(
        "{when} · {replies} repl{}",
        if replies == 1 { "y" } else { "ies" }
    )
}

/// The root list, or what matches a query.
#[tauri::command]
pub(crate) async fn search_commands(
    app: AppHandle,
    state: State<'_, RegistryState>,
    prefs: State<'_, PrefsState>,
    emoji: State<'_, crate::emoji::Emoji>,
    timings: State<'_, crate::timing::Timings>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let _timing = timings.inner().timing("commands");

    // `tag:work` is taken out of the query before ranking and applied after
    // it: the words that are left match as they always did, and only the rows
    // carrying that tag survive. A tag is a `#name` keyword, so a plain word
    // finds a tagged row too; the operator is for wanting only those.
    let (query, tag) = registry::tag_operator(&query);

    let (excluded, hidden, pinned, tone, notes_on) = {
        let prefs = prefs.inner.lock().await;
        (
            prefs.sources.excluded.clone(),
            prefs.sources.hidden.clone(),
            prefs.sources.pinned.clone(),
            prefs.emoji.tone,
            // A prototype, off unless somebody has said otherwise. Read here
            // with everything else rather than behind its own lock: a switch
            // that is off has to cost a keystroke a `bool` and no more.
            prefs.general.notes,
        )
    };
    // A snapshot rather than a lock: ranking reads it from beginning to end
    // and nothing else should wait for that.
    let index = state.index();
    let ranking = state.ranking();

    /*
     * The conversation you left, for as long as it is worth offering.
     *
     * Built here and chained into the corpus rather than pushed onto the
     * results, so that it is found by typing like everything else. It has to
     * outlive the search below, which borrows it, which is why it is a
     * binding rather than an expression inside the chain.
     */
    let offered = app
        .state::<crate::ai::chat::Chat>()
        .offer(now_seconds())
        .map(|offer| registry::conversation_record(&offer.id, &offer.title, &said_about(&offer)));

    /*
     * Open windows, ranked in the same pass as everything else.
     *
     * They used to be a second command, appended to the results after the
     * first had already been capped at 120. So on a one or two character query
     * the cap filled with weak command matches and **a window whose title was
     * an exact match landed at row 121**, which is to say nowhere. Two lists
     * concatenated is not a ranking, and the cap is what made it visible.
     *
     * Chained into the same iterator instead, so a window competes on its
     * match class like anything else. Enumeration is Win32 and synchronous,
     * which is why it is cached for a moment: typing eight characters walked
     * every top-level window on the desktop eight times.
     *
     * The switcher is not this. It has its own command and its own order,
     * because an empty query there means "the window you were just in" rather
     * than "the best match for nothing".
     */
    let windows = if query.trim().is_empty() {
        Vec::new()
    } else {
        windowing::recent_records(&app.state::<crate::state::Fresh<Vec<registry::CommandRecord>>>())
    };

    // Chained, not collected: both sides are borrowed and nothing is copied.
    let mut results = registry::search_excluding(
        index
            .everything()
            .chain(offered.iter())
            .chain(windows.iter()),
        &query,
        &ranking.frecency,
        &index.aliases,
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded {
            terms: &excluded,
            ids: &hidden,
        },
        &pinned,
    );

    if let Some(tag) = &tag {
        results.retain(|hit| registry::tagged(&hit.command, tag));
    }

    /*
     * A switch says which way it is set.
     *
     * Read here rather than carried by the index, because what a switch is set
     * to is a fact about the moment somebody looks at it. Read at all only if
     * one actually matched, so a search that finds no switch costs nothing,
     * and behind a one second cache, because typing "bluetooth" is eight
     * keystrokes and enumerating the radios eight times is eight times too
     * many.
     */
    if results.iter().any(|hit| hit.command.mode == "system") {
        let live = crate::system::live(&app.state::<crate::state::Fresh<crate::system::Live>>());

        for hit in results.iter_mut() {
            if hit.command.mode == "system" {
                hit.command.toggle = crate::system::toggle_state(&hit.command.entrypoint, &live);
            }
        }
    }

    /*
     * The one row of Sill's own that says which way it is set.
     *
     * A switch's state is a fact about the moment somebody looks at it, the
     * same as the Windows switches above, so it is filled in here rather than
     * carried by the index. It costs an atomic load and only when the row
     * actually matched, which is why it is behind the same `any` the switches
     * are: a search that finds no private mode row asks nothing.
     *
     * The id is `registry::PRIVATE_MODE` rather than the string, because the
     * row is built in one file and named in two others and a row that quietly
     * stopped showing its state is exactly what a fourth spelling would buy.
     */
    let private_row = registry::builtin_id(registry::PRIVATE_MODE);
    if results.iter().any(|hit| hit.command.id == private_row) {
        let paused = app.state::<crate::privacy::Privacy>().paused();

        for hit in results.iter_mut() {
            if hit.command.id == private_row {
                hit.command.toggle = Some(paused);
            }
        }
    }

    /*
     * Emoji, ranked here rather than fetched by a second round trip.
     *
     * They stay a separate corpus on purpose: two thousand of them beside
     * fifteen hundred real entries would swamp the list, and their names are
     * ordinary words, so ranking them together would put a smiley in the
     * middle of every search anybody types. Only plainly named ones are
     * offered, and only a few.
     *
     * What changed is who asks. The window used to make a second invoke per
     * keystroke and splice the answer in itself, which meant the placement
     * rule lived in TypeScript and the list was rebuilt twice for one
     * keystroke.
     */
    let inline = inline_emoji(&query, &index, &ranking, &emoji, tone);
    splice_suggestions(&mut results, inline);

    /*
     * What is playing, when the query is one of the words that asks for it.
     *
     * `media::matched` holds the gate and is handed the reading rather than
     * taking it: the machine is not asked anything unless the query was
     * exactly one of nine words. So a keystroke that is not one of them costs
     * a trim and a lookup in a list of nine, and the WinRT call that fetches a
     * session manager and a track title never happens. That is the whole of
     * this item's "costs nothing when not matched", and `media`'s own tests
     * prove it by counting the readings a hundred non-matching queries take.
     *
     * Spliced rather than put at the top. "play" is a word somebody types on
     * the way to Play Store, and this is the same placement the emoji
     * suggestions use: below anything that is a strong match for what was
     * typed, above everything that merely contains the letters. With nothing
     * else matching at all, which is what "pause" usually looks like, that is
     * the top.
     *
     * Nothing playing answers `None`, and `splice_suggestions` draws nothing
     * for an empty list. A machine with no media session gets no row rather
     * than an empty one.
     */
    let playing = crate::media::matched(&query, || {
        crate::media::now(&app.state::<crate::state::Fresh<Option<crate::media::NowPlaying>>>())
    });

    splice_suggestions(
        &mut results,
        playing
            .iter()
            .map(registry::now_playing_record)
            .collect::<Vec<_>>(),
    );

    /*
     * The terminals this machine has, and the profiles inside them.
     *
     * Behind the same shape of gate: the first word of the query, exactly, and
     * `matched` is handed the reading rather than taking it, so a keystroke
     * that is not one of six words never opens Terminal's settings file or
     * touches the registry.
     *
     * A minute of cache, because what it answers changes when somebody edits
     * Terminal's settings or installs a distribution and neither happens while
     * they are typing.
     *
     * These are spliced rather than put on top for the reason the media row
     * is: "terminal" is a word somebody types on the way to a program called
     * Terminal, and a row that stole Enter from it would be the launcher
     * arguing with them.
     */
    let profiles = crate::terminals::matched(&query, || {
        crate::terminals::now(&app.state::<crate::state::Fresh<Vec<crate::terminals::Profile>>>())
    });

    splice_suggestions(
        &mut results,
        profiles
            .iter()
            .map(registry::terminal_record)
            .collect::<Vec<_>>(),
    );

    /*
     * What has been opened recently, out of every application's jump list.
     *
     * The same gate the media row has and for the same reason: `matched` holds
     * it and is handed both readings rather than taking them, so a keystroke
     * whose first word is not one of three never opens a file. Two hundred and
     * seven jump lists on this machine, six megabytes, and a query that is not
     * asking pays a `split_whitespace` and three string comparisons.
     *
     * Behind five seconds of cache because the words after "recent" narrow the
     * list, so typing "recent tax" would otherwise re-read the folder per
     * letter. Bounded to three hundred documents, which is what stops the
     * cache being a leak with a good reason.
     *
     * The existence check is the second closure and is deliberately separate:
     * a path out of a jump list is often gone, and asking the filesystem about
     * three hundred of them per keystroke would cost more than reading them
     * did. Only the handful that survived the filter are asked about, and the
     * same call says whether it is a folder, which decides the row's mode.
     */
    let opened = crate::jumplists::matched(
        &query,
        || {
            crate::jumplists::now(
                &app.state::<crate::state::Fresh<Vec<crate::jumplists::Recent>>>(),
            )
        },
        |path| std::fs::metadata(path).ok().map(|found| found.is_dir()),
    );

    splice_suggestions(
        &mut results,
        opened
            .iter()
            .map(registry::jumplist_record)
            .collect::<Vec<_>>(),
    );

    /*
     * The notes somebody has written, when the query asks for one.
     *
     * The same gate the media, terminal and jump list rows have, with a switch
     * in front of it: `notes::matched` answers on a `bool` before it reaches
     * the word, so a machine with notes turned off never constructs the
     * service, never opens the file and never sorts a list.
     *
     * The gate is deliberately narrower than the others. A note's text is
     * searched, and searching it from every query would put a paragraph out of
     * somebody's diary underneath an application, so the word has to be asked
     * for by name before any of it is looked at.
     *
     * `New Note` last, not first. Somebody typing `note` twice a day is
     * usually going back to the one they wrote this morning, and a row that
     * makes something new sitting under the cursor is the wrong default for a
     * key pressed without looking.
     */
    let notes = crate::notes::matched(&query, notes_on, || {
        app.state::<crate::notes::Notes>().all(&app)
    });

    if let Some(asked) = notes {
        let mut rows: Vec<registry::RankedCommand> =
            asked.found.iter().map(registry::note_record).collect();
        rows.push(registry::new_note_record());

        splice_suggestions(&mut results, rows);
    }

    /*
     * Sums answered earlier, when the query asks for them by name.
     *
     * `sums::matched` holds its own gate on the first word and takes the
     * reading as a closure, so a query that is not the word never opens the
     * file. The rows are the answer rows they were, so Enter copies again and
     * the same action moves the sum back to the top.
     */
    /*
     * The time somewhere, when the query names a city.
     *
     * `zones::matched` holds its own gate on the first or last word and takes
     * the table as a closure, so a keystroke that is not a question about
     * time never enumerates a zone. The table is Windows' own, read at most
     * once an hour.
     */
    let clocks = crate::zones::matched(&query, || {
        app.state::<crate::state::Fresh<std::sync::Arc<Vec<crate::zones::Zone>>>>()
            .get(|| std::sync::Arc::new(crate::zones::all()))
    });

    if !clocks.is_empty() {
        let rows = clocks.iter().map(registry::clock_record).collect();
        splice_suggestions(&mut results, rows);
    }

    /*
     * Installed fonts, when the query asks for them by name.
     *
     * `fonts::matched` holds its own gate on the first word and takes the
     * reading as a closure, held ten minutes once taken, so no keystroke
     * that is not the word enumerates a font.
     */
    let faces = crate::fonts::matched(&query, || {
        app.state::<crate::state::Fresh<std::sync::Arc<Vec<String>>>>()
            .get(|| std::sync::Arc::new(crate::fonts::installed()))
    });

    if !faces.is_empty() {
        let rows = faces
            .iter()
            .map(|name| registry::font_record(name))
            .collect();
        splice_suggestions(&mut results, rows);
    }

    /*
     * The modes a display can be set to, when the query asks.
     *
     * Enumerated on the ask, which is one word, and not cached: a display
     * list is short and a mode list is a millisecond, and both are wrong the
     * moment a monitor is plugged in.
     */
    if let Some(asked) = crate::displays::asked(&query) {
        let devices = crate::displays::devices();
        let chosen = match asked.display {
            Some(number) => devices.iter().find(|(index, _)| *index == number),
            None => devices.first(),
        };

        if let Some((index, device)) = chosen {
            let modes = crate::displays::matched(&asked, crate::displays::modes(device, *index));
            let rows = modes.iter().map(registry::display_mode_record).collect();
            splice_suggestions(&mut results, rows);
        }
    }

    let past = crate::sums::matched(&query, || {
        app.state::<crate::sums::Sums>()
            .recall(&crate::sums::path(&crate::state::data_dir(&app)))
    });

    if !past.is_empty() {
        let rows = past
            .iter()
            .enumerate()
            .map(|(at, one)| registry::past_answer_record(at, one))
            .collect();

        splice_suggestions(&mut results, rows);
    }

    /*
     * The timer somebody has just described, before it is set.
     *
     * `timers::matched` holds its own gate on the first word, so a query that
     * is not asking pays a `split_whitespace` and three comparisons. Nothing
     * is written down here and no clock is set: the row says what will happen
     * and Enter is what makes it happen.
     *
     * The clock is read only once a timer has actually been recognised, which
     * is what keeps the row's "at 14:35" honest without asking the machine the
     * time on every keystroke.
     */
    #[cfg(windows)]
    if let Some(timer) = crate::timers::matched(&query) {
        let at = crate::timers::fires_at(crate::timers::now(), timer.after);

        splice_suggestions(
            &mut results,
            vec![registry::reminder_record(&query, &timer, at)],
        );
    }

    /*
     * Above everything, because when a query IS a sum, or a request for a
     * UUID, the answer is the only thing wanted.
     *
     * Both return nothing for the ninety-nine queries in a hundred that are
     * searches, so this costs those nothing. The calculator is asked first
     * because its gate is the stricter of the two: it has to guess whether a
     * string is arithmetic at all, while a utility is asked for by name.
     */
    // Dates go first: `today + 3 weeks` and `2026-03-01 - 2026-01-15` are
    // both things the calculator would otherwise hand to fend as arithmetic
    // on integers, and its gate is the one place a date sum cannot be told
    // from a sum.
    // A colour written one way is offered in the others, above everything,
    // the way a sum's answer is. The gate is the first character, so a
    // search that is not a colour pays one comparison.
    if let Some(colour) = crate::colour::parse(&query) {
        let hex = colour.hex();
        let rows: Vec<registry::RankedCommand> = colour
            .other_forms(&query)
            .into_iter()
            .map(|(which, text)| registry::colour_record(which, &text, &query, &hex))
            .collect();

        results.splice(0..0, rows);
    }

    let answer = crate::dates::evaluate(&query, crate::dates::today())
        .or_else(|| calculator::evaluate(&query))
        .or_else(|| crate::utilities::evaluate(&query));

    if let Some(answer) = answer {
        results.insert(0, registry::answer_record(&answer.text, &answer.input));
    }

    // Narrowed to what the window actually reads on the way out. The ranked
    // form carries the fields matching needs, which is most of the bytes and
    // none of the use once ranking is over.
    Ok(results
        .into_iter()
        .map(|ranked| {
            // Looked up here rather than carried through ranking: only the
            // rows that survive are drawn, and only drawn rows show a name.
            let alias = index
                .aliases
                .for_command(&ranked.command.id)
                .map(str::to_string);
            let mut result: registry::SearchResult = ranked.into();
            result.alias = alias;
            result
        })
        .collect())
}

/// Where a set of switches are set, right now.
///
/// The row that was pressed is not the only one that can have changed. The
/// audio outputs are one switch each but one choice between them, so turning
/// on Speakers turns off everything else, and nothing about the Speakers row
/// says so. Asked for the switches on screen, which is a handful, and answered
/// out of the same reading the search took, so it is arithmetic rather than a
/// second trip to the sound system.
///
/// Ids that are not switches answer `null`, which is the same answer they give
/// everywhere else: this is not a thing that is on or off.
#[tauri::command]
pub(crate) async fn system_states(
    app: AppHandle,
    state: State<'_, RegistryState>,
    switches: State<'_, crate::state::Fresh<crate::system::Live>>,
    ids: Vec<String>,
) -> Result<Vec<Option<bool>>, String> {
    let index = state.index();
    let live = crate::system::live(&switches);

    let states = crate::system::states_for(
        index
            .commands
            .iter()
            .map(|row| (row.id.as_str(), row.entrypoint.as_str())),
        &ids,
        &live,
    );

    /*
     * Private mode answers here too, because this is the one question the
     * window asks after pressing a row that has a state.
     *
     * It is not a Windows switch and is not in `system::states_for`, so
     * without this it answered `null` and the window left the row showing what
     * it said before it was pressed. A switch that reads the same after being
     * pressed is a switch that looks broken.
     */
    let private = registry::builtin_id(registry::PRIVATE_MODE);
    let paused = app.state::<crate::privacy::Privacy>().paused();

    Ok(states
        .into_iter()
        .zip(ids.iter())
        .map(|(state, id)| if *id == private { Some(paused) } else { state })
        .collect())
}

/// A picture of one open window, for the switcher.
///
/// Asked for the selected row only, never for the list: opening the switcher
/// on twenty windows must not photograph twenty windows. `None` when the
/// window has closed, is minimized, or refuses to be photographed, which is
/// not an error worth a message: a switcher with no picture is a switcher.
#[tauri::command]
pub(crate) async fn window_preview(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let Ok(handle) = id.parse::<isize>() else {
        return Ok(None);
    };

    /*
     * On a blocking task, because this photographs a window and encodes a
     * picture: tens of milliseconds of GDI and PNG, which has no business on
     * an async worker.
     *
     * The state is fetched inside rather than borrowed from a `State`
     * parameter, because a borrow cannot cross onto another thread and a
     * handle can.
     */
    /*
     * Private mode means no picture rather than an error.
     *
     * A switcher with no preview is a switcher, which is what the comment
     * above says about a window that refuses to be photographed, and the row
     * that switched private mode on is what explains why. An error here would
     * put a message about Sill on screen once per arrow key.
     */
    let Ok(allowed) = crate::privacy::allow(&app.state::<crate::privacy::Privacy>()) else {
        return Ok(None);
    };

    tokio::task::spawn_blocking(move || {
        app.state::<crate::previews::Previews>()
            .of(&allowed, handle)
    })
    .await
    .map_err(|err| format!("could not photograph that window: {err}"))
}

/// Drops every window picture.
///
/// Called when the switcher closes. A preview is a picture of a moment, and
/// keeping them would mean showing a window as it was rather than as it is.
#[tauri::command]
pub(crate) fn forget_previews(previews: State<'_, crate::previews::Previews>) {
    previews.forget();
}

/// A look inside the one file under the cursor.
///
/// Asked for the selected row only and only once the selection has settled,
/// never for the list: arrowing through twenty results must not open twenty of
/// somebody's files. `None` when there is nothing worth showing, which is most
/// files and is not an error.
#[tauri::command]
pub(crate) async fn file_preview(
    app: AppHandle,
    path: String,
) -> Result<Option<crate::previews::Look>, String> {
    /*
     * On a blocking task, because this opens somebody's file. Eight kilobytes
     * for anything that is not a picture, and up to two megabytes and a base64
     * for one that is, neither of which belongs on an async worker.
     *
     * The state is fetched inside rather than borrowed from a `State`
     * parameter, for the reason `window_preview` gives: a borrow cannot cross
     * onto another thread and a handle can.
     */
    tokio::task::spawn_blocking(move || app.state::<crate::previews::Previews>().of_file(&path))
        .await
        .map_err(|err| format!("could not look inside that file: {err}"))
}

/// Drops every look inside a file.
///
/// Called when the list showing them goes away and when the window hides. Up to
/// twelve of these hold up to two megabytes each of somebody's picture, and a
/// hidden launcher holding those is exactly the shape of leak `P2-06` closed.
#[tauri::command]
pub(crate) fn forget_file_previews(previews: State<'_, crate::previews::Previews>) {
    previews.forget_files();
}

/// The window has painted, which is when a summon is actually over.
///
/// Rust can see when the window was told to show itself and not when the page
/// finished drawing, and the page is the half somebody is waiting for: a
/// window that is up and blank is not a launcher you can type into. So the
/// window reports this, once, from inside the frame that follows the summon.
///
/// Deliberately does nothing if no summon is in flight. A page that reloads,
/// or a window shown some other way, would otherwise be recorded as a summon
/// that took as long as the launcher had been sitting open.
#[tauri::command]
pub(crate) fn summon_painted(timings: State<'_, crate::timing::Timings>) {
    timings.summon_painted();
}

/// One batch of readings the page took, handed over when it was put away.
///
/// The other measurements the window owns: how long a keystroke took to reach
/// a screen, and how long an extension took to draw its first view. Rust knows
/// when it answered and not when the answer was drawn, and the drawing is the
/// half somebody waited for.
///
/// **A batch rather than a call per keystroke, deliberately.** The budget
/// table allows one keystroke one search and one delayed page, so reporting on
/// those two with a third call would be the instrumentation breaking a budget
/// in order to measure one.
///
/// An unrecognised name is refused rather than filed somewhere. A page and a
/// binary that disagree about what they are measuring should say so, and the
/// alternative is a keystroke's time appearing in an extension's row.
#[tauri::command]
pub(crate) fn painted(
    timings: State<'_, crate::timing::Timings>,
    what: String,
    took_us: Vec<u64>,
) -> Result<(), String> {
    let Some(what) = crate::timing::Painted::parse(&what) else {
        return Err(format!("nothing called {what:?} is measured here"));
    };

    let took: Vec<std::time::Duration> = took_us
        .into_iter()
        .map(std::time::Duration::from_micros)
        .collect();

    timings.painted_all(what, &took);
    Ok(())
}

/// What reaching the launcher has cost lately.
///
/// Read by the settings window and by the probe that holds the budget. The
/// numbers are the ones the audit refused to let anybody claim without
/// measuring.
#[tauri::command]
pub(crate) fn timings(timings: State<'_, crate::timing::Timings>) -> crate::timing::Report {
    timings.report()
}

/// The programs playing sound, matching a query.
///
/// Its own command for the reason the window switcher has one: a different
/// corpus with a different lifetime. The index is scanned once and cached; a
/// program has a volume of its own only while it is playing something, so this
/// is enumerated when it is asked for.
///
/// Not part of the root list, and that is a measurement rather than a taste.
/// Enumerating costs about three milliseconds, and the root list runs on every
/// keystroke whether or not anything about sound was typed. It sits behind its
/// own row instead, so it costs nothing until somebody wants it.
#[tauri::command]
pub(crate) async fn search_app_volume(
    app: AppHandle,
    state: State<'_, RegistryState>,
    timings: State<'_, crate::timing::Timings>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let _timing = timings.inner().timing("app volume");

    // Blocking: a COM apartment and an enumeration of the audio engine. The
    // handle is cloned in rather than a `State` borrowed, because the reading
    // happens on a thread that outlives this call.
    let sessions = tokio::task::spawn_blocking(move || {
        crate::app_volume::sessions(
            &app.state::<crate::state::Fresh<Vec<crate::app_volume::Session>>>(),
        )
    })
    .await
    .unwrap_or_default();

    let records: Vec<registry::CommandRecord> = sessions
        .into_iter()
        .map(|session| registry::audio_session_record(&session))
        .collect();

    // An empty query is the whole list, in the order the audio engine gave
    // them, which puts what started playing most recently first.
    if query.trim().is_empty() {
        // Cloned per row rather than by taking the vector, because the corpus
        // is shared and the picker is one of several readers of it.
        return Ok(records
            .iter()
            .cloned()
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let ranking = state.ranking();

    let results = registry::search_excluding(
        records.iter(),
        &query,
        &ranking.frecency,
        // A session is not in the index, so nothing can have been given a name
        // for one: an alias points at a command id that survives a restart.
        &registry::Aliases::default(),
        now_seconds(),
        registry::SEARCH_LIMIT,
        // A program that is playing is a fact rather than a preference.
        // Hiding it would mean not being able to turn down the thing making
        // the noise.
        registry::Excluded::none(),
        // Not the root list, so nothing is pinned to the top of it.
        &[],
    );

    Ok(results.into_iter().map(Into::into).collect())
}

/// What is running, matching a query.
///
/// Its own command for the reason the volume list has one: a different corpus
/// with a different lifetime. The index is scanned once and cached; a process
/// exists only while it is running, and the list is wrong the moment anything
/// starts or stops.
///
/// **Not part of the root list, and that is the whole reason it is here.**
/// Walking every process on the machine, opening each one and reading its
/// working set, is not something to do because somebody typed the letter p.
/// It sits behind a row of its own, so it costs nothing until somebody asks.
///
/// Anything Windows needs is dropped, because a row drawn only to refuse is a
/// worse row than no row. Sill's own processes stay: what the launcher costs
/// is a fair question to ask a list of what things cost, and quitting one is
/// refused when it is tried rather than hidden from view.
///
/// The refusing is the action's, not this list's. Nothing here is a gate: a
/// hotkey, a workflow and the model all reach the same action without passing
/// through a search, so the check that matters is the one at the other end.
#[tauri::command]
pub(crate) async fn search_processes(
    app: AppHandle,
    state: State<'_, RegistryState>,
    timings: State<'_, crate::timing::Timings>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let _timing = timings.inner().timing("processes");

    // Blocking: an enumeration of every process on the machine. The handle is
    // cloned in rather than a `State` borrowed, because the reading happens on
    // a thread that outlives this call.
    let running = tokio::task::spawn_blocking(move || {
        crate::processes::listed(
            &app.state::<crate::state::Fresh<Vec<crate::processes::Process>>>(),
        )
    })
    .await
    .unwrap_or_default();

    let records: Vec<registry::CommandRecord> = running
        .into_iter()
        .filter(|process| !crate::processes::is_protected(&process.name))
        .map(|process| registry::process_record(&process))
        .collect();

    // An empty query is the whole list, in the order `running` produced, which
    // is heaviest first: what somebody opening this came to find.
    if query.trim().is_empty() {
        return Ok(records
            .into_iter()
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let ranking = state.ranking();

    let results = registry::search_excluding(
        records.iter(),
        &query,
        &ranking.frecency,
        // A process is not in the index, so nothing can have been given a name
        // for one: an alias points at a command id that survives a restart.
        &registry::Aliases::default(),
        now_seconds(),
        registry::SEARCH_LIMIT,
        // What is running is a fact rather than a preference. Hiding a row
        // here would mean not being able to quit the thing eating the machine.
        registry::Excluded::none(),
        // Not the root list, so nothing is pinned to the top of it.
        &[],
    );

    Ok(results.into_iter().map(Into::into).collect())
}

/// The pressable controls of the window somebody was in, matching a query.
///
/// **Not part of the root list, for the same reason the process list is not,
/// only more so.** Reading a window's controls walks another program's tree
/// across the process boundary; done on every keystroke against whatever
/// happened to be in front, the launcher would be interrogating the window
/// behind it while somebody searches for a calculator. Against a
/// Firefox-family browser the first such read switches that browser's
/// accessibility engine on for the life of the process, which is a cost
/// nobody asked for. So it sits behind a row of its own and costs exactly
/// nothing until somebody opens it.
///
/// **Which window is not a parameter**, and that is deliberate. The window is
/// whichever one the launcher took the foreground from, which Sill already
/// records to hand focus back on dismissal. Passing a handle in from the
/// frontend would make the window somebody's controls are read from a value
/// the window layer could get wrong, and there is no honest way for a
/// launcher's own field to name a window it is not in front of.
///
/// The refusing is `controls::read`'s, not this list's: a hotkey and the
/// action registry reach the same code without passing through a search.
#[tauri::command]
pub(crate) async fn search_controls(
    state: State<'_, RegistryState>,
    timings: State<'_, crate::timing::Timings>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let _timing = timings.inner().timing("controls");

    let Some(window) = crate::summon::previous_foreground() else {
        return Err("nothing was in front when Sill opened".to_string());
    };

    // Cross-process COM calls that block, so never on an async worker. The
    // same rule the tab read follows and for the same reason.
    let (controls, about) = tokio::task::spawn_blocking(move || {
        (
            crate::controls::read(window),
            crate::windowing::find(window),
        )
    })
    .await
    .map_err(|err| format!("reading that window failed: {err}"))?;

    let controls = controls?;

    let Some(about) = about else {
        return Err("that window closed while it was being read".to_string());
    };

    let records: Vec<registry::CommandRecord> = controls
        .iter()
        .map(|control| {
            registry::control_record(
                control,
                &about.app,
                (!about.app_path.is_empty()).then_some(about.app_path.as_str()),
            )
        })
        .collect();

    // An empty query is every control, in the order the window has them, which
    // is roughly the order somebody looking at it would read them in. Ranking
    // an empty query would replace that with an alphabet.
    if query.trim().is_empty() {
        return Ok(records
            .into_iter()
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let ranking = state.ranking();

    let results = registry::search_excluding(
        records.iter(),
        &query,
        &ranking.frecency,
        // A control is not in the index, so nothing can have been given a name
        // for one: an alias points at a command id that survives a restart,
        // and this one does not survive the window being redrawn.
        &registry::Aliases::default(),
        now_seconds(),
        registry::SEARCH_LIMIT,
        // What a window offers is a fact rather than a preference. Hiding a
        // row here would mean not being able to press the one button somebody
        // opened this to reach.
        registry::Excluded::none(),
        // Not the root list, so nothing is pinned to the top of it.
        &[],
    );

    Ok(results.into_iter().map(Into::into).collect())
}

/// The open windows matching a query.
///
/// Separate from `search_commands` for the reason file search is separate: it
/// is a different corpus with a different lifetime. The index is scanned once
/// and cached; the desktop is enumerated fresh every time, because a window
/// list is wrong the moment anything is opened or closed.
///
/// Ranked by the same function as everything else. A window is a
/// `CommandRecord` for exactly as long as the ranking takes, so "chrome"
/// finds Chrome windows by the same rules that make it find Chrome.
#[tauri::command]
pub(crate) async fn search_windows(
    state: State<'_, RegistryState>,
    timings: State<'_, crate::timing::Timings>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let _timing = timings.inner().timing("windows");

    // Blocking: enumeration is synchronous Win32 and touches every top-level
    // window on the desktop.
    let records = tokio::task::spawn_blocking(windowing::records)
        .await
        .unwrap_or_default();

    // An empty query is the switcher, and its order is already right.
    //
    // Enumeration walks the Z-order from the front, which is what recency
    // means for windows. Ranking an empty query sorts by frecency and then by
    // title, which would replace "the window you were just in" with "the
    // window with the shortest name". Alt-Tab's whole value is that first
    // entry, so it is left alone.

    if query.trim().is_empty() {
        return Ok(records
            .into_iter()
            .take(registry::SEARCH_LIMIT)
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let ranking = state.ranking();

    // No exclusion terms. Those hide things from the index, and a window that
    // is open is a fact rather than a preference: hiding it would mean the
    // switcher cannot reach something the taskbar shows.
    let results = registry::search_excluding(
        records.iter(),
        &query,
        &ranking.frecency,
        // A window is not in the index, so nothing can have been given a name
        // for one. An alias points at a command id that survives a restart.
        &registry::Aliases::default(),
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded::none(),
        // Not the root list, so nothing is pinned to the top of it.
        &[],
    );

    Ok(results.into_iter().map(Into::into).collect())
}

/// Emoji matching a query.
///
/// Its own corpus rather than part of the index. Three thousand seven hundred
/// entries would nearly quadruple a fifteen-hundred-entry index that is ranked
/// on every keystroke, so that typing "smile" could find an emoji as well as
/// an application. Behind its own command, they cost nothing until asked for.
#[tauri::command]
pub(crate) async fn search_emoji(
    state: State<'_, RegistryState>,
    prefs: State<'_, PrefsState>,
    emoji: State<'_, crate::emoji::Emoji>,
    timings: State<'_, crate::timing::Timings>,
    query: String,
    // Whether these are being offered beside results that were asked for.
    //
    // Emoji volunteer themselves into the root list, so they have to earn the
    // room: a handful, and only where the user plainly named the thing. Loose
    // matching would put a smiley in the middle of every search, because there
    // are nearly two thousand of them and their names are ordinary words.
    //
    // The picker itself passes nothing, because there the emoji ARE the list.
    inline: Option<bool>,
) -> Result<Vec<registry::SearchResult>, String> {
    let _timing = timings.inner().timing("emoji");

    let tone = prefs.inner.lock().await.emoji.tone;

    // The same corpus the inline search uses, so the picker does not build a
    // second copy of two thousand records every time it is opened.
    let records = emoji.records(tone);

    let index = state.index();
    let ranking = state.ranking();

    // An empty query lists them in their own order, which is by group and then
    // by how Unicode arranged them: smileys, people, animals, food. Ranking
    // that by frecency would scatter related emoji across the list.
    if query.trim().is_empty() {
        // Cloned per row rather than by taking the vector, because the corpus
        // is shared now and the picker is one of several readers of it.
        return Ok(records
            .iter()
            .cloned()
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let results = registry::search_excluding(
        records.iter(),
        &query,
        &ranking.frecency,
        &index.aliases,
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded::none(),
        // Not the root list, so nothing is pinned to the top of it.
        &[],
    );

    if !inline.unwrap_or(false) {
        return Ok(results.into_iter().map(Into::into).collect());
    }

    Ok(results
        .into_iter()
        .filter(|ranked| {
            registry::match_class_with_alias(
                &query,
                &ranked.command,
                index.aliases.for_command(&ranked.command.id).unwrap_or(""),
            )
            .is_some_and(registry::is_strong)
        })
        .take(INLINE_EMOJI)
        .map(Into::into)
        .collect())
}

/// Puts a second search's results into the first, by how well each matched.
///
/// Above everything the index only half-recognised, below everything it knew
/// by name. Neither list is reordered within itself and nothing is dropped.
///
/// Appending was wrong and measuring showed how wrong: **typing `tada` matched
/// eighty-four things in the index, every one a coincidence of spelling, and
/// the emoji somebody had plainly named landed eighty-fifth**, where Enter
/// opened a Sill setting instead.
///
/// This rule used to live in the window, in TypeScript, which meant a second
/// invoke per keystroke to have something to splice. It is here now, and this
/// is where its tests are.
fn splice_suggestions(
    results: &mut Vec<registry::RankedCommand>,
    suggestions: Vec<registry::RankedCommand>,
) {
    if suggestions.is_empty() {
        return;
    }

    let at = results
        .iter()
        .position(|hit| !registry::is_strong(hit.class))
        .unwrap_or(results.len());

    results.splice(at..at, suggestions);
}

/// The few emoji worth offering beside an ordinary search.
///
/// Empty for an empty query, and empty unless the query plainly names one:
/// `is_strong` is the same test the ranker uses to separate "this is what you
/// asked for" from "these letters are in there somewhere". Without it every
/// search would carry a smiley, because there are nearly two thousand of them
/// and their names are ordinary words.
fn inline_emoji(
    query: &str,
    index: &crate::state::Index,
    ranking: &crate::state::Ranking,
    emoji: &crate::emoji::Emoji,
    tone: crate::emoji::Tone,
) -> Vec<registry::RankedCommand> {
    if query.trim().is_empty() {
        return Vec::new();
    }

    // Shared and built once per tone. This runs on every keystroke, and
    // building two thousand records each time was a megabyte of allocation
    // thrown away a moment later.
    let records = emoji.records(tone);

    registry::search_excluding(
        records.iter(),
        query,
        &ranking.frecency,
        &index.aliases,
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded::none(),
        // Not the root list, so nothing is pinned to the top of it.
        &[],
    )
    .into_iter()
    .filter(|ranked| {
        registry::match_class_with_alias(
            query,
            &ranked.command,
            index.aliases.for_command(&ranked.command.id).unwrap_or(""),
        )
        .is_some_and(registry::is_strong)
    })
    .take(INLINE_EMOJI)
    .collect()
}

/// How many emoji may appear beside an ordinary search.
///
/// Few. They are volunteering rather than being asked for, and a row of them
/// pushing applications off the screen is worse than not offering any.
const INLINE_EMOJI: usize = 4;

/// Every display, for laying windows out.
#[tauri::command]
pub(crate) async fn list_monitors() -> Result<Vec<windowing::Monitor>, String> {
    Ok(windowing::monitors())
}

/// The program that opens a web address on this machine.
///
/// So the row offering to search the web can wear the mark of the browser it
/// will open, rather than Sill's. The row is not Sill doing something; it is
/// Sill handing the question to that program, and it should look like it.
#[tauri::command]
pub(crate) async fn default_browser() -> Result<Option<String>, String> {
    Ok(browsers::default_browser().map(|path| path.to_string_lossy().into_owned()))
}

/// The search engines Sill knows.
///
/// Named by Rust so the list exists once. A settings pane holding its own copy
/// is a second place to add an engine and a first place to forget one.
#[tauri::command]
pub(crate) async fn search_engines() -> Result<Vec<crate::websearch::Engine>, String> {
    Ok(crate::websearch::ENGINES.to_vec())
}

/// Which browsers are on this machine, named.
///
/// So the settings page can say what would be read rather than leaving somebody
/// to trust a switch. A feature that reads a browsing history should be able to
/// answer "whose?" before it is turned on.
///
/// Names only, and each one once: a browser with four profiles is still one
/// browser as far as the question goes.
#[tauri::command]
pub(crate) async fn browser_profiles() -> Result<Vec<KnownBrowser>, String> {
    let mut found: Vec<KnownBrowser> = Vec::new();

    for profile in browsers::profiles() {
        if found.iter().any(|known| known.name == profile.browser) {
            continue;
        }

        found.push(KnownBrowser {
            name: profile.browser,
            program: profile.program.map(|p| p.to_string_lossy().into_owned()),
        });
    }

    Ok(found)
}

/// A browser Sill found, and the program behind it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnownBrowser {
    pub name: String,
    /// So the pane can show the browser's own mark rather than describing it.
    pub program: Option<String>,
}

/// Pages a browser remembers, visited or saved.
///
/// Separate from `search_commands` for the same reason files are: it reads
/// files that belong to other programs and are large, so the window asks for it
/// behind a debounce and lets what Sill already knows appear first.
///
/// Copies live under Sill's own data directory rather than in the system
/// temporary folder. They are derived from somebody's browsing history, and
/// leaving that in a world-writable directory that nothing ever cleans is not
/// where it belongs.
async fn matching_pages(
    query: &str,
    settings: crate::preferences::Browsers,
    scratch: std::path::PathBuf,
    searching: &crate::state::Searching,
    token: u64,
) -> Result<Vec<browsers::Hit>, String> {
    let query = query.to_string();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    /*
     * Checked before anything is read, because the cheapest version of this
     * search is still expensive.
     *
     * A copy of the history is taken when the previous one is over five
     * minutes old, and Chromium's is tens of megabytes. Doing that for a
     * keystroke that has already been overtaken is the clearest waste in the
     * whole search path.
     */
    if !searching.is_current(token) {
        return Ok(Vec::new());
    }

    let wanted = settings.max_results as usize;
    let want = browsers::Want {
        history: settings.history,
        bookmarks: settings.bookmarks,
    };

    // Reads and copies files, so it never runs on an async worker.
    tokio::task::spawn_blocking(move || browsers::search(&query, wanted, want, &scratch))
        .await
        .map_err(|err| format!("browser search failed: {err}"))
}

/// Everything the index does not hold, in one answer.
///
/// Files and browser pages, which are the two sources that read somebody
/// else's files rather than Sill's own index, and the two the window waits a
/// moment before asking for.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Elsewhere {
    files: Vec<files::FileHit>,
    pages: Vec<browsers::Hit>,
    /// Tabs the running browsers have open right now.
    tabs: Vec<TabRow>,
}

/// One open tab, as a row.
///
/// A shape of its own rather than the domain type, for the one reason rule 9
/// allows: `entrypoint` is a **format**, and the window that draws the row is
/// the wrong place to know it. Written there, the composing and the parsing
/// would be one function in TypeScript and one in Rust, and the pair would
/// hold only for as long as nobody added a field. Here it is `Where`'s own
/// writer, and `Where`'s own reader takes it back.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TabRow {
    /// Stable while the tab lives, which is as long as the row is worth
    /// anything: the window it is in and the browser's own name for it.
    id: String,
    title: String,
    /// Which browser, for the row's group and its label.
    browser: String,
    /// That browser's program, for the row's icon.
    program: Option<String>,
    /// Already the tab in front of its own window.
    active: bool,
    /// Everything the action needs to find this tab again. See `uia::Where`.
    entrypoint: String,
}

impl TabRow {
    /// What the row carries, for the probe that presses Enter on one.
    ///
    /// The field stays private: everything else reads this through JSON, and
    /// a reader inside Rust would be a second way to get at a value whose only
    /// consumer is a `#[tauri::command]`'s answer.
    #[cfg(test)]
    pub(crate) fn entrypoint(&self) -> &str {
        &self.entrypoint
    }
}

impl From<crate::uia::Tab> for TabRow {
    fn from(tab: crate::uia::Tab) -> Self {
        let located = tab.located();

        Self {
            id: format!("browser-tab:{}:{}", tab.window, tab.key),
            title: tab.title,
            browser: tab.browser,
            program: tab.program,
            active: tab.active,
            entrypoint: located.to_entrypoint(),
        }
    }
}

/// Tabs the running browsers have open, matching a query.
///
/// Here rather than beside `search_commands` for the same reason files and
/// history are here: it asks another program a question, so the window shows
/// what Sill already knows first and lets this arrive after.
///
/// **Nothing is read unless a browser is running.** The window list is
/// something the launcher enumerates anyway, so a machine with no browser open
/// spends one filter over a list it already has and never touches UI
/// Automation at all.
async fn matching_tabs(
    query: &str,
    settings: crate::preferences::Browsers,
    searching: &crate::state::Searching,
    token: u64,
) -> Result<Vec<TabRow>, String> {
    if !settings.tabs {
        return Ok(Vec::new());
    }

    let mut families = vec![crate::browsers::Family::Chromium];

    // Firefox separately, because reading one switches that browser's
    // accessibility engine on and it stays on. See `preferences::Browsers`.
    if settings.tabs_firefox {
        families.push(crate::browsers::Family::Firefox);
    }

    let open = crate::uia::browser_windows(&crate::windowing::list(), &families);

    if open.is_empty() {
        return Ok(Vec::new());
    }

    // Checked here rather than earlier because the check above is the cheap
    // one and this is the expensive one: past this line the read crosses into
    // another program's process, once per window.
    if !searching.is_current(token) {
        return Ok(Vec::new());
    }

    let query = query.to_string();
    let wanted = settings.max_results as usize;

    // Cross-process COM calls that block, so never on an async worker.
    tokio::task::spawn_blocking(move || {
        crate::uia::rank(crate::uia::read(&open), &query, wanted)
            .into_iter()
            .map(TabRow::from)
            .collect()
    })
    .await
    .map_err(|err| format!("browser tab search failed: {err}"))
}

/// Both of the slow sources, asked once and answered together.
///
/// Two commands before, awaited one after the other, so a keystroke that got
/// past the debounce cost two round trips and the browser search did not start
/// until the file search had finished. They run at the same time now, and the
/// window appends one answer instead of two.
///
/// Still separate from `search_commands`, and deliberately: one spawns a
/// process and the other copies a browser's history, so the window shows what
/// the index knows first and lets these arrive after.
#[tauri::command]
pub(crate) async fn search_elsewhere(
    app: AppHandle,
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
    searching: State<'_, crate::state::Searching>,
    timings: State<'_, crate::timing::Timings>,
    recent: State<'_, crate::state::Fresh<std::sync::Arc<Vec<files::Trace>>>>,
    query: String,
) -> Result<Elsewhere, String> {
    let token = searching.begin();

    let (files_settings, browser_settings) = {
        let prefs = state.inner.lock().await;
        (prefs.files.clone(), prefs.browsers.clone())
    };

    let scratch = crate::state::data_dir(&app).join("browser-copies");
    // Cloned out of the guard so the snapshot can be handed to a task that
    // outlives this borrow.
    let catalog = std::sync::Arc::clone(&catalog.inner.load_full());

    /*
     * The Recent folder, read at most once every few seconds.
     *
     * Windows keeps a shortcut in it to everything that has been opened, which
     * is the one source that knows a file was worked on this morning without
     * Sill having watched anybody work. It is a directory listing rather than
     * an index, so it is held for the length of a summon rather than read per
     * keystroke, and the `Arc` is what makes taking it out of the cache a
     * pointer rather than three hundred strings.
     */
    let traces = recent.get(|| {
        std::sync::Arc::new(match files::recent_folder() {
            Some(folder) => files::traces(&folder, files::RECENT_MOST),
            None => Vec::new(),
        })
    });

    // Together rather than one after the other. Neither needs the other's
    // answer, and the browser search used to wait out the file search first.
    //
    // Timed one at a time rather than as a pair, which is the whole reason
    // they are separately named here: they run at the same time, so a total
    // for both would be whichever of them is slower and would never say which.
    let clock = timings.inner();
    // The same settings answer two questions here, so each half takes its own
    // copy rather than one borrowing while the other moves.
    let tab_settings = browser_settings.clone();
    let (files, pages, tabs) = tokio::join!(
        async {
            let _timing = clock.timing("files");
            matching_files(&query, files_settings, catalog, traces, &searching, token).await
        },
        async {
            let _timing = clock.timing("browser pages");
            matching_pages(&query, browser_settings, scratch, &searching, token).await
        },
        async {
            let _timing = clock.timing("browser tabs");
            matching_tabs(&query, tab_settings, &searching, token).await
        },
    );

    Ok(Elsewhere {
        files: files?,
        pages: pages?,
        tabs: tabs?,
    })
}

/// Files matching a query, from Sill's index and from Everything.
async fn matching_files(
    query: &str,
    settings: crate::preferences::FileSearch,
    catalog: std::sync::Arc<crate::catalog::Catalog>,
    traces: std::sync::Arc<Vec<files::Trace>>,
    searching: &crate::state::Searching,
    token: u64,
) -> Result<Vec<files::FileHit>, String> {
    let query = query.to_string();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    let wanted = settings.max_results as usize;

    /*
     * What the query asked to look for inside the files themselves.
     *
     * Taken out here rather than inside the index, because it is the one
     * operator the index cannot answer: it holds names. What it does instead
     * is narrow the field to something worth opening, so the candidate list
     * is asked to be as wide as the content search is allowed to read rather
     * than as wide as the window will draw.
     */
    let bounds = crate::content::Bounds::default();
    let content = catalog::operators(&query).1.content().map(str::to_string);
    let looking_at = match content {
        Some(_) => wanted.max(bounds.files),
        None => wanted,
    };

    // Sill's own index first. It knows the folders somebody actually works in
    // and it answers in a few milliseconds without a second program being
    // installed, so it is the answer rather than the fallback.
    //
    // Narrowed by the same setting that narrows the other source. It says
    // "only show results in", and a filter that only applied to one of two
    // sources would be a setting that half worked, which is worse than one
    // that does not exist.
    let ours = catalog.search(query.trim(), looking_at, &settings.only_in);

    /*
     * Then what was open lately, which is a different question.
     *
     * The index answers "what is on this machine called that". The Recent
     * folder answers "what did you have open", and the two disagree often
     * enough to be worth both: a document under a folder nobody added as a
     * root is not in the index at all, and the thing somebody wants is usually
     * the thing they were just looking at.
     *
     * **A query that used an operator sits this out.** `ext:`, `size:` and
     * `date:` are questions about a file's metadata, answered from numbers the
     * index holds. The Recent folder holds shortcuts, so answering the same
     * question of it would mean opening every one of them and then asking the
     * disk about whatever each points at. Half-applying the operators would be
     * worse: a `size:>1mb` that quietly listed small files from one source is a
     * filter that cannot be trusted, which is worse than one that says less.
     */
    let ours = if catalog::operators(&query).1.asked_for_nothing() {
        let lately = files::from_recent(&traces, query.trim(), wanted, &settings.only_in);
        blend(ours, lately, query.trim(), wanted)
    } else {
        ours
    };

    /*
     * Then, only when asked, what is inside them.
     *
     * On a worker thread because it reads files, and carrying its own claim
     * to still being wanted so a query overtaken while it reads stops within
     * one file rather than finishing its two hundred. The bounds are in
     * `content`; what is decided here is only which files it looks at.
     */
    let ours = match content {
        None => ours,
        Some(needle) => {
            let claim = searching.claim(token);
            let paths: Vec<String> = ours.iter().map(|hit| hit.path.clone()).collect();

            let found = tokio::task::spawn_blocking(move || {
                crate::content::matching(
                    &paths,
                    &needle,
                    bounds,
                    |path, most| crate::content::head_of(path, most),
                    || claim.still_wanted(),
                )
            })
            .await
            .map_err(|err| format!("looking inside those files failed: {err}"))?;

            /*
             * In the order the content search found them, which is the order
             * the index offered them, so a name that matched well stays above
             * one that matched poorly. The line goes on the row: it is what
             * says whether this is the right file, and the path is already
             * the thing every other file row shows.
             */
            let mut lines: std::collections::HashMap<String, String> =
                found.into_iter().map(|one| (one.path, one.line)).collect();

            ours.into_iter()
                .filter_map(|mut hit| {
                    let line = lines.remove(&hit.path)?;
                    hit.snippet = Some(line);
                    Some(hit)
                })
                .collect()
        }
    };

    /*
     * Our own index is a few milliseconds and has already run. The other one
     * is a different matter, so this is where an overtaken search stops.
     *
     * The rows found so far are still returned rather than thrown away: they
     * cost nothing more, and the window will discard them if it has moved on.
     */
    if !searching.is_current(token) {
        return Ok(ours.into_iter().take(wanted).collect());
    }

    // Then a whole-volume indexer, when one is running. It sees the rest of
    // the machine, which our index deliberately does not.
    let scoped = files::scope(&query, &settings.only_in);
    let theirs = tokio::task::spawn_blocking(move || {
        files::search_with(
            &scoped,
            wanted,
            settings.match_path,
            settings.match_case,
            settings.regex,
        )
    })
    .await
    .unwrap_or_default();

    Ok(merge(ours, theirs, wanted))
}

/// Two ranked lists of files made into one ranking.
///
/// Not concatenated, and the difference matters. Sill's index and the Recent
/// folder both rank with `registry::match_name`, so their classes are directly
/// comparable, and putting one list after the other would sit a weak match from
/// the index above an exact one from the Recent folder. That is the same fault
/// `search_commands` had while open windows were a second list appended to the
/// results, and the note there says it: two lists concatenated is not a ranking.
///
/// The sort is stable and the index goes in first, so two rows of equal class
/// and equal length keep the index's own order and its tie-break on the path.
///
/// [`merge`] below is the other case and stays as it is: a whole-volume indexer
/// ranks by its own rules, which are not these, so its answers fill in after
/// rather than being sorted against them.
pub fn blend(
    ours: Vec<files::FileHit>,
    lately: Vec<files::FileHit>,
    query: &str,
    limit: usize,
) -> Vec<files::FileHit> {
    if lately.is_empty() {
        return ours.into_iter().take(limit).collect();
    }

    let needle: Vec<char> = query.to_lowercase().chars().collect();

    let mut ranked: Vec<(registry::MatchClass, usize, files::FileHit)> = ours
        .into_iter()
        .chain(lately)
        .map(|hit| {
            // A row that reached here matched at the source, so a miss means
            // the two are asking slightly different questions rather than that
            // the row is wrong. Last is the honest place for it.
            let class = registry::match_name(&needle, &hit.name)
                .map(|(class, _)| class)
                .unwrap_or(registry::MatchClass::TitleTypo);

            (class, hit.name.chars().count(), hit)
        })
        .collect();

    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(limit.min(ranked.len()));

    for (_, _, hit) in ranked {
        if out.len() >= limit {
            break;
        }

        if seen.insert(hit.path.to_lowercase()) {
            out.push(hit);
        }
    }

    out
}

/// Puts two sets of file results together without repeating anything.
///
/// Ours first and in its own order, because it ranks with the same code as
/// every other row and a whole-volume indexer has its own idea of relevance
/// that does not agree. Theirs fills the rest, which is where anything outside
/// the indexed folders comes from.
///
/// Paths are compared case-insensitively: Windows does, and the same file
/// arriving from both sources under different capitalisation would otherwise
/// be listed twice.
pub fn merge(
    ours: Vec<files::FileHit>,
    theirs: Vec<files::FileHit>,
    limit: usize,
) -> Vec<files::FileHit> {
    let mut seen: std::collections::HashSet<String> =
        ours.iter().map(|hit| hit.path.to_lowercase()).collect();
    let mut out = ours;

    for hit in theirs {
        if out.len() >= limit {
            break;
        }

        if seen.insert(hit.path.to_lowercase()) {
            out.push(hit);
        }
    }

    out.truncate(limit);
    out
}

/// What is stopping file search from answering, if anything.
///
/// Asked when the launcher is summoned, not per keystroke. The answer only
/// changes when a program starts or stops, and rule 18 is about not paying for
/// answers nothing asked a new question about.
///
/// Returns nothing when file search is switched off, because then there is no
/// problem to report: somebody turned it off on purpose.
#[tauri::command]
/// Finishes a path somebody is part way through typing.
///
/// Reads one folder, which is why it is a command rather than something the
/// window works out: the window cannot see the disk, and asking the file
/// index instead would answer with names from anywhere rather than from the
/// folder that was actually typed.
pub(crate) fn complete_path(typed: String) -> Option<String> {
    crate::complete::complete(&typed)
}

#[tauri::command]
pub(crate) async fn file_search_missing(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
) -> Result<Option<files::Missing>, String> {
    let enabled = state.inner.lock().await.files.enabled;

    Ok(files::missing(
        enabled,
        catalog.inner.load().len(),
        busy(&catalog),
    ))
}

/// Whether Sill has finished looking at what is installed for the first time.
///
/// Asked on mount and again when Rust says the index changed, which is two
/// calls per session rather than anything that repeats. What it decides is one
/// sentence: an empty root list while this is true is "still reading what is
/// installed", and an empty root list once it is false is "no results for that
/// word". They look the same on screen and mean opposite things, and the wrong
/// one on a first run tells somebody there is nothing here.
#[tauri::command]
pub(crate) fn index_building(registry: State<'_, RegistryState>) -> bool {
    registry
        .first_scan
        .load(std::sync::atomic::Ordering::Acquire)
}

/// Starts the whole-drive indexer that is already on this machine.
///
/// Its own command rather than a branch of `start_file_search`, which answers
/// the question "what is stopping file search from answering" and rightly says
/// nothing at all when Sill's own index is working. This is the other case:
/// file search answers, and there is still a program sitting closed that would
/// see the rest of the machine.
#[tauri::command]
pub(crate) async fn start_everything() -> Result<String, String> {
    files::start().map(|()| "Starting whole drive search.".to_string())
}

/// Whether the index is being rebuilt right now.
fn busy(catalog: &CatalogState) -> bool {
    catalog.building.load(std::sync::atomic::Ordering::Acquire)
}

/// Does whatever the thing standing in the way needs.
///
/// One command rather than two, because the launcher offers one row and the
/// row does the right thing. Which of the two it is was already decided by
/// [`files::missing`], and asking again here keeps the decision in one place.
///
/// Nothing here installs anything. It used to, and the row that ran it said
/// something else entirely while it did; see the `Absent` arm below.
#[tauri::command]
pub(crate) async fn start_file_search(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
) -> Result<String, String> {
    let enabled = state.inner.lock().await.files.enabled;
    let indexed = catalog.inner.load().len();

    match files::missing(enabled, indexed, busy(&catalog)) {
        Some(files::Missing::Indexing) => Ok("Still reading your files.".to_string()),
        None => Ok("File search is already working.".to_string()),
        Some(files::Missing::Asleep) => {
            files::start().map(|()| "Starting file search.".to_string())
        }
        /*
         * Sill's own index, turned on.
         *
         * This used to run winget and install Everything, a program the row
         * does not mention: it says "Sill is not indexing any folders" and
         * offers to set that up, and choosing it opened a console window
         * installing something else. A launcher row that installs third party
         * software without saying so is not a row anybody should be able to
         * press by accident.
         *
         * Sill has an index of its own and this is what it is for. Everything
         * is still used when it happens to be running, and adding it is a
         * choice somebody can make in Settings, where the drive list already
         * lives and where it can be described honestly.
         */
        Some(files::Missing::Absent) => {
            let roots = {
                let mut prefs = state.inner.lock().await;
                prefs.files.enabled = true;
                prefs.files.index = true;

                prefs
                    .save(&state.path)
                    .map_err(|err| format!("Could not save: {err}"))?;

                prefs.files.indexed_roots()
            };

            catalog.rebuild(roots);
            Ok("Reading your files. This happens once.".to_string())
        }
    }
}

/// Every mounted drive, and whether Sill is indexing it.
///
/// Asked when the settings that show them are opened, never on a timer. A
/// drive appearing is something a person did, and they are looking at the
/// list when they do it.
#[tauri::command]
pub(crate) async fn list_drives(
    state: State<'_, PrefsState>,
) -> Result<Vec<crate::catalog::Drive>, String> {
    let roots = state.inner.lock().await.files.indexed_roots();

    Ok(crate::catalog::drives(&roots))
}

/// Starts or stops indexing one folder, and rebuilds either way.
///
/// One command for both directions because the settings offer one switch. What
/// it does is decided by what the folder is now, not by what the window
/// believed when it drew itself.
#[tauri::command]
pub(crate) async fn index_folder(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
    watching: State<'_, crate::state::Watching>,
    path: String,
    wanted: bool,
) -> Result<Vec<String>, String> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err("No folder given.".to_string());
    }

    let roots = {
        let mut prefs = state.inner.lock().await;

        // Written into the list as it was given, but compared without case or
        // trailing separators, since `C:/` and `C:\` are the same folder and
        // adding both would index it twice.
        let already = prefs
            .files
            .roots
            .iter()
            .position(|root| crate::catalog::same_folder(root, &path));

        match (wanted, already) {
            (true, None) => prefs.files.roots.push(path),
            (false, Some(at)) => {
                prefs.files.roots.remove(at);
            }
            // Already as asked. Falling through would rebuild for nothing.
            _ => return Ok(prefs.files.roots.clone()),
        }

        // Empty means the home folder, which is not the same as indexing
        // nothing. Somebody who removes their last root means to stop, so it
        // is written down rather than left to be read as the default.
        if prefs.files.roots.is_empty() {
            prefs.files.index = false;
        } else {
            prefs.files.index = true;
        }

        // Reported rather than dropped. A change that cannot be written down
        // comes back on the next start, and silently indexing a folder
        // somebody removed is worse than saying the save failed.
        prefs
            .save(&state.path)
            .map_err(|err| format!("Could not save: {err}"))?;

        prefs.files.clone()
    };

    let indexed = roots.indexed_roots();

    // The watcher follows the folders. Without this it kept watching whatever
    // the list held at startup, so a folder added here was walked once and
    // then never noticed again, and a folder removed went on waking the
    // index up.
    watching.re_root(catalog.inner().clone(), indexed.clone());
    catalog.rebuild(indexed);

    Ok(roots.roots)
}

/// Opens a file or folder in its default application.
#[tauri::command]
pub(crate) async fn open_path(path: String) -> Result<(), String> {
    // The extension host reaches this one, so the argument is whatever an
    // extension put in an action, and an extension is third-party code.
    let path = crate::reach::target(&path)?;

    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{splice_suggestions, TabRow};
    use crate::registry::{CommandRecord, MatchClass, RankedCommand};

    /// A row's entrypoint is read back by the action that runs it.
    ///
    /// The one seam in this feature where a mistake is silent: the row is
    /// written here and parsed in `sill.browser.tab.focus`, and if the two
    /// disagree the row is simply refused with "is not a tab". Both sides are
    /// `Where`'s own, which is what this holds in place.
    #[test]
    fn a_tab_row_carries_something_the_action_can_read_back() {
        for title in [
            "Plain",
            "",
            // Chromium's real tab names, colons and all.
            "Alpha Tab - Memory usage - 21.0 MB",
            "Comparing v3.17.16...v3.18.0 \u{b7} cline/cline",
            "12:34:56",
        ] {
            let tab = crate::uia::Tab {
                browser: "Zen".to_string(),
                program: None,
                window: -4242,
                index: 9,
                title: title.to_string(),
                active: true,
                key: "42.16190184.4.0.0.753".to_string(),
            };

            let located = tab.located();
            let row = TabRow::from(tab);

            assert_eq!(
                crate::uia::Where::parse(&row.entrypoint),
                Some(located),
                "the row written for {title:?} is not one the action can read"
            );
        }
    }

    /// A tab with no identifier still produces a readable row.
    ///
    /// The fallback path, which is what a browser that will not name its own
    /// elements gets. An empty field in the middle of a written record is
    /// exactly the case a parser gets wrong.
    #[test]
    fn a_tab_the_browser_would_not_name_still_writes_a_row() {
        let tab = crate::uia::Tab {
            browser: "Chrome".to_string(),
            program: Some(r"C:\chrome.exe".to_string()),
            window: 7,
            index: 0,
            title: "Inbox".to_string(),
            active: false,
            key: String::new(),
        };

        let located = tab.located();
        let row = TabRow::from(tab);

        assert_eq!(crate::uia::Where::parse(&row.entrypoint), Some(located));
        assert_eq!(row.id, "browser-tab:7:");
    }

    /// A result that matched by name, or one that merely matched.
    ///
    /// `ExactTitle` and `TitleSubsequence` are the two sides of `is_strong`,
    /// which is the only thing the placement rule reads.
    fn result(id: &str, strong: bool) -> RankedCommand {
        RankedCommand {
            class: if strong {
                MatchClass::ExactTitle
            } else {
                MatchClass::TitleSubsequence
            },
            score: 0,
            matched: Vec::new(),
            command: CommandRecord {
                id: id.to_string(),
                extension: "test".into(),
                extension_title: "Test".into(),
                command: "run".into(),
                title: id.to_string(),
                subtitle: String::new(),
                description: String::new(),
                mode: "app".into(),
                entrypoint: String::new(),
                keywords: Vec::new(),
                icon: None,
                panel: None,
                preferences: serde_json::Value::Null,
                manifest: None,
                toggle: None,
            },
        }
    }

    fn ids(rows: &[RankedCommand]) -> Vec<String> {
        rows.iter().map(|row| row.command.id.clone()).collect()
    }

    #[test]
    fn results_found_by_name_keep_the_top() {
        let mut results = vec![
            result("named", true),
            result("named too", true),
            result("loose", false),
        ];

        splice_suggestions(&mut results, vec![result("emoji", true)]);

        assert_eq!(ids(&results), ["named", "named too", "emoji", "loose"]);
    }

    /// The measurement this exists for.
    ///
    /// Typing `tada` matched eighty-four things in the index, every one a
    /// coincidence of spelling, and the emoji somebody had plainly named
    /// landed eighty-fifth, where Enter opened a Sill setting instead.
    #[test]
    fn when_the_index_only_half_recognised_the_query_the_named_result_leads() {
        let mut results: Vec<RankedCommand> = (0..84)
            .map(|n| result(&format!("loose {n}"), false))
            .collect();

        splice_suggestions(&mut results, vec![result("emoji", true)]);

        assert_eq!(ids(&results)[0], "emoji");
    }

    #[test]
    fn with_nothing_to_add_nothing_moves() {
        let mut results = vec![result("a", false), result("b", false)];

        splice_suggestions(&mut results, Vec::new());

        assert_eq!(ids(&results), ["a", "b"]);
    }

    #[test]
    fn an_empty_list_is_just_the_suggestions() {
        let mut results = Vec::new();

        splice_suggestions(&mut results, vec![result("emoji", true)]);

        assert_eq!(ids(&results), ["emoji"]);
    }

    #[test]
    fn neither_list_is_reordered_within_itself() {
        let mut results = vec![
            result("s1", true),
            result("s2", true),
            result("w1", false),
            result("w2", false),
        ];

        splice_suggestions(&mut results, vec![result("e1", true), result("e2", true)]);

        assert_eq!(ids(&results), ["s1", "s2", "e1", "e2", "w1", "w2"]);
    }

    #[test]
    fn everything_strong_means_the_suggestions_go_last() {
        // Nothing to get above, so they read after what was asked for.
        let mut results = vec![result("a", true), result("b", true)];

        splice_suggestions(&mut results, vec![result("e", true)]);

        assert_eq!(ids(&results), ["a", "b", "e"]);
    }

    /// The property.
    ///
    /// A merge that drops a result is worse than one that orders them oddly,
    /// and much harder to notice.
    #[test]
    fn nothing_is_lost_whatever_the_mix() {
        let mut results = vec![
            result("a", true),
            result("b", false),
            result("c", true),
            result("d", false),
        ];

        splice_suggestions(&mut results, vec![result("e", true), result("f", true)]);

        assert_eq!(results.len(), 6);
        for id in ["a", "b", "c", "d", "e", "f"] {
            assert!(
                ids(&results).iter().any(|had| had == id),
                "{id} was dropped by the merge"
            );
        }
    }
}
