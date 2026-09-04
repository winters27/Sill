//! Open browser tabs, read through UI Automation when somebody asks.
//!
//! A tab is the one thing a launcher on this platform cannot reach any other
//! way. Windows knows about windows; it knows nothing about the twenty
//! documents inside one of them. UI Automation is the only interface a browser
//! offers to that list without an extension installed into the browser itself,
//! which is a thing Sill deliberately does not ask anybody to do.
//!
//! ## Nothing lives between two questions
//!
//! This is the whole design, and it is the reason this module is written the
//! way it is rather than the way a UI Automation client is usually written.
//!
//! The normal shape is an `IUIAutomation` held for the life of the process,
//! event handlers registered on the windows of interest, and a cached tree
//! kept up to date by those events. Every part of that is refused here.
//! Registering **any** automation event handler makes Windows switch
//! accessibility instrumentation on inside every process the handler can
//! reach, and it stays on. The cost of that does not appear in Sill's numbers;
//! it appears in the browser's, which is worse, because it is invisible to the
//! person who could act on it.
//!
//! So: the automation object is created when a query arrives and released
//! before the answer is returned. No handler is ever registered. No element,
//! no pattern and no tree survives the call. Two reads of the same tab strip
//! share nothing.
//!
//! **Standing that client up costs 0.1 ms**, measured, against 35 for the walk
//! it is used for. Which is the answer to the obvious objection: a client held
//! for the life of the process would save a fifth of a millisecond per search
//! and would be a permanent object existing to be ready for a question nobody
//! has asked.
//!
//! The real cost of the rule is that a tab cannot be remembered as an element.
//! It is remembered as a [`Where`] instead, and activating it reads the strip
//! a second time. Two cheap reads rather than one read and a residency.
//!
//! ## The walk stops at the level that has the tabs
//!
//! A browser's own furniture is exposed to UI Automation whatever else is
//! going on, because the browser draws it. The **contents of a page** are a
//! different matter: Chromium builds that tree per renderer only when a client
//! asks, and asking is expensive inside the browser and lasts as long as the
//! tab does. Asking for every descendant of a browser window asks for that.
//!
//! Every tab of a window is a sibling of every other, so a breadth-first walk
//! that stops the moment a level yields a tab never goes below the strip.
//! [`TAB_STRIP_DEPTH`], [`ELEMENT_BUDGET`] and the refusal to descend through
//! a document are the belts on top of it, for a browser that has no strip at
//! all and would otherwise be walked to the bottom.
//!
//! ## Firefox is not Chromium
//!
//! Chromium's browser UI is exposed whether or not anybody is listening.
//! Firefox, and so Zen, keeps its whole accessibility engine switched off
//! until a client asks, and the asking is `WM_GETOBJECT`, which is what
//! `ElementFromHandle` sends.
//!
//! Reading Zen's tabs therefore switches Firefox accessibility on inside Zen,
//! **and it stays on**. Measured on this machine: the first read took 374 ms
//! and cost that browser about 10 MB in its window's process and 85 MB across
//! its content processes; a read twenty minutes later, with none in between,
//! took 40 ms rather than 374, which is what says the engine never went back
//! down. That is why the Firefox half has a setting of its own and why the
//! settings pane prints those numbers rather than describing them.
//!
//! `suite::real_tabs` is where all of that is measured, and it is the only
//! place it can be: none of it is a fact about this code.

use crate::browsers::Family;

/// A running browser window, and what it takes to read it.
///
/// Worked out from the ordinary window list rather than from a process scan,
/// so a browser that is installed and not running never appears here and
/// nothing below it ever runs. That is the whole of "a browser that is not
/// running costs nothing": the automation object is not created either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub window: isize,
    pub browser: String,
    pub family: Family,
    /// The browser's own program, for the row's icon.
    pub program: Option<String>,
}

/// Which of the open windows belong to a browser Sill knows.
///
/// `families` says which families are wanted. Firefox is separable from
/// Chromium here and nowhere else, because reading a Firefox window has a
/// cost inside Firefox that reading a Chromium one does not. See the module
/// note.
pub fn browser_windows(windows: &[crate::windowing::Window], families: &[Family]) -> Vec<Open> {
    windows
        .iter()
        .filter_map(|window| {
            let exe = std::path::Path::new(&window.app_path)
                .file_name()?
                .to_str()?;
            let (browser, family) = crate::browsers::known_by_exe(exe)?;

            families.contains(&family).then(|| Open {
                window: window.id,
                browser: browser.to_string(),
                family,
                program: (!window.app_path.is_empty()).then(|| window.app_path.clone()),
            })
        })
        .collect()
}

/// One open tab of one browser.
///
/// Not the shape the window is handed. That is `commands::search::TabRow`,
/// which carries a written `Where` rather than the pieces of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// What the browser calls itself, which is the group the row appears under.
    pub browser: String,
    /// The program behind it, so the row wears the browser's own mark rather
    /// than Sill's. Same rule as a history hit.
    pub program: Option<String>,
    /// The top-level window holding the strip this tab is in.
    pub window: isize,
    /// Where in that strip it sits, counting from zero.
    pub index: usize,
    pub title: String,
    /// Already the tab in front in its own window.
    pub active: bool,
    /// What the browser calls this element, for finding it again.
    ///
    /// Empty when the browser would not say, which is not fatal: the position
    /// and the title still find it. See [`pick`].
    pub key: String,
}

impl Tab {
    /// The description this tab's row carries, so it can be found again.
    pub fn located(&self) -> Where {
        Where {
            window: self.window,
            index: self.index,
            title: self.title.clone(),
            key: self.key.clone(),
        }
    }
}

/// Where a tab was when it was listed, written down so it can be found again.
///
/// Deliberately not an element. See the module note: nothing survives a query,
/// so the row carries a description rather than a reference, and the
/// description has to be enough to re-find the tab in a strip that may have
/// changed since.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Where {
    pub window: isize,
    pub index: usize,
    pub title: String,
    pub key: String,
}

impl Where {
    /// Written into a row's entrypoint.
    ///
    /// The title goes last and is never escaped, because a title may hold any
    /// character at all including the separator, and the parser below stops
    /// splitting after four fields for exactly that reason. A scheme that
    /// escaped it would be a second thing to keep in step with this one.
    ///
    /// Everything before the title is digits, dots and minus signs, so no
    /// field but the last can ever contain the separator.
    pub fn to_entrypoint(&self) -> String {
        format!("{}:{}:{}:{}", self.window, self.index, self.key, self.title)
    }

    /// Reads one back, or nothing if the text is not one.
    pub fn parse(text: &str) -> Option<Where> {
        let mut parts = text.splitn(4, ':');
        let window = parts.next()?.parse::<isize>().ok()?;
        let index = parts.next()?.parse::<usize>().ok()?;
        let key = parts.next()?.to_string();
        let title = parts.next()?.to_string();

        // A key is a run of numbers joined by dots, or nothing at all. Anything
        // else is a mangled entrypoint rather than a browser that declined to
        // identify a tab, and reading it as the latter would silently fall back
        // to matching by title on text that is not a title either.
        if !key.is_empty() && !key.split('.').all(|part| part.parse::<i32>().is_ok()) {
            return None;
        }

        Some(Where {
            window,
            index,
            key,
            title,
        })
    }
}

/// A tab as the strip has it right now, for [`pick`] to choose between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub key: String,
    pub title: String,
}

/// Which tab in a strip is the one that was asked for.
///
/// The strip is read a second time to activate, and by then it may not be the
/// strip that was listed: tabs get opened, closed, dragged and renamed, and a
/// page that finished loading renames itself without anybody touching it.
///
/// **A position is not an identity and a title is not an identity.** Position
/// alone activates whatever slid into that slot, which is the failure that
/// makes a feature like this worse than not having it. A title alone picks the
/// wrong one of two tabs on the same site, and worse, a Chromium tab's name
/// carries that tab's memory use, so the title read a second later is not even
/// the same string.
///
/// So the browser's own identifier decides, and it decides alone: if a key was
/// recorded and no tab now carries it, that tab has gone, and the position it
/// used to be at belongs to somebody else.
///
/// The position and title are the fallback for a provider that will not give
/// an identifier, which is why they are still recorded. There, the position
/// wins while it still holds the title, then the nearest tab that does, then
/// nothing.
pub fn pick(strip: &[Found], want: &Where) -> Option<usize> {
    if !want.key.is_empty() {
        return strip.iter().position(|tab| tab.key == want.key);
    }

    if strip
        .get(want.index)
        .is_some_and(|tab| tab.title == want.title)
    {
        return Some(want.index);
    }

    strip
        .iter()
        .enumerate()
        .filter(|(_, tab)| tab.title == want.title)
        .min_by_key(|(at, _)| at.abs_diff(want.index))
        .map(|(at, _)| at)
}

/// How a tab scores against what somebody typed.
///
/// The same shape as a history hit's score, and deliberately so: a tab and a
/// remembered page are the same kind of thing to the person typing, and two
/// scoring schemes would sort them into two blocks by accident rather than by
/// relevance.
fn score(tab: &Tab, needle: &str) -> i32 {
    let title = tab.title.to_lowercase();

    let mut points = if title == needle {
        1000
    } else if title.starts_with(needle) {
        700
    } else if title
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(needle))
    {
        500
    } else if title.contains(needle) {
        300
    } else {
        return i32::MIN;
    };

    // A shorter title containing the query is a closer match than a long one.
    points -= (title.len() as i32).min(200);

    /*
     * The tab already in front loses a little.
     *
     * Not much, and not nothing. Somebody typing a launcher query for the tab
     * they are already looking at is the one case where the answer is "you are
     * already there", so it should not sit above the tab they cannot see.
     */
    if tab.active {
        points -= 20;
    }

    points
}

/// The best of them for a query, most useful first.
pub fn rank(mut tabs: Vec<Tab>, query: &str, limit: usize) -> Vec<Tab> {
    let needle = query.trim().to_lowercase();

    if needle.is_empty() {
        tabs.truncate(limit);
        return tabs;
    }

    let mut scored: Vec<(i32, Tab)> = tabs
        .drain(..)
        .filter_map(|tab| {
            let points = score(&tab, &needle);
            (points != i32::MIN).then_some((points, tab))
        })
        .collect();

    // Stable beneath the score, so two tabs that score the same keep the order
    // the browser has them in rather than an order that changes per keystroke.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(limit);
    scored.into_iter().map(|(_, tab)| tab).collect()
}

/// How deep below a browser window the tab strip is looked for.
///
/// Measured rather than guessed. Chromium buries the strip further than
/// anybody would predict: on this machine the tab items sit **nine** levels
/// below the window, under half a dozen anonymous panes. Twelve leaves room
/// for a browser that adds a layer and still stops long before the depth at
/// which a page's own structure lives.
const TAB_STRIP_DEPTH: usize = 12;

/// The most elements looked at while finding one window's tabs.
///
/// The stop that matters. Depth alone does not bound a walk, because a level
/// can be arbitrarily wide, and this walk runs against a program somebody else
/// wrote and can change. Reached, the answer is whatever was found so far
/// rather than an error: a partial tab list is still useful and an unbounded
/// read on a keystroke is not.
const ELEMENT_BUDGET: usize = 2000;

/// The most tabs read from one window.
///
/// A ceiling rather than a limit anybody meets. It exists so a browser session
/// restored with several hundred tabs cannot turn one keystroke into an
/// unbounded read.
const TABS_PER_WINDOW: usize = 300;

#[cfg(windows)]
pub use windows_impl::{activate, read};

#[cfg(all(windows, test))]
pub(crate) use windows_impl::{cost, dump};

#[cfg(windows)]
mod windows_impl {
    use super::{Tab, Where, ELEMENT_BUDGET, TABS_PER_WINDOW, TAB_STRIP_DEPTH};

    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationElement,
        IUIAutomationSelectionItemPattern, TreeScope_Children, UIA_ControlTypePropertyId,
        UIA_DocumentControlTypeId, UIA_NamePropertyId, UIA_SelectionItemPatternId,
        UIA_TabItemControlTypeId,
    };

    /// What every element in a walk is asked for, in one go.
    ///
    /// **This is not a cache in the sense rule 2 forbids.** It is created
    /// inside the call that uses it and dropped before that call returns, and
    /// it holds nothing between two queries. What it does is say, once,
    /// "whenever you hand me an element, hand me its type and its name with
    /// it", which turns three round trips into another program's process per
    /// element into one.
    ///
    /// The walk is where the whole read spends itself, measured, so this is
    /// where the only optimisation in this module belongs.
    fn asking_for(automation: &IUIAutomation) -> windows::core::Result<IUIAutomationCacheRequest> {
        // SAFETY: the request comes from the automation object and both are
        // released by Drop.
        unsafe {
            let request = automation.CreateCacheRequest()?;
            request.AddProperty(UIA_ControlTypePropertyId)?;
            request.AddProperty(UIA_NamePropertyId)?;
            Ok(request)
        }
    }

    /// Runs some work with COM up, and takes it down again.
    ///
    /// **Multi-threaded, unlike every other COM user in this codebase.** The
    /// others talk to an in-process audio API. This one makes calls that cross
    /// into another program's process, and a single-threaded apartment that
    /// does that without pumping messages is the classic way to hang. There is
    /// no message loop on a blocking pool thread, so the apartment has to be
    /// one that does not need one.
    ///
    /// `RPC_E_CHANGED_MODE` means somebody already put this thread in an
    /// apartment. That is fine to work in and must not be torn down here,
    /// which is what the flag carries.
    fn with_com<T>(work: impl FnOnce() -> windows::core::Result<T>) -> windows::core::Result<T> {
        // SAFETY: initialised and uninitialised on the same thread around the
        // whole call, and every interface is released by its own Drop before
        // the uninitialise below.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = work();

            if initialised {
                CoUninitialize();
            }

            result
        }
    }

    /// The name of an element, or an empty string.
    ///
    /// From what the walk already asked for where there is one, and from the
    /// element itself otherwise, so this reads correctly whether or not it was
    /// handed an element that came out of a cached walk.
    fn name_of(element: &IUIAutomationElement) -> String {
        // SAFETY: reads one property from a live element.
        unsafe {
            element
                .CachedName()
                .or_else(|_| element.CurrentName())
                .map(|name| name.to_string())
                .unwrap_or_default()
        }
    }

    /// The provider's own identifier for an element, as text.
    ///
    /// **This is what a tab is remembered by.** A title is not an identity: a
    /// page renames itself when it finishes loading, two tabs of the same site
    /// carry the same one, and a Chromium tab's accessible name has the tab's
    /// memory use appended to it, which is a different string every few
    /// seconds. A position is not an identity either, for the obvious reason.
    ///
    /// A runtime identifier is what UI Automation offers instead. It is
    /// assigned by whoever provides the element, it does not change while that
    /// element lives, and it is not reused by a later one. It means nothing
    /// outside this desktop session and is never persisted.
    fn runtime_id(element: &IUIAutomationElement) -> Option<String> {
        // SAFETY: the array comes from the call above, is read only between
        // the bounds it reports for the one dimension a runtime identifier
        // has, and is destroyed on every way out including the early ones.
        unsafe {
            let array = element.GetRuntimeId().ok()?;

            let read = (|| {
                let low = SafeArrayGetLBound(array, 1).ok()?;
                let high = SafeArrayGetUBound(array, 1).ok()?;
                let mut parts: Vec<String> = Vec::new();

                for at in low..=high {
                    let mut value = 0i32;
                    SafeArrayGetElement(array, &at, (&raw mut value).cast()).ok()?;
                    parts.push(value.to_string());
                }

                (!parts.is_empty()).then(|| parts.join("."))
            })();

            let _ = SafeArrayDestroy(array);
            read
        }
    }

    /// What kind of control an element is. See [`name_of`] on the two reads.
    fn control_type(element: &IUIAutomationElement) -> i32 {
        // SAFETY: reads one property from a live element.
        unsafe {
            element
                .CachedControlType()
                .or_else(|_| element.CurrentControlType())
                .map(|it| it.0)
                .unwrap_or(0)
        }
    }

    /// Every tab item under a window, in the order the strip has them.
    ///
    /// Breadth first, and it stops as soon as a level has produced any tab.
    /// **That is the important clause, not the depth.** Every tab of a window
    /// is a sibling of every other, so the level holding the first one holds
    /// them all, and there is never a reason to look below it. A browser
    /// window's whole tree runs to thousands of elements once a page is
    /// involved; the furniture above the strip runs to a few dozen.
    ///
    /// Bounded three ways beyond that: a depth, an element budget, and a tab
    /// count. See the module note for why a read on a keystroke is the one
    /// place a tree walk has to be able to say "enough".
    fn tab_items(
        automation: &IUIAutomation,
        asking: &IUIAutomationCacheRequest,
        root: &IUIAutomationElement,
    ) -> Vec<IUIAutomationElement> {
        // SAFETY: everything below comes from the automation object passed in
        // and releases on Drop.
        unsafe {
            let Ok(anything) = automation.CreateTrueCondition() else {
                return Vec::new();
            };

            let mut found = Vec::new();
            let mut level = vec![root.clone()];
            let mut seen = 0usize;

            for _ in 0..TAB_STRIP_DEPTH {
                let mut next = Vec::new();

                for parent in &level {
                    /*
                     * A whole level of children in one crossing.
                     *
                     * Asking for the first child and then for each next
                     * sibling is the obvious way to write this and it was
                     * measurably the wrong one: every step is a call into the
                     * browser's process, and the walk was **39 of the 43
                     * milliseconds** a read took. Asking for all the children
                     * of a node at once, with the properties already attached,
                     * makes a level cost one crossing instead of one per
                     * element.
                     */
                    let Ok(children) =
                        parent.FindAllBuildCache(TreeScope_Children, &anything, asking)
                    else {
                        continue;
                    };

                    let count = children.Length().unwrap_or(0);

                    for at in 0..count {
                        let Ok(one) = children.GetElement(at) else {
                            continue;
                        };

                        seen += 1;

                        let kind = control_type(&one);

                        if kind == UIA_TabItemControlTypeId.0 {
                            found.push(one);

                            if found.len() >= TABS_PER_WINDOW {
                                return found;
                            }
                        } else if kind != UIA_DocumentControlTypeId.0 {
                            // A document is a page. Everything else is
                            // furniture, and furniture is what a strip is
                            // made of.
                            next.push(one);
                        }

                        if seen >= ELEMENT_BUDGET {
                            return found;
                        }
                    }
                }

                if !found.is_empty() || next.is_empty() {
                    break;
                }

                level = next;
            }

            found
        }
    }

    /// Whether a tab is the one in front of its own window.
    fn is_selected(element: &IUIAutomationElement) -> bool {
        // SAFETY: the pattern is queried from a live element and released by
        // Drop; a tab item that has no selection pattern returns an error
        // rather than a null, which is what the `ok()` covers.
        unsafe {
            element
                .GetCurrentPattern(UIA_SelectionItemPatternId)
                .ok()
                .and_then(|pattern| pattern.cast::<IUIAutomationSelectionItemPattern>().ok())
                .and_then(|selection| selection.CurrentIsSelected().ok())
                .is_some_and(|it| it.as_bool())
        }
    }

    /// The open tabs of the given browser windows.
    ///
    /// One automation object for the whole call and none afterwards. Called
    /// with an empty list it does not create one at all, which is what makes a
    /// machine with no browser running cost nothing: the caller filters the
    /// window list first and this never runs.
    pub fn read(windows: &[super::Open]) -> Vec<Tab> {
        if windows.is_empty() {
            return Vec::new();
        }

        with_com(|| {
            // SAFETY: every interface below comes from the call above it and
            // releases on Drop before COM is torn down.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                let mut out = Vec::new();

                for open in windows {
                    let root = match automation.ElementFromHandle(HWND(open.window as *mut _)) {
                        Ok(root) => root,
                        // A window that closed between being listed and being
                        // read. The rest of the browsers still answer.
                        Err(_) => continue,
                    };

                    for (index, item) in tab_items(&automation, &asking, &root)
                        .into_iter()
                        .enumerate()
                    {
                        let title = name_of(&item);

                        if title.is_empty() {
                            continue;
                        }

                        out.push(Tab {
                            browser: open.browser.clone(),
                            program: open.program.clone(),
                            window: open.window,
                            index,
                            title,
                            active: is_selected(&item),
                            key: runtime_id(&item).unwrap_or_default(),
                        });
                    }
                }

                Ok(out)
            }
        })
        .unwrap_or_default()
    }

    /// Brings one tab to the front of its window, and the window to the front.
    ///
    /// Reads the strip again rather than trusting the position it was listed
    /// at. See [`super::pick`] for why, and for what happens when the tab that
    /// was listed is no longer there: this returns an error saying so, because
    /// activating the wrong tab is worse than activating none.
    pub fn activate(want: &Where) -> Result<(), String> {
        with_com(|| {
            // SAFETY: as `read`.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                let root = automation.ElementFromHandle(HWND(want.window as *mut _))?;
                let items = tab_items(&automation, &asking, &root);

                let strip: Vec<super::Found> = items
                    .iter()
                    .map(|item| super::Found {
                        key: runtime_id(item).unwrap_or_default(),
                        title: name_of(item),
                    })
                    .collect();

                let Some(at) = super::pick(&strip, want) else {
                    return Ok(Err(format!("{} is no longer open", want.title)));
                };

                let pattern = items[at]
                    .GetCurrentPattern(UIA_SelectionItemPatternId)?
                    .cast::<IUIAutomationSelectionItemPattern>()?;

                pattern.Select()?;
                Ok(Ok(()))
            }
        })
        .map_err(|err| format!("that browser would not answer: {err}"))?
        .and_then(|()| {
            // The tab is in front of its window; the window may be behind
            // three others. Both halves or the row did nothing visible.
            crate::windowing::focus(want.window)
        })
    }

    /// Where the time in a read goes, for a probe to print.
    ///
    /// The phases are separately interesting because they are separately
    /// avoidable. Standing the automation client up is the price of keeping
    /// nothing alive between queries and is paid once per query however many
    /// browsers are open; the walk and the per-tab reads are paid per window
    /// and per tab. Knowing which dominates is what says whether the "nothing
    /// lives between two questions" rule is expensive or free.
    #[cfg(test)]
    pub(crate) fn cost(windows: &[super::Open]) -> String {
        use std::time::Instant;

        let mut out = String::new();

        let _ = with_com(|| {
            // SAFETY: as `read`.
            unsafe {
                let at = Instant::now();
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                out.push_str(&format!("  client stood up   {:?}\n", at.elapsed()));

                for open in windows {
                    let at = Instant::now();
                    let root = automation.ElementFromHandle(HWND(open.window as *mut _))?;
                    out.push_str(&format!(
                        "  {} window reached {:?}\n",
                        open.browser,
                        at.elapsed()
                    ));

                    let at = Instant::now();
                    let items = tab_items(&automation, &asking, &root);
                    out.push_str(&format!(
                        "  {} strip found    {:?} ({} tabs)\n",
                        open.browser,
                        at.elapsed(),
                        items.len()
                    ));

                    let at = Instant::now();
                    for item in &items {
                        let _ = name_of(item);
                    }
                    out.push_str(&format!(
                        "  {} names          {:?}\n",
                        open.browser,
                        at.elapsed()
                    ));

                    let at = Instant::now();
                    for item in &items {
                        let _ = runtime_id(item);
                    }
                    out.push_str(&format!(
                        "  {} keys           {:?}\n",
                        open.browser,
                        at.elapsed()
                    ));

                    let at = Instant::now();
                    for item in &items {
                        let _ = is_selected(item);
                    }
                    out.push_str(&format!(
                        "  {} which is front {:?}\n",
                        open.browser,
                        at.elapsed()
                    ));
                }

                Ok(())
            }
        });

        out
    }

    /// Dumps a window's tree, for a probe to print.
    ///
    /// Only ever called from `suite::real_tabs`. It exists because the shape
    /// of these trees is the one thing that cannot be reasoned about from
    /// here: it is whatever two browser vendors happen to build this year.
    #[cfg(test)]
    pub(crate) fn dump(handle: isize, depth: usize) -> String {
        let mut out = String::new();

        let _ = with_com(|| {
            // SAFETY: as `read`.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                let root = automation.ElementFromHandle(HWND(handle as *mut _))?;

                let anything = automation.CreateTrueCondition()?;
                let mut level = vec![root];

                for at in 0..depth {
                    let mut next = Vec::new();

                    for parent in &level {
                        let Ok(children) =
                            parent.FindAllBuildCache(TreeScope_Children, &anything, &asking)
                        else {
                            continue;
                        };

                        for index in 0..children.Length().unwrap_or(0) {
                            let Ok(one) = children.GetElement(index) else {
                                continue;
                            };

                            out.push_str(&format!(
                                "{}type {} name {:?}",
                                "  ".repeat(at + 1),
                                control_type(&one),
                                name_of(&one)
                            ));

                            if control_type(&one) == UIA_TabItemControlTypeId.0 {
                                out.push_str(&format!(
                                    " id {:?} runtime {:?} class {:?}",
                                    one.CurrentAutomationId()
                                        .map(|it| it.to_string())
                                        .unwrap_or_default(),
                                    runtime_id(&one),
                                    one.CurrentClassName()
                                        .map(|it| it.to_string())
                                        .unwrap_or_default(),
                                ));
                            }

                            out.push('\n');

                            if control_type(&one) != UIA_DocumentControlTypeId.0 {
                                next.push(one.clone());
                            }
                        }
                    }

                    if next.is_empty() {
                        break;
                    }

                    level = next;
                }

                Ok(())
            }
        });

        out
    }
}

#[cfg(not(windows))]
pub fn read(_windows: &[Open]) -> Vec<Tab> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn activate(_want: &Where) -> Result<(), String> {
    Err("browser tabs are a Windows feature".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(index: usize, title: &str) -> Tab {
        Tab {
            browser: "Zen".to_string(),
            program: None,
            window: 42,
            index,
            title: title.to_string(),
            active: false,
            key: format!("42.7.{index}"),
        }
    }

    /// A strip as a provider that identifies its tabs reports it.
    fn keyed(titles: &[&str]) -> Vec<Found> {
        titles
            .iter()
            .enumerate()
            .map(|(at, title)| Found {
                key: format!("42.7.{at}"),
                title: (*title).to_string(),
            })
            .collect()
    }

    /// A strip as a provider that will not identify its tabs reports it.
    fn unkeyed(titles: &[&str]) -> Vec<Found> {
        titles
            .iter()
            .map(|title| Found {
                key: String::new(),
                title: (*title).to_string(),
            })
            .collect()
    }

    fn window(id: isize, path: &str) -> crate::windowing::Window {
        crate::windowing::Window {
            id,
            title: "a window".to_string(),
            app: "something".to_string(),
            app_path: path.to_string(),
            pid: 1,
            minimized: false,
            maximized: false,
            rect: crate::windowing::Rect::new(0, 0, 100, 100),
            monitor: 0,
            elsewhere: false,
            desktop: None,
        }
    }

    #[test]
    fn an_entrypoint_survives_the_round_trip() {
        for title in [
            "Plain",
            "",
            "One: two: three",
            "1234",
            "Trailing spaces   ",
            "unicode \u{2014} and \u{1f600}",
            // What a Chromium tab is really called, memory reading and all.
            "Alpha Tab - Memory usage - 21.0 MB",
        ] {
            for key in ["", "42.16190184.4.0.0.753", "-1.-2"] {
                let want = Where {
                    window: -1234,
                    index: 7,
                    title: title.to_string(),
                    key: key.to_string(),
                };

                let back = Where::parse(&want.to_entrypoint())
                    .unwrap_or_else(|| panic!("{title:?} with key {key:?} did not parse back"));

                assert_eq!(back, want, "{title:?} changed on the way round");
            }
        }
    }

    #[test]
    fn nonsense_is_not_a_tab() {
        assert_eq!(Where::parse(""), None);
        assert_eq!(Where::parse("nothing"), None);
        assert_eq!(Where::parse("12"), None);
        assert_eq!(Where::parse("12:notanumber:1.2:Title"), None);
        // Three fields, so no title at all, which is not an empty one.
        assert_eq!(Where::parse("12:3:1.2"), None);
        // A key that is not a run of numbers. Read as an empty key it would
        // quietly fall back to matching on a title that is not a title.
        assert_eq!(Where::parse("12:3:notakey:Title"), None);
    }

    #[test]
    fn the_browsers_own_identifier_decides() {
        let strip = keyed(&["A", "B", "C"]);
        let want = Where {
            window: 1,
            index: 0,
            title: "wrong".to_string(),
            key: "42.7.2".to_string(),
        };

        // Neither the position nor the title agrees, and neither gets a say.
        assert_eq!(pick(&strip, &want), Some(2));
    }

    #[test]
    fn a_renamed_tab_is_still_the_same_tab() {
        // Which is what a Chromium tab does every few seconds: its accessible
        // name carries its memory use.
        let strip = keyed(&["Alpha Tab - Memory usage - 22.4 MB", "Beta Tab"]);
        let want = Where {
            window: 1,
            index: 0,
            title: "Alpha Tab - Memory usage - 21.0 MB".to_string(),
            key: "42.7.0".to_string(),
        };

        assert_eq!(pick(&strip, &want), Some(0));
    }

    #[test]
    fn a_tab_whose_identifier_has_gone_activates_nothing() {
        let strip = keyed(&["A", "B", "C"]);
        let want = Where {
            window: 1,
            index: 1,
            // The title is still in the strip, at the position it was at. The
            // tab that had it was closed and another took the name. Activating
            // that one is the failure this whole design exists to refuse.
            title: "B".to_string(),
            key: "42.7.99".to_string(),
        };

        assert_eq!(pick(&strip, &want), None);
    }

    #[test]
    fn the_position_wins_while_it_still_holds_the_title() {
        let want = Where {
            window: 1,
            index: 1,
            title: "B".to_string(),
            key: String::new(),
        };

        assert_eq!(pick(&unkeyed(&["A", "B", "C"]), &want), Some(1));
    }

    #[test]
    fn a_tab_that_moved_is_found_by_its_title() {
        // One opened to its left, so everything slid right by one.
        let want = Where {
            window: 1,
            index: 1,
            title: "B".to_string(),
            key: String::new(),
        };

        assert_eq!(pick(&unkeyed(&["New", "A", "B", "C"]), &want), Some(2));
    }

    #[test]
    fn the_nearer_of_two_tabs_with_the_same_title_wins() {
        let strip = unkeyed(&["Inbox", "Other", "Inbox", "Inbox"]);

        let want = Where {
            window: 1,
            index: 3,
            title: "Inbox".to_string(),
            key: String::new(),
        };

        assert_eq!(pick(&strip, &want), Some(3));

        let want = Where {
            window: 1,
            index: 1,
            title: "Inbox".to_string(),
            key: String::new(),
        };

        // Position 1 no longer holds it, and 0 and 2 are equally far. The
        // first wins, so the answer does not depend on iteration order.
        assert_eq!(pick(&strip, &want), Some(0));
    }

    #[test]
    fn a_tab_that_closed_activates_nothing() {
        let want = Where {
            window: 1,
            index: 5,
            title: "Gone".to_string(),
            key: String::new(),
        };

        assert_eq!(pick(&unkeyed(&["A", "B"]), &want), None);
    }

    #[test]
    fn a_strip_that_emptied_activates_nothing() {
        let want = Where {
            window: 1,
            index: 0,
            title: "A".to_string(),
            key: "42.7.0".to_string(),
        };

        assert_eq!(pick(&[], &want), None);
    }

    #[test]
    fn a_row_carries_where_its_tab_was() {
        let one = tab(3, "Mail");
        let back = Where::parse(&one.located().to_entrypoint()).expect("parses");

        assert_eq!(back, one.located());
        assert_eq!(back.window, 42);
        assert_eq!(back.index, 3);
    }

    #[test]
    fn ranking_prefers_a_title_that_starts_with_the_query() {
        let ranked = rank(
            vec![
                tab(0, "The mail app"),
                tab(1, "Mail"),
                tab(2, "Something else about mail"),
            ],
            "mail",
            10,
        );

        assert_eq!(
            ranked.iter().map(|t| t.index).collect::<Vec<_>>(),
            vec![1, 0, 2],
            "an exact title should beat a word start, which should beat a \
             substring"
        );
    }

    #[test]
    fn ranking_drops_what_does_not_match_at_all() {
        let ranked = rank(vec![tab(0, "Mail"), tab(1, "Calendar")], "mail", 10);

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].title, "Mail");
    }

    #[test]
    fn ranking_puts_a_tab_already_in_front_below_one_that_is_not() {
        let mut front = tab(0, "Mail");
        front.active = true;

        let ranked = rank(vec![front, tab(1, "Mail")], "mail", 10);

        assert_eq!(
            ranked.iter().map(|t| t.active).collect::<Vec<_>>(),
            vec![false, true],
            "the tab somebody is already looking at is the one answer they \
             did not need"
        );
    }

    #[test]
    fn an_empty_query_keeps_the_browsers_own_order() {
        let ranked = rank(vec![tab(0, "A"), tab(1, "B"), tab(2, "C")], "  ", 2);

        assert_eq!(
            ranked.iter().map(|t| t.index).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn a_window_that_is_not_a_browser_is_not_read() {
        let windows = vec![
            window(1, r"C:\Program Files\Zen Browser\zen.exe"),
            window(2, r"C:\Windows\explorer.exe"),
            window(
                3,
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            ),
            window(4, ""),
        ];

        let open = browser_windows(&windows, &[Family::Chromium, Family::Firefox]);

        assert_eq!(
            open.iter().map(|one| one.window).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(open[0].browser, "Zen");
        assert_eq!(open[1].browser, "Edge");
    }

    #[test]
    fn a_family_that_was_not_asked_for_is_not_read() {
        let windows = vec![
            window(1, r"C:\Program Files\Zen Browser\zen.exe"),
            window(
                2,
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            ),
        ];

        assert_eq!(
            browser_windows(&windows, &[Family::Chromium])
                .iter()
                .map(|one| one.window)
                .collect::<Vec<_>>(),
            vec![2],
            "asking for Chromium alone must not reach a Firefox, because \
             reaching one switches its accessibility engine on"
        );

        assert!(browser_windows(&windows, &[]).is_empty());
    }
}
