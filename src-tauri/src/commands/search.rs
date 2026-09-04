//! Searching, and opening what was found.

use tauri::{AppHandle, Manager, State};

use crate::state::{now_seconds, CatalogState, PrefsState, RegistryState};
use crate::{browsers, calculator, files, registry, windowing};

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

    let (excluded, hidden, pinned, tone) = {
        let prefs = prefs.inner.lock().await;
        (
            prefs.sources.excluded.clone(),
            prefs.sources.hidden.clone(),
            prefs.sources.pinned.clone(),
            prefs.emoji.tone,
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
     * Above everything, because when a query IS a sum, or a request for a
     * UUID, the answer is the only thing wanted.
     *
     * Both return nothing for the ninety-nine queries in a hundred that are
     * searches, so this costs those nothing. The calculator is asked first
     * because its gate is the stricter of the two: it has to guess whether a
     * string is arithmetic at all, while a utility is asked for by name.
     */
    let answer = calculator::evaluate(&query).or_else(|| crate::utilities::evaluate(&query));

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
    state: State<'_, RegistryState>,
    switches: State<'_, crate::state::Fresh<crate::system::Live>>,
    ids: Vec<String>,
) -> Result<Vec<Option<bool>>, String> {
    let index = state.index();
    let live = crate::system::live(&switches);

    Ok(crate::system::states_for(
        index
            .commands
            .iter()
            .map(|row| (row.id.as_str(), row.entrypoint.as_str())),
        &ids,
        &live,
    ))
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
    tokio::task::spawn_blocking(move || app.state::<crate::previews::Previews>().of(handle))
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

    // Together rather than one after the other. Neither needs the other's
    // answer, and the browser search used to wait out the file search first.
    //
    // Timed one at a time rather than as a pair, which is the whole reason
    // they are separately named here: they run at the same time, so a total
    // for both would be whichever of them is slower and would never say which.
    let clock = timings.inner();
    let (files, pages) = tokio::join!(
        async {
            let _timing = clock.timing("files");
            matching_files(&query, files_settings, catalog, &searching, token).await
        },
        async {
            let _timing = clock.timing("browser pages");
            matching_pages(&query, browser_settings, scratch, &searching, token).await
        },
    );

    Ok(Elsewhere {
        files: files?,
        pages: pages?,
    })
}

/// Files matching a query, from Sill's index and from Everything.
async fn matching_files(
    query: &str,
    settings: crate::preferences::FileSearch,
    catalog: std::sync::Arc<crate::catalog::Catalog>,
    searching: &crate::state::Searching,
    token: u64,
) -> Result<Vec<files::FileHit>, String> {
    let query = query.to_string();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    let wanted = settings.max_results as usize;

    // Sill's own index first. It knows the folders somebody actually works in
    // and it answers in a few milliseconds without a second program being
    // installed, so it is the answer rather than the fallback.
    //
    // Narrowed by the same setting that narrows the other source. It says
    // "only show results in", and a filter that only applied to one of two
    // sources would be a setting that half worked, which is worse than one
    // that does not exist.
    let ours = catalog.search(query.trim(), wanted, &settings.only_in);

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
/// The install runs in a console window somebody can see. A package manager
/// asks about agreements and can fail on a network, and a launcher that
/// swallowed all of that and reported nothing would be worse than one that
/// shows the same output a person would have seen typing it themselves.
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
    use super::splice_suggestions;
    use crate::registry::{CommandRecord, MatchClass, RankedCommand};

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
