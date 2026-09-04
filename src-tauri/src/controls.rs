//! Pressing a control in somebody else's window, by typing its name.
//!
//! A button, a checkbox, a menu item, a tab. Windows draws thousands of them
//! and offers exactly one way to reach one without a mouse: UI Automation's
//! `Invoke`, which asks the program that drew the control to do whatever
//! clicking it would have done. This is that, wired to the launcher's own
//! field, so a control is reached by typing part of its name.
//!
//! ## Why this is not synthesised input
//!
//! Sill has form here. Synthetic keystrokes sent at a window this process did
//! not open have landed in a document somebody was writing, and the whole of
//! `dialog.rs` exists because of it: `P8-07` addresses a named control's own
//! handle and never the keyboard.
//!
//! This is the same doctrine one layer up. Nothing is typed, no key is
//! synthesised and the mouse is not moved. A specific element is named, that
//! element is found again, and that element is asked to invoke itself. A
//! program that is not the one holding the named control cannot receive
//! anything from this, however wrong the name turns out to be.
//!
//! ## Identity is the runtime id, and the name as well
//!
//! `uia.rs` established the first half: a runtime identifier is what an
//! element is, because a name changes and a position is not an identity. All
//! of that holds here.
//!
//! **This adds the second half, and is deliberately stricter than a tab.** A
//! tab whose name changed is still the page somebody asked for: a Chromium tab
//! renames itself every few seconds because its accessible name carries its
//! own memory use. A *button* whose name changed is a different button.
//! Toolbar buttons are reused: one element is Play and then Pause, Save and
//! then Discard. So a row is honoured only when the identifier and the name
//! both still hold, and refuses otherwise.
//!
//! Pressing the wrong tab wastes a keystroke. Pressing the wrong button cannot
//! be taken back, which is why the two make opposite trade-offs about a name
//! that moved.
//!
//! ## Nothing lives between two questions
//!
//! Inherited whole from `uia.rs`, including the code: no automation object, no
//! element, no tree and no event handler survives a call, and none is ever
//! registered. A row carries a written [`Spot`] rather than a hold on an
//! element, and pressing it reads the window again.
//!
//! ## Nothing is read until somebody asks for it
//!
//! **This is not a source in the root list**, and that is a cost decision
//! rather than a presentation one. Reading a window's controls means walking
//! another program's tree across the process boundary; done on every keystroke
//! against whatever happened to be in front, it would be a launcher that
//! interrogates the window behind it while somebody searches for a calculator.
//! Worse against a Firefox-family browser, where the first read switches that
//! browser's accessibility engine on for the life of the process.
//!
//! So it is a view somebody opens. Until they do, this module runs no code at
//! all, which is what makes its idle cost exactly zero rather than nearly.
//!
//! ## What is offered, and what is not
//!
//! Only things that are pressed: buttons, checkboxes, radio buttons, menu
//! items, links, split buttons and tabs. Not list rows and not tree rows,
//! which are where a program's *contents* live rather than its furniture: an
//! inbox would arrive as four hundred rows named after somebody's mail.
//!
//! For the same reason the walk refuses to descend through a document, exactly
//! as the tab read does. A document is a web page or an editor's text, and
//! walking one both asks a browser to build a tree it otherwise would not and
//! turns what somebody is reading into launcher rows.
//!
//! ## The window being read is never the one in front
//!
//! Worth saying on its own, because it is the assumption a whole filter was
//! built on and it was wrong. The launcher takes the foreground when somebody
//! summons it, so the window whose controls are read is by definition the
//! window that just lost it, and a provider is allowed to describe its
//! elements differently in that state. Gecko does: see [`ready`]. Anything
//! judged here has to be judged about a window that is visible and behind.
//!
//! `suite::real_controls` is where the half of this that is a fact about
//! somebody else's program is measured, and it is the only place it can be.

/// One control of one window, as it is right now.
///
/// Not the shape a row is handed. That is a `CommandRecord`, built by the
/// command, carrying a written [`Spot`] rather than the pieces of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The top-level window this was found under.
    pub window: isize,
    /// What the program calls it, which is what somebody types to find it.
    pub name: String,
    /// What kind of control it is, for the row's label.
    pub kind: Kind,
    /// The provider's own identifier, which is what this control *is*.
    pub key: String,
}

impl Control {
    /// The description this control's row carries, so it can be found again.
    pub fn spotted(&self) -> Spot {
        Spot {
            window: self.window,
            key: self.key.clone(),
            name: self.name.clone(),
        }
    }
}

/// What sort of control a row is, in one word somebody would recognise.
///
/// An enum rather than the raw control type, so the one place that decides
/// which kinds are offered at all is the same place that decides what each is
/// called. A control type with no entry here is not pressable as far as this
/// module is concerned, and that is the whole filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Button,
    Checkbox,
    Radio,
    MenuItem,
    Link,
    SplitButton,
    Tab,
}

impl Kind {
    /// The word the row wears.
    pub fn said(self) -> &'static str {
        match self {
            Kind::Button => "Button",
            Kind::Checkbox => "Checkbox",
            Kind::Radio => "Option",
            Kind::MenuItem => "Menu item",
            Kind::Link => "Link",
            Kind::SplitButton => "Split button",
            Kind::Tab => "Tab",
        }
    }

    /// Which UI Automation control types are offered, and as what.
    ///
    /// The numbers are `UIA_*ControlTypeId`, written out rather than imported
    /// so that this table exists on every platform and can be tested on one
    /// without a desktop. They are a published, frozen part of the platform: a
    /// button has been 50000 since UI Automation shipped.
    ///
    /// **What is missing is the interesting half.** A list row and a tree row
    /// are both invokable and neither is here, because they are a program's
    /// contents rather than its furniture: offering them turns a mail client
    /// into four hundred rows named after somebody's mail, and turns this from
    /// "press a button" into "read what is on the screen".
    pub fn of(control_type: i32) -> Option<Kind> {
        Some(match control_type {
            50000 => Kind::Button,
            50002 => Kind::Checkbox,
            50005 => Kind::Link,
            50011 => Kind::MenuItem,
            50013 => Kind::Radio,
            50019 => Kind::Tab,
            50031 => Kind::SplitButton,
            _ => return None,
        })
    }
}

/// A document, which the walk refuses to descend through.
///
/// `UIA_DocumentControlTypeId`. Written out for the same reason the table
/// above is, and named separately because it is a rule rather than a row: a
/// document is a web page or an editor's text, and asking a browser for the
/// tree underneath one is both expensive inside that browser and a way of
/// turning what somebody is reading into launcher rows.
const DOCUMENT: i32 = 50030;

/// Where a control was, written down so it can be found again.
///
/// Deliberately not an element. See the module note: nothing survives a query,
/// so the row carries a description and pressing it reads the window a second
/// time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Spot {
    pub window: isize,
    /// The provider's own identifier. **Never empty**: see [`Spot::parse`].
    pub key: String,
    pub name: String,
}

impl Spot {
    /// Written into a row's entrypoint.
    ///
    /// The name goes last and is never escaped, because a control may be
    /// called anything at all including the separator, and the parser stops
    /// splitting after three fields for exactly that reason. Everything before
    /// it is digits, dots and minus signs.
    pub fn to_entrypoint(&self) -> String {
        format!("{}:{}:{}", self.window, self.key, self.name)
    }

    /// Reads one back, or nothing if the text is not one.
    ///
    /// **An empty identifier is not a spot**, which is where this parts
    /// company with a tab's. A tab with no identifier still has a position and
    /// a title to fall back on, and the worst a wrong answer does is show
    /// somebody a page they did not ask for. There is no acceptable fallback
    /// for a button: without the provider's own name for the element there is
    /// nothing distinguishing Save from Discard except a label the program is
    /// free to move, so the row does not exist at all.
    pub fn parse(text: &str) -> Option<Spot> {
        let mut parts = text.splitn(3, ':');
        let window = parts.next()?.parse::<isize>().ok()?;
        let key = parts.next()?.to_string();
        let name = parts.next()?.to_string();

        /*
         * A run of numbers joined by dots, which is what a runtime identifier
         * is. Anything else is a mangled entrypoint rather than a provider
         * that declined to identify a control.
         *
         * The emptiness half is deliberately redundant and it is said here so
         * nobody removes the other half believing this one carries it: an
         * empty key splits to one empty part, which does not parse as a
         * number, so the run-of-numbers test refuses it on its own. Sabotaged
         * by deleting `is_empty` and every test still passed, which is the
         * second guard doing the work rather than a weak test.
         */
        if key.is_empty() || !key.split('.').all(|part| part.parse::<i32>().is_ok()) {
            return None;
        }

        // A control nobody can name is a control nobody can type, so it was
        // never listed and an entrypoint claiming one is not one.
        if name.is_empty() {
            return None;
        }

        Some(Spot { window, key, name })
    }
}

/// A control as the window has it right now, for [`pick`] to choose between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub key: String,
    pub name: String,
}

/// Which control in a window is the one that was asked for.
///
/// **Both halves have to still hold.** The identifier says this is the same
/// element; the name says that element is still the thing the row promised.
/// Either alone is not enough, and the reasons are different.
///
/// The identifier alone is not enough because providers reuse elements. A
/// media player's transport button is Play and then Pause at one runtime id; a
/// dialog's button is Save and then, once something has changed, Discard.
/// Somebody who read "Save" and pressed Enter has agreed to save.
///
/// The name alone is not enough for the reason `uia::pick` gives at greater
/// length: a name is not an identity, two controls share one all the time, and
/// a program is free to rename anything.
///
/// There is no positional fallback of any kind, unlike a tab's. Nothing about
/// "the third button along" survives a window redrawing itself, and pressing
/// the wrong button is not a keystroke anybody can take back.
pub fn pick(controls: &[Found], want: &Spot) -> Option<usize> {
    controls
        .iter()
        .position(|one| one.key == want.key && one.name == want.name)
}

/// Whether a window belongs to this process.
///
/// The one thing this feature must never reach. Sill's own window is on screen
/// while somebody is choosing a row, so without this the launcher would offer
/// its own buttons, and worse, would offer a way for anything holding the
/// capability to press them. A pure function over two process ids so the rule
/// can be tested without a desktop; the call sites are held to consulting it
/// by `verify:source`.
pub fn is_ours(window_pid: u32, our_pid: u32) -> bool {
    window_pid == our_pid
}

/// How deep below a window a control is looked for.
///
/// Deeper than the tab strip's twelve, because a tab strip is one place and
/// this is everywhere: `P8-07` measured a Save As dialog's filename field four
/// levels down inside a `DUIViewWndClassName`, Chromium's tab items sit nine
/// below the window, and a modern application's toolbar sits under a stack of
/// anonymous panes that varies with its framework.
///
/// The element budget rather than this is the stop that actually binds. See
/// [`ELEMENT_BUDGET`].
const DEPTH: usize = 16;

/// The most elements looked at while reading one window.
///
/// The stop that matters, exactly as in `uia.rs`: depth alone does not bound a
/// walk, because a level can be arbitrarily wide, and this walk runs against a
/// program somebody else wrote. Reached, the answer is whatever was found so
/// far, because a partial list of buttons is useful and an unbounded read is
/// not.
const ELEMENT_BUDGET: usize = 4000;

/// The most controls returned for one window.
///
/// A ceiling rather than a limit anybody meets. A window with four hundred
/// pressable controls is not a window anybody finds anything in by scrolling,
/// and the field is how one is found here anyway.
const CONTROLS_PER_WINDOW: usize = 400;

#[cfg(windows)]
pub use windows_impl::{press, read};

#[cfg(all(windows, test))]
pub(crate) use windows_impl::switched_on;

#[cfg(windows)]
mod windows_impl {
    use super::{Control, Found, Kind, Spot, CONTROLS_PER_WINDOW, DEPTH, DOCUMENT, ELEMENT_BUDGET};

    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationElement,
        IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern,
        TreeScope_Children, UIA_ControlTypePropertyId, UIA_InvokePatternId,
        UIA_IsEnabledPropertyId, UIA_NamePropertyId, UIA_SelectionItemPatternId,
        UIA_TogglePatternId,
    };

    /// What every element in the walk is asked for, in one go.
    ///
    /// Three properties rather than the tab read's two, and for the same
    /// reason: each one asked for separately is a round trip into another
    /// program's process per element, and the walk is where the whole read
    /// spends itself.
    ///
    /// `IsOffscreen` was a fourth and was taken out rather than left unread.
    /// See [`ready`] for what it does to a Firefox.
    ///
    /// Not a cache in the sense rule 2 forbids. It is made inside the call that
    /// uses it and dropped before that call returns.
    fn asking_for(automation: &IUIAutomation) -> windows::core::Result<IUIAutomationCacheRequest> {
        // SAFETY: the request comes from the automation object and both are
        // released by Drop.
        unsafe {
            let request = automation.CreateCacheRequest()?;
            request.AddProperty(UIA_ControlTypePropertyId)?;
            request.AddProperty(UIA_NamePropertyId)?;
            request.AddProperty(UIA_IsEnabledPropertyId)?;
            Ok(request)
        }
    }

    /**
    Whether a control is worth offering: it can be pressed now.

    A greyed-out button is a real element that comes back from the walk, and a
    row for one is a row that does nothing when somebody presses Enter on it.
    Character Map's "Copy" is disabled until a character has been chosen, and
    it is correctly absent from the list until then.

    It defaults to "yes" when the provider will not answer, which is the right
    direction: a provider that does not implement `IsEnabled` is saying nothing
    rather than saying no, and dropping every control of such a program would
    be a window with no rows and no explanation.

    ## `IsOffscreen` was here and had to come out

    The obvious companion, and it makes this feature useless on a Firefox.
    Measured on this machine, against windows the probe opened, with each
    window **not** in the foreground, which is the only state that matters
    because the launcher has taken the foreground by the time this runs:

    | Browser | Controls found | Skipped as offscreen |
    | --- | --- | --- |
    | Edge | 24 | none; only "Back", correctly disabled |
    | Zen | **4** | every control Gecko provides, tabs included |

    The four Zen kept are Windows' own non-client buttons. Gecko reports its
    entire chrome as offscreen whenever the window is not in front, however
    visible it is; the same window brought forward answers with 17 and skips
    only Back and Forward, which really are disabled with no history.

    So the check is exactly wrong for the case it would run in. It is not that
    it misses a few controls on one browser: it removes every control the
    feature exists to reach, on the browser this machine's owner uses, and it
    would have looked like a working feature on Chromium.

    What is lost by dropping it is a row for a control scrolled out of view.
    That is a much smaller thing than it sounds: `Invoke` does not need a
    control to be visible, so pressing one still does what clicking it would,
    and this list is searched by typing rather than read down.
    */
    fn ready(element: &IUIAutomationElement) -> bool {
        // SAFETY: reads one cached property from a live element.
        unsafe {
            element
                .CachedIsEnabled()
                .or_else(|_| element.CurrentIsEnabled())
                .map(|it| it.as_bool())
                .unwrap_or(true)
        }
    }

    /// Refuses a window this process owns. See [`super::is_ours`].
    ///
    /// A window that has closed since it was listed is refused here too, and
    /// says so rather than being read as somebody else's: `find` answers with
    /// nothing for a handle that is no longer a window, and reading a stale
    /// handle is reading whatever Windows has since put there.
    fn refuse_our_own(window: isize) -> Result<(), String> {
        let Some(found) = crate::windowing::find(window) else {
            return Err("that window is not open any more".to_string());
        };

        // SAFETY: takes nothing, returns this process's id.
        let ours = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };

        if super::is_ours(found.pid, ours) {
            return Err("that is Sill's own window".to_string());
        }

        Ok(())
    }

    /// Every pressable control under a window, with the element behind it.
    ///
    /// Breadth first, and unlike the tab read it does not stop at the first
    /// level that yields something: a tab strip is one row of siblings, and a
    /// window's buttons are scattered through its whole tree. The element
    /// budget is what bounds this instead.
    ///
    /// The element is carried beside the description because pressing needs
    /// it, and walking a second time to fetch it would be a second traversal
    /// that could order itself differently from the first, which is a way of
    /// pressing the wrong control. It never leaves this module: [`read`] drops
    /// every one of them before it returns.
    fn walk(
        automation: &IUIAutomation,
        asking: &IUIAutomationCacheRequest,
        root: &IUIAutomationElement,
        window: isize,
    ) -> Vec<(Control, IUIAutomationElement)> {
        // SAFETY: everything below comes from the automation object passed in
        // and releases on Drop.
        unsafe {
            let Ok(anything) = automation.CreateTrueCondition() else {
                return Vec::new();
            };

            let mut found: Vec<(Control, IUIAutomationElement)> = Vec::new();
            let mut level = vec![root.clone()];
            let mut seen = 0usize;

            for _ in 0..DEPTH {
                let mut next = Vec::new();

                for parent in &level {
                    /*
                     * A whole level of children in one crossing.
                     *
                     * The same measurement `uia.rs` records: asking for a
                     * first child and then each next sibling makes every step
                     * a call into somebody else's process, and it was 39 of
                     * the 43 milliseconds a tab read took. Batching a level
                     * with the properties already attached took that to 34.
                     */
                    let Ok(children) =
                        parent.FindAllBuildCache(TreeScope_Children, &anything, asking)
                    else {
                        continue;
                    };

                    for at in 0..children.Length().unwrap_or(0) {
                        let Ok(one) = children.GetElement(at) else {
                            continue;
                        };

                        seen += 1;
                        let control_type = crate::uia::control_type(&one);

                        if let Some(kind) = Kind::of(control_type) {
                            let name = crate::uia::name_of(&one);

                            // A control nobody can name is a control nobody
                            // can type, and an identifier is what a row is.
                            // Either missing and there is no row to build.
                            if !name.is_empty() && ready(&one) {
                                if let Some(key) = crate::uia::runtime_id(&one) {
                                    found.push((
                                        Control {
                                            window,
                                            name,
                                            kind,
                                            key,
                                        },
                                        one.clone(),
                                    ));

                                    if found.len() >= CONTROLS_PER_WINDOW {
                                        return found;
                                    }
                                }
                            }
                        }

                        /*
                         * Down through everything except a document.
                         *
                         * A document is a page or an editor's text. Descending
                         * through one asks a browser to build a tree it would
                         * otherwise not build, per renderer, for as long as
                         * the tab lives, and it turns what somebody is reading
                         * into rows in a launcher.
                         *
                         * A control that is itself pressable is still
                         * descended through: a split button holds its menu,
                         * and a tab item in some frameworks holds its label.
                         */
                        if control_type != DOCUMENT {
                            next.push(one);
                        }

                        if seen >= ELEMENT_BUDGET {
                            return found;
                        }
                    }
                }

                if next.is_empty() {
                    break;
                }

                level = next;
            }

            found
        }
    }

    /// The controls of a window as descriptions, for [`super::pick`].
    fn described(found: &[(Control, IUIAutomationElement)]) -> Vec<Found> {
        found
            .iter()
            .map(|(control, _)| Found {
                key: control.key.clone(),
                name: control.name.clone(),
            })
            .collect()
    }

    /// The pressable controls of one window.
    ///
    /// One automation object for the whole call and none afterwards, exactly
    /// as the tab read does it and through the same code. Every element the
    /// walk held is dropped before this returns, which is the module's whole
    /// rule: what comes back is a list of descriptions.
    pub fn read(window: isize) -> Result<Vec<Control>, String> {
        refuse_our_own(window)?;

        crate::uia::with_com(|| {
            // SAFETY: every interface below comes from the call above it and
            // releases on Drop before COM is torn down.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                let root = automation.ElementFromHandle(HWND(window as *mut _))?;

                Ok(walk(&automation, &asking, &root, window)
                    .into_iter()
                    .map(|(control, _)| control)
                    .collect())
            }
        })
        .map_err(|err| format!("that window would not answer: {err}"))
    }

    /// Presses one control, having found it again.
    ///
    /// Reads the window a second time rather than trusting what the row was
    /// built from, and refuses outright when the control the row named is no
    /// longer there under that name. See [`super::pick`]: there is no
    /// second-best answer to "which button", so there is no fallback.
    pub fn press(want: &Spot) -> Result<(), String> {
        refuse_our_own(want.window)?;

        crate::uia::with_com(|| {
            // SAFETY: as `read`.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                let root = automation.ElementFromHandle(HWND(want.window as *mut _))?;
                let found = walk(&automation, &asking, &root, want.window);

                let Some(at) = super::pick(&described(&found), want) else {
                    return Ok(Err(format!("{} is not in that window any more", want.name)));
                };

                Ok(invoke(&found[at].1, &want.name))
            }
        })
        .map_err(|err| format!("that window would not answer: {err}"))?
    }

    /// Asks a control to do what clicking it would have done.
    ///
    /// Three ways, in the order a control offers them. Invoke is a press.
    /// Toggle is a checkbox, and comes second because a checkbox that offers
    /// both means the same thing by either. Select is a tab or a radio button,
    /// where the meaning is "choose this one" rather than "do this".
    ///
    /// A control offering none of the three refuses in words. That is a row
    /// that should not have been built, and saying so is how it gets noticed.
    fn invoke(element: &IUIAutomationElement, name: &str) -> Result<(), String> {
        // SAFETY: every pattern is queried from a live element and released by
        // Drop. A control that does not support one answers with an error
        // rather than a null, which is what each `ok()` covers.
        unsafe {
            if let Some(pattern) = element
                .GetCurrentPattern(UIA_InvokePatternId)
                .ok()
                .and_then(|it| it.cast::<IUIAutomationInvokePattern>().ok())
            {
                return pattern
                    .Invoke()
                    .map_err(|err| format!("{name} would not be pressed: {err}"));
            }

            if let Some(pattern) = element
                .GetCurrentPattern(UIA_TogglePatternId)
                .ok()
                .and_then(|it| it.cast::<IUIAutomationTogglePattern>().ok())
            {
                return pattern
                    .Toggle()
                    .map_err(|err| format!("{name} would not be switched: {err}"));
            }

            if let Some(pattern) = element
                .GetCurrentPattern(UIA_SelectionItemPatternId)
                .ok()
                .and_then(|it| it.cast::<IUIAutomationSelectionItemPattern>().ok())
            {
                return pattern
                    .Select()
                    .map_err(|err| format!("{name} would not be chosen: {err}"));
            }

            Err(format!(
                "{name} is not something that can be pressed from here"
            ))
        }
    }

    /// What a switch is set to now, for a probe to check that a press worked.
    ///
    /// A toggle state is the one observable a press has that is a property of
    /// the control rather than of a document, which is what makes it the thing
    /// a real probe can assert on without reading anybody's text.
    #[cfg(test)]
    pub(crate) fn switched_on(want: &Spot) -> Option<bool> {
        use windows::Win32::UI::Accessibility::ToggleState_On;

        crate::uia::with_com(|| {
            // SAFETY: as `read`.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)?;
                let asking = asking_for(&automation)?;
                let root = automation.ElementFromHandle(HWND(want.window as *mut _))?;
                let found = walk(&automation, &asking, &root, want.window);

                let Some(at) = super::pick(&described(&found), want) else {
                    return Ok(None);
                };

                Ok(found[at]
                    .1
                    .GetCurrentPattern(UIA_TogglePatternId)
                    .ok()
                    .and_then(|it| it.cast::<IUIAutomationTogglePattern>().ok())
                    .and_then(|pattern| pattern.CurrentToggleState().ok())
                    .map(|state| state == ToggleState_On))
            }
        })
        .unwrap_or(None)
    }
}

#[cfg(not(windows))]
pub fn read(_window: isize) -> Result<Vec<Control>, String> {
    Err("pressing a control is a Windows feature".to_string())
}

#[cfg(not(windows))]
pub fn press(_want: &Spot) -> Result<(), String> {
    Err("pressing a control is a Windows feature".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(pairs: &[(&str, &str)]) -> Vec<Found> {
        pairs
            .iter()
            .map(|(key, name)| Found {
                key: (*key).to_string(),
                name: (*name).to_string(),
            })
            .collect()
    }

    #[test]
    fn an_entrypoint_survives_the_round_trip() {
        for name in [
            "Save",
            "One: two: three",
            "1234",
            "Trailing spaces   ",
            "unicode \u{2014} and \u{1f600}",
            "Settings and more (Alt+F)",
        ] {
            for key in ["42.16190184.4.0.0.753", "-1.-2", "7"] {
                let want = Spot {
                    window: -1234,
                    key: key.to_string(),
                    name: name.to_string(),
                };

                let back = Spot::parse(&want.to_entrypoint())
                    .unwrap_or_else(|| panic!("{name:?} with key {key:?} did not parse back"));

                assert_eq!(back, want, "{name:?} changed on the way round");
            }
        }
    }

    #[test]
    fn nonsense_is_not_a_control() {
        assert_eq!(Spot::parse(""), None);
        assert_eq!(Spot::parse("nothing"), None);
        assert_eq!(Spot::parse("12"), None);
        // Two fields, so no name at all.
        assert_eq!(Spot::parse("12:1.2"), None);
        assert_eq!(Spot::parse("notawindow:1.2:Save"), None);
        assert_eq!(Spot::parse("12:notakey:Save"), None);
    }

    /// The difference between this and a tab, as a test.
    ///
    /// A tab with no identifier is still findable by position and title. A
    /// control with none is not a row at all, because there is nothing left
    /// that distinguishes Save from Discard.
    #[test]
    fn a_control_with_no_identifier_is_not_a_row() {
        assert_eq!(Spot::parse("12::Save"), None);
        assert_eq!(Spot::parse("12:1.2:"), None);
    }

    #[test]
    fn the_providers_own_identifier_finds_it() {
        let here = found(&[("1.1", "Copy"), ("1.2", "Paste"), ("1.3", "Select")]);

        let want = Spot {
            window: 1,
            key: "1.3".to_string(),
            name: "Select".to_string(),
        };

        assert_eq!(pick(&here, &want), Some(2));
    }

    /**
    The rule this module exists for, and the one place it is stricter than
    `uia::pick`.

    A toolbar button is reused: one element is Play and then Pause, Save and
    then Discard. Somebody who read "Save" on a row and pressed Enter has
    agreed to save, and pressing the element that used to say so is not the
    same act.
    */
    #[test]
    fn a_control_that_was_renamed_is_a_different_control() {
        let here = found(&[("1.1", "Discard"), ("1.2", "Cancel")]);

        let want = Spot {
            window: 1,
            key: "1.1".to_string(),
            name: "Save".to_string(),
        };

        assert_eq!(
            pick(&here, &want),
            None,
            "the element that used to say Save was pressed under its new name"
        );
    }

    /// And the other way round: a name alone decides nothing either.
    #[test]
    fn a_second_control_with_the_same_name_is_not_it() {
        let here = found(&[("1.1", "Close"), ("1.2", "Close")]);

        let want = Spot {
            window: 1,
            key: "9.9".to_string(),
            name: "Close".to_string(),
        };

        assert_eq!(
            pick(&here, &want),
            None,
            "a control that has gone was pressed because something shares its name"
        );
    }

    /// There is no positional fallback, unlike a tab's.
    #[test]
    fn a_control_that_has_gone_presses_nothing() {
        let here = found(&[("1.1", "Copy"), ("1.2", "Paste")]);

        for want in [
            Spot {
                window: 1,
                key: "1.9".to_string(),
                name: "Copy".to_string(),
            },
            Spot {
                window: 1,
                key: "1.1".to_string(),
                name: "Delete".to_string(),
            },
        ] {
            assert_eq!(pick(&here, &want), None);
        }

        assert_eq!(
            pick(
                &[],
                &Spot {
                    window: 1,
                    key: "1.1".to_string(),
                    name: "Copy".to_string(),
                }
            ),
            None
        );
    }

    /// The kinds that are offered, and the ones deliberately not.
    #[test]
    fn only_furniture_is_offered() {
        for (control_type, kind) in [
            (50000, Kind::Button),
            (50002, Kind::Checkbox),
            (50005, Kind::Link),
            (50011, Kind::MenuItem),
            (50013, Kind::Radio),
            (50019, Kind::Tab),
            (50031, Kind::SplitButton),
        ] {
            assert_eq!(Kind::of(control_type), Some(kind));
            assert!(!kind.said().is_empty());
        }

        for (control_type, what) in [
            (50007, "a list row"),
            (50023, "a tree row"),
            (50004, "an edit box"),
            (50030, "a document"),
            (50033, "a pane"),
            (50026, "a toolbar"),
            (0, "nothing at all"),
        ] {
            assert_eq!(
                Kind::of(control_type),
                None,
                "{what} is offered as something to press"
            );
        }
    }

    /// A document is never descended through, and the number says which one.
    #[test]
    fn a_document_is_the_thing_the_walk_stops_at() {
        assert_eq!(DOCUMENT, 50030);
        assert_eq!(
            Kind::of(DOCUMENT),
            None,
            "a document is offered as something to press"
        );
    }

    #[test]
    fn sills_own_window_is_refused() {
        assert!(is_ours(4321, 4321));
        assert!(!is_ours(4321, 1234));
    }

    #[test]
    fn a_row_carries_where_its_control_was() {
        let one = Control {
            window: 42,
            name: "Advanced view".to_string(),
            kind: Kind::Checkbox,
            key: "42.7.3".to_string(),
        };

        let back = Spot::parse(&one.spotted().to_entrypoint()).expect("parses");

        assert_eq!(back, one.spotted());
        assert_eq!(back.window, 42);
        assert_eq!(back.name, "Advanced view");
    }
}
