//! Which mark the window draws for a Raycast icon name.
//!
//! ## Why the list is here rather than in the window
//!
//! Raycast publishes 469 icon names. An extension writes `Icon.Cog` and means
//! the same picture as `Icon.Gear`, `Icon.Xmark` and `Icon.XMarkCircle` are
//! one drawing with and without a ring, and `Icon.Number42` is a picture of
//! the number forty-two. That is interpretation of somebody else's vocabulary,
//! and interpretation is Rust's job here the same way parsing a query is.
//!
//! It is also the shape this project keeps losing sessions to: one list of
//! names in Rust and another in TypeScript, agreeing on the day they were
//! written and quietly disagreeing a month later. There is one list, and it is
//! this one. `scripts/verify-source.mjs` reads it and reads
//! `src/lib/components/marks.ts`, and **fails in both directions**: a name
//! added here with no drawing, a drawing for a name that is not here, and two
//! names folded onto one mark here but drawn by two different arms there. So
//! the pair cannot drift; it is not a pair that has to be remembered.
//!
//! ## Why the window does not ask for this at runtime
//!
//! It would be one call and then a table held for the life of the process, and
//! that is not the problem with it. `iconOf` runs on every row on every
//! keystroke, and Emoji Search draws six hundred and eighty rows; an await per
//! row, or a module-global primed before the first one, buys nothing over a
//! check that runs before the code ships. The route under `/preview/` that
//! draws these components has no Rust behind it at all, so a table fetched
//! over IPC would turn every mark in the harness into a lettered tile and the
//! screenshots would stop being of the thing.
//!
//! ## Every name, and what that cost
//!
//! This used to hold the names the store's extensions were measured asking
//! for, which was 106 of the 469, and everything else drew a lettered tile.
//! That was a real answer to a real question and it was the wrong one to stop
//! on: an extension is written against the whole vocabulary, and a row reading
//! `W` where its author asked for a windsock is a row that looks like Sill
//! failed rather than like Sill decided.
//!
//! The whole list is here because the marks stopped being drawn one at a time.
//! They are Phosphor Icons at regular weight, which this launcher already
//! draws its own menus with, so the artwork was a mapping job rather than an
//! artwork job. Where Phosphor has nothing honest to offer, the window draws
//! it: the numerals, the bar and progress families, and two shapes nobody
//! else has.
//!
//! ## What a name with no mark does
//!
//! A relative path into an extension's own assets arrives here as a name, and
//! there is nothing to look it up as. It draws the letter tile, which is the
//! launcher's existing answer for an application whose icon the shell will not
//! give up: a row that looks like something Sill drew rather than like
//! something that broke.

/// Every Raycast icon name, and the mark the window draws for it.
///
/// The second column is the mark's own name rather than the first column
/// repeated, because several names are one picture: `Gear` and `Cog` are the
/// same drawing, and so are `Warning` and `ExclamationMark`. Folding them here
/// rather than in a chain of `||` in the markup is what makes "these two names
/// are the same icon" a fact with a test on it.
///
/// Sorted by mark, then by name, so a reader can see the groups.
pub const MARKS: &[(&str, &str)] = &[
    ("Airplane", "airplane"),
    ("AirplaneFilled", "airplane"),
    ("AirplaneLanding", "airplane-landing"),
    ("AirplaneTakeoff", "airplane-takeoff"),
    ("Alarm", "alarm"),
    ("AlignLeft", "align-left"),
    ("AlignRight", "align-right"),
    ("Anchor", "anchor"),
    ("AppWindow", "app-window"),
    ("Window", "app-window"),
    ("Undo", "arrow-arc-left"),
    ("Redo", "arrow-arc-right"),
    ("Reply", "arrow-bend-up-left"),
    ("ArrowDownCircle", "arrow-circle-down"),
    ("ArrowDownCircleFilled", "arrow-circle-down"),
    ("ArrowLeftCircle", "arrow-circle-left"),
    ("ArrowLeftCircleFilled", "arrow-circle-left"),
    ("ArrowRightCircle", "arrow-circle-right"),
    ("ArrowRightCircleFilled", "arrow-circle-right"),
    ("ArrowUpCircle", "arrow-circle-up"),
    ("ArrowUpCircleFilled", "arrow-circle-up"),
    ("ArrowClockwise", "arrow-clockwise"),
    ("RotateClockwise", "arrow-clockwise"),
    ("ArrowCounterClockwise", "arrow-counter-clockwise"),
    ("RotateAntiClockwise", "arrow-counter-clockwise"),
    ("ArrowDown", "arrow-down"),
    ("ArrowLeft", "arrow-left"),
    ("Rewind", "arrow-left"),
    ("RewindFilled", "arrow-left"),
    ("ArrowRight", "arrow-right"),
    ("Forward", "arrow-right"),
    ("ForwardFilled", "arrow-right"),
    ("ArrowUp", "arrow-up"),
    ("ArrowNe", "arrow-up-right"),
    ("TwoArrowsClockwise", "arrows-clockwise"),
    ("ArrowsContract", "arrows-in"),
    ("ArrowsExpand", "arrows-out"),
    ("Move", "arrows-out-cardinal"),
    ("AtSymbol", "at"),
    ("BandAid", "bandaids"),
    ("Patch", "bandaids"),
    ("Weights", "barbell"),
    ("BarCode", "barcode"),
    ("Signal0", "bars-0"),
    ("Signal1", "bars-1"),
    ("StackedBars1", "bars-1"),
    ("Signal2", "bars-2"),
    ("StackedBars2", "bars-2"),
    ("Signal3", "bars-3"),
    ("StackedBars3", "bars-3"),
    ("FullSignal", "bars-4"),
    ("StackedBars4", "bars-4"),
    ("BathTub", "bathtub"),
    ("BatteryCharging", "battery-charging"),
    ("Battery", "battery-high"),
    ("BatteryDisabled", "battery-warning"),
    ("Bell", "bell"),
    ("AlarmRinging", "bell-ringing"),
    ("BellDisabled", "bell-slash"),
    ("Bike", "bicycle"),
    ("Binoculars", "binoculars"),
    ("Bird", "bird"),
    ("Bluetooth", "bluetooth"),
    ("Boat", "boat"),
    ("Book", "book"),
    ("Bookmark", "bookmark"),
    ("Livestream", "broadcast"),
    ("Bug", "bug"),
    ("Building", "building"),
    ("Calculator", "calculator"),
    ("Calendar", "calendar"),
    ("Camera", "camera"),
    ("Car", "car"),
    ("ChevronDown", "caret-down"),
    ("ChevronDownSmall", "caret-down"),
    ("ChevronLeft", "caret-left"),
    ("ChevronLeftSmall", "caret-left"),
    ("ChevronRight", "caret-right"),
    ("ChevronRightSmall", "caret-right"),
    ("ChevronUp", "caret-up"),
    ("ChevronUpSmall", "caret-up"),
    ("ChevronUpDown", "caret-up-down"),
    ("LivestreamDisabled", "cell-tower"),
    ("BarChart", "chart-bar"),
    ("LineChart", "chart-line"),
    ("PieChart", "chart-pie"),
    ("Bubble", "chat"),
    ("Message", "chat-teardrop"),
    ("SpeechBubble", "chat-teardrop"),
    ("SpeechBubbleActive", "chat-teardrop-dots"),
    ("SpeechBubbleImportant", "chat-teardrop-text"),
    ("Check", "check"),
    ("Checkmark", "check"),
    ("CheckCircle", "check-circle"),
    ("ChessPiece", "chess-piece"),
    ("Circle", "circle"),
    ("CircleFilled", "circle"),
    ("Contrast", "circle-half"),
    ("Clipboard", "clipboard"),
    ("CopyClipboard", "clipboard-text"),
    ("Clock", "clock"),
    ("Cloud", "cloud"),
    ("CloudLightning", "cloud-lightning"),
    ("CloudRain", "cloud-rain"),
    ("CloudSnow", "cloud-snow"),
    ("CloudSun", "cloud-sun"),
    ("Code", "code"),
    ("CodeBlock", "code-block"),
    ("Mug", "coffee"),
    ("MugSteam", "coffee"),
    ("Coin", "coin"),
    ("Coins", "coins"),
    ("CommandSymbol", "command"),
    ("Compass", "compass"),
    ("Duplicate", "copy-simple"),
    ("Minimize", "corners-in"),
    ("Maximize", "corners-out"),
    ("ComputerChip", "cpu"),
    ("CreditCard", "credit-card"),
    ("CricketBall", "cricket"),
    ("Crop", "crop"),
    ("BullsEyeMissed", "crosshair"),
    ("Center", "crosshair-simple"),
    ("Crown", "crown"),
    ("Crypto", "currency-btc"),
    ("TextCursor", "cursor-text"),
    ("TextInput", "cursor-text"),
    ("Desktop", "desktop"),
    ("Mobile", "device-mobile"),
    ("Devices", "devices"),
    ("Cd", "disc"),
    ("Dna", "dna"),
    ("Dot", "dot"),
    ("Ellipsis", "dots-three"),
    ("SquareEllipsis", "dots-three"),
    ("CircleEllipsis", "dots-three-circle"),
    ("Download", "download"),
    ("Droplets", "drop"),
    ("Raindrop", "drop"),
    ("Humidity", "drop-half"),
    ("Eject", "eject"),
    ("Envelope", "envelope"),
    ("LevelMeter", "equalizer"),
    ("Eraser", "eraser"),
    ("Eye", "eye"),
    ("EyeDisabled", "eye-slash"),
    ("EyeSlash", "eye-slash"),
    ("EyeDropper", "eyedropper"),
    ("Glasses", "eyeglasses"),
    ("Mask", "face-mask"),
    ("BlankDocument", "file"),
    ("Document", "file"),
    ("NewDocument", "file-plus"),
    ("TextDocument", "file-text"),
    ("DeleteDocument", "file-x"),
    ("FilmStrip", "film-strip"),
    ("Fingerprint", "fingerprint"),
    ("MedicalSupport", "first-aid"),
    ("Flag", "flag"),
    ("Torch", "flashlight"),
    ("SaveDocument", "floppy-disk"),
    ("Folder", "folder"),
    ("Finder", "folder-open"),
    ("NewFolder", "folder-plus"),
    ("AmericanFootball", "football"),
    ("Footprints", "footprints"),
    ("Filter", "funnel"),
    ("GameController", "game-controller"),
    ("Gauge", "gauge"),
    ("Cog", "gear"),
    ("Gear", "gear"),
    ("Female", "gender-female"),
    ("Male", "gender-male"),
    ("Gift", "gift"),
    ("Globe", "globe"),
    ("AppWindowGrid3x3", "grid-nine"),
    ("Hammer", "hammer"),
    ("HardDrive", "hard-drive"),
    ("Hashtag", "hash"),
    ("Airpods", "headphones"),
    ("Headphones", "headphones"),
    ("Heart", "heart"),
    ("HeartDisabled", "heart-break"),
    ("Heartbeat", "heartbeat"),
    ("Highlight", "highlighter"),
    ("Hourglass", "hourglass"),
    ("House", "house"),
    ("Image", "image"),
    ("Info", "info"),
    ("Key", "key"),
    ("Keyboard", "keyboard"),
    ("Leaf", "leaf"),
    ("Lowercase", "letters-lower"),
    ("Uppercase", "letters-upper"),
    ("Buoy", "lifebuoy"),
    ("LightBulb", "lightbulb"),
    ("LightBulbOff", "lightbulb-filament"),
    ("Bolt", "lightning"),
    ("RaycastLogoNeg", "lightning"),
    ("RaycastLogoPos", "lightning"),
    ("BoltDisabled", "lightning-slash"),
    ("Link", "link"),
    ("List", "list"),
    ("BulletPoints", "list-bullets"),
    ("AppWindowList", "list-dashes"),
    ("Lock", "lock"),
    ("LockUnlocked", "lock-open"),
    ("LockDisabled", "lock-simple-open"),
    ("Wand", "magic-wand"),
    ("MagnifyingGlass", "magnifying-glass"),
    ("Geopin", "map-pin"),
    ("Map", "map-trifold"),
    ("Megaphone", "megaphone"),
    ("MemoryChip", "memory"),
    ("MemoryStick", "memory"),
    ("Microphone", "microphone"),
    ("MicrophoneDisabled", "microphone-slash"),
    ("Minus", "minus"),
    ("MinusCircle", "minus-circle"),
    ("MinusCircleFilled", "minus-circle"),
    ("BankNote", "money"),
    ("Monitor", "monitor"),
    ("Moon", "moon"),
    ("MoonDown", "moon"),
    ("Moonrise", "moon-stars"),
    ("MoonUp", "moon-stars"),
    ("Mountain", "mountains"),
    ("Mouse", "mouse"),
    ("Music", "music-notes"),
    ("Network", "network"),
    ("Number00", "numeral-00"),
    ("Number01", "numeral-01"),
    ("Number02", "numeral-02"),
    ("Number03", "numeral-03"),
    ("Number04", "numeral-04"),
    ("Number05", "numeral-05"),
    ("Number06", "numeral-06"),
    ("Number07", "numeral-07"),
    ("Number08", "numeral-08"),
    ("Number09", "numeral-09"),
    ("Number10", "numeral-10"),
    ("Number11", "numeral-11"),
    ("Number12", "numeral-12"),
    ("Number13", "numeral-13"),
    ("Number14", "numeral-14"),
    ("Number15", "numeral-15"),
    ("Number16", "numeral-16"),
    ("Number17", "numeral-17"),
    ("Number18", "numeral-18"),
    ("Number19", "numeral-19"),
    ("Number20", "numeral-20"),
    ("Number21", "numeral-21"),
    ("Number22", "numeral-22"),
    ("Number23", "numeral-23"),
    ("Number24", "numeral-24"),
    ("Number25", "numeral-25"),
    ("Number26", "numeral-26"),
    ("Number27", "numeral-27"),
    ("Number28", "numeral-28"),
    ("Number29", "numeral-29"),
    ("Number30", "numeral-30"),
    ("Number31", "numeral-31"),
    ("Number32", "numeral-32"),
    ("Number33", "numeral-33"),
    ("Number34", "numeral-34"),
    ("Number35", "numeral-35"),
    ("Number36", "numeral-36"),
    ("Number37", "numeral-37"),
    ("Number38", "numeral-38"),
    ("Number39", "numeral-39"),
    ("Number40", "numeral-40"),
    ("Number41", "numeral-41"),
    ("Number42", "numeral-42"),
    ("Number43", "numeral-43"),
    ("Number44", "numeral-44"),
    ("Number45", "numeral-45"),
    ("Number46", "numeral-46"),
    ("Number47", "numeral-47"),
    ("Number48", "numeral-48"),
    ("Number49", "numeral-49"),
    ("Number50", "numeral-50"),
    ("Number51", "numeral-51"),
    ("Number52", "numeral-52"),
    ("Number53", "numeral-53"),
    ("Number54", "numeral-54"),
    ("Number55", "numeral-55"),
    ("Number56", "numeral-56"),
    ("Number57", "numeral-57"),
    ("Number58", "numeral-58"),
    ("Number59", "numeral-59"),
    ("Number60", "numeral-60"),
    ("Number61", "numeral-61"),
    ("Number62", "numeral-62"),
    ("Number63", "numeral-63"),
    ("Number64", "numeral-64"),
    ("Number65", "numeral-65"),
    ("Number66", "numeral-66"),
    ("Number67", "numeral-67"),
    ("Number68", "numeral-68"),
    ("Number69", "numeral-69"),
    ("Number70", "numeral-70"),
    ("Number71", "numeral-71"),
    ("Number72", "numeral-72"),
    ("Number73", "numeral-73"),
    ("Number74", "numeral-74"),
    ("Number75", "numeral-75"),
    ("Number76", "numeral-76"),
    ("Number77", "numeral-77"),
    ("Number78", "numeral-78"),
    ("Number79", "numeral-79"),
    ("Number80", "numeral-80"),
    ("Number81", "numeral-81"),
    ("Number82", "numeral-82"),
    ("Number83", "numeral-83"),
    ("Number84", "numeral-84"),
    ("Number85", "numeral-85"),
    ("Number86", "numeral-86"),
    ("Number87", "numeral-87"),
    ("Number88", "numeral-88"),
    ("Number89", "numeral-89"),
    ("Number90", "numeral-90"),
    ("Number91", "numeral-91"),
    ("Number92", "numeral-92"),
    ("Number93", "numeral-93"),
    ("Number94", "numeral-94"),
    ("Number95", "numeral-95"),
    ("Number96", "numeral-96"),
    ("Number97", "numeral-97"),
    ("Number98", "numeral-98"),
    ("Number99", "numeral-99"),
    ("Box", "package"),
    ("Brush", "paint-brush"),
    ("Paperclip", "paperclip"),
    ("Paragraph", "paragraph"),
    ("ShortParagraph", "paragraph"),
    ("Pause", "pause"),
    ("PauseFilled", "pause"),
    ("FountainTip", "pen-nib"),
    ("Pencil", "pencil"),
    ("Phone", "phone"),
    ("PhoneRinging", "phone-call"),
    ("Pill", "pill"),
    ("Play", "play"),
    ("PlayFilled", "play"),
    ("Plug", "plug"),
    ("Plus", "plus"),
    ("PlusCircle", "plus-circle"),
    ("PlusCircleFilled", "plus-circle"),
    ("PlusMinusDivideMultiply", "plus-minus"),
    ("PlusSquare", "plus-square"),
    ("PlusTopRightSquare", "plus-square"),
    ("Power", "power"),
    ("Print", "printer"),
    ("CircleProgress", "progress-0"),
    ("CircleProgress100", "progress-100"),
    ("CircleProgress25", "progress-25"),
    ("CircleProgress50", "progress-50"),
    ("CircleProgress75", "progress-75"),
    ("CircleDisabled", "prohibit"),
    ("Pin", "push-pin"),
    ("Tack", "push-pin"),
    ("PinDisabled", "push-pin-slash"),
    ("TackDisabled", "push-pin-slash"),
    ("QuestionMarkCircle", "question"),
    ("QuestionMark", "question-mark"),
    ("QuotationMarks", "quotes"),
    ("QuoteBlock", "quotes"),
    ("Racket", "racquet"),
    ("Leaderboard", "ranking"),
    ("Receipt", "receipt"),
    ("Repeat", "repeat"),
    ("Rocket", "rocket"),
    ("Rss", "rss"),
    ("Ruler", "ruler"),
    ("Snippets", "scissors"),
    ("Rosette", "seal"),
    ("CheckRosette", "seal-check"),
    ("TextSelection", "selection"),
    ("EditShape", "shapes"),
    ("Shield", "shield"),
    ("Cart", "shopping-cart"),
    ("Exclamationmark", "shout-1"),
    ("Exclamationmark2", "shout-2"),
    ("Exclamationmark3", "shout-3"),
    ("Shuffle", "shuffle"),
    ("AppWindowSidebarRight", "sidebar"),
    ("Sidebar", "sidebar"),
    ("AppWindowSidebarLeft", "sidebar-simple"),
    ("Logout", "sign-out"),
    ("Emoji", "smiley"),
    ("EmojiSad", "smiley-sad"),
    ("Snowflake", "snowflake"),
    ("Goal", "soccer-ball"),
    ("SoccerBall", "soccer-ball"),
    ("Stars", "sparkle"),
    ("Speaker", "speaker-high"),
    ("SpeakerHigh", "speaker-high"),
    ("SpeakerOn", "speaker-high"),
    ("SpeakerLow", "speaker-low"),
    ("SpeakerOff", "speaker-none"),
    ("SpeakerArrowUp", "speaker-simple-high"),
    ("SpeakerUp", "speaker-simple-high"),
    ("SpeakerArrowDown", "speaker-simple-low"),
    ("SpeakerDown", "speaker-simple-low"),
    ("SpeakerSlash", "speaker-slash"),
    ("AppWindowGrid2x2", "squares-four"),
    ("Layers", "stack"),
    ("Star", "star"),
    ("StarCircle", "star"),
    ("StarDisabled", "star-disabled"),
    ("Stop", "stop"),
    ("StopFilled", "stop"),
    ("Store", "storefront"),
    ("Sun", "sun"),
    ("Sunrise", "sun-horizon"),
    ("Swatch", "swatches"),
    ("Syringe", "syringe"),
    ("Tag", "tag"),
    ("BullsEye", "target"),
    ("TennisBall", "tennis-ball"),
    ("Terminal", "terminal"),
    ("AlignCentre", "text-align-center"),
    ("Bold", "text-b"),
    ("Italics", "text-italic"),
    ("StrikeThrough", "text-strikethrough"),
    ("Text", "text-t"),
    ("ClearFormatting", "text-t-slash"),
    ("Underline", "text-underline"),
    ("Temperature", "thermometer"),
    ("ThumbsDown", "thumbs-down"),
    ("ThumbsDownFilled", "thumbs-down"),
    ("ThumbsUp", "thumbs-up"),
    ("ThumbsUpFilled", "thumbs-up"),
    ("Ticket", "ticket"),
    ("Stopwatch", "timer"),
    ("Switch", "toggle-right"),
    ("Train", "train"),
    ("Trash", "trash"),
    ("Tray", "tray"),
    ("Tree", "tree"),
    ("Trophy", "trophy"),
    ("Lorry", "truck"),
    ("Umbrella", "umbrella"),
    ("Upload", "upload"),
    ("Person", "user"),
    ("PersonCircle", "user-circle"),
    ("PersonLines", "user-list"),
    ("RemovePerson", "user-minus"),
    ("AddPerson", "user-plus"),
    ("TwoPeople", "users"),
    ("Video", "video"),
    ("Germ", "virus"),
    ("Wallet", "wallet"),
    ("ExclamationMark", "warning"),
    ("Warning", "warning"),
    ("Important", "warning-octagon"),
    ("WristWatch", "watch"),
    ("Waveform", "waveform"),
    ("Wifi", "wifi-high"),
    ("WifiDisabled", "wifi-slash"),
    ("Wind", "wind"),
    ("Windsock", "wind"),
    ("WrenchScrewdriver", "wrench"),
    ("Multiply", "x"),
    ("Xmark", "x"),
    ("XmarkCircle", "x-circle"),
    ("XMarkCircle", "x-circle"),
    ("XMarkCircleFilled", "x-circle"),
    ("XMarkTopRightSquare", "x-square"),
];

/// The mark for a name, or nothing when Sill has no drawing for it.
///
/// Nothing rather than a nearest guess. An extension asking for a name Sill
/// does not draw and getting an unrelated picture has been told something
/// untrue about its own row, while one getting the lettered tile has simply
/// not been given a picture, which is the same thing the root list says about
/// an application whose icon it could not read.
pub fn mark_for(name: &str) -> Option<&'static str> {
    MARKS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, mark)| *mark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// A name written twice is a name whose second row nothing reaches.
    ///
    /// `mark_for` takes the first match, so a duplicate with a different mark
    /// is a drawing that can never be chosen and a reader who cannot tell
    /// which of the two is live.
    #[test]
    fn no_name_appears_twice() {
        let mut seen = BTreeSet::new();

        for (name, _) in MARKS {
            assert!(seen.insert(*name), "{name} has two rows in MARKS");
        }
    }

    /// Names are the vocabulary somebody else publishes, so they look like it.
    ///
    /// Raycast writes them in upper camel case, and the string an extension
    /// puts in the prop is the property name itself: `Icon.Star` arrives as
    /// `"Star"`. A row written in any other shape matches nothing at runtime
    /// and would sit here looking correct.
    #[test]
    fn every_name_is_written_the_way_raycast_writes_it() {
        for (name, _) in MARKS {
            assert!(
                name.chars().next().is_some_and(char::is_uppercase)
                    && name.chars().all(char::is_alphanumeric),
                "{name} is not a Raycast icon name",
            );
        }
    }

    /// Mark ids are what the window keys its drawings on, so they are one
    /// shape too: lower case words joined by hyphens, and a trailing number
    /// where the mark is one of a graded family.
    #[test]
    fn every_mark_is_written_the_way_the_window_keys_them() {
        for (_, mark) in MARKS {
            assert!(
                !mark.is_empty()
                    && mark.starts_with(|c: char| c.is_ascii_lowercase())
                    && mark
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{mark} is not a mark id",
            );
        }
    }

    /// The whole vocabulary, not the part somebody measured being asked for.
    ///
    /// This table held 106 of Raycast's 469 names for as long as the marks
    /// were drawn one at a time, and the gap was invisible: a name with no row
    /// draws a lettered tile, which is a shape somebody has to already know is
    /// wrong. A count is what turns "we added some more" back into a fact.
    #[test]
    fn every_name_raycast_publishes_has_a_mark() {
        assert_eq!(MARKS.len(), 469);
    }

    /// A family generated from a rule is complete or it is a hole.
    ///
    /// The hundred numerals are one drawing with the digits changed, so they
    /// arrive together or not at all. A generation that stopped at `Number89`
    /// would leave ten names lettered and nothing else would say so.
    #[test]
    fn the_graded_families_have_every_step() {
        for n in 0..100 {
            let name = format!("Number{n:02}");
            assert_eq!(
                mark_for(&name),
                Some(format!("numeral-{n:02}").as_str()),
                "{name} has no mark",
            );
        }

        for step in ["0", "25", "50", "75", "100"] {
            let name = if step == "0" {
                "CircleProgress".to_string()
            } else {
                format!("CircleProgress{step}")
            };
            assert_eq!(mark_for(&name), Some(format!("progress-{step}").as_str()));
        }

        for n in 1..=4 {
            assert_eq!(
                mark_for(&format!("StackedBars{n}")),
                Some(format!("bars-{n}").as_str()),
            );
        }

        // The signal names are the same reading and fold onto the same bars.
        for n in 0..=3 {
            assert_eq!(
                mark_for(&format!("Signal{n}")),
                Some(format!("bars-{n}").as_str()),
            );
        }
        assert_eq!(mark_for("FullSignal"), Some("bars-4"));
    }

    /// The folding is the point of the second column.
    ///
    /// If every name had a mark of its own this table would be a list with a
    /// column repeated, and the aliases that make it worth having would have
    /// been lost without anything saying so.
    #[test]
    fn names_that_are_one_picture_share_a_mark() {
        let mut by_mark: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (name, mark) in MARKS {
            by_mark.entry(mark).or_default().push(name);
        }

        assert_eq!(by_mark.get("gear"), Some(&vec!["Cog", "Gear"]));
        assert_eq!(by_mark.get("star"), Some(&vec!["Star", "StarCircle"]));
        assert_eq!(
            by_mark.get("warning"),
            Some(&vec!["ExclamationMark", "Warning"]),
        );
    }

    #[test]
    fn a_name_sill_draws_resolves_and_one_it_does_not_answers_nothing() {
        assert_eq!(mark_for("Cog"), Some("gear"));
        assert_eq!(mark_for("Gear"), Some("gear"));
        // Every name Raycast publishes has one now, including this, which
        // spent a while as a lettered tile.
        assert_eq!(mark_for("Livestream"), Some("broadcast"));
        // A name Raycast does not publish still answers nothing rather than
        // the nearest thing spelled like it.
        assert_eq!(mark_for("Unicorn"), None);
        // A relative asset path reaches the window as a name too.
        assert_eq!(mark_for("assets/logo.png"), None);
        assert_eq!(mark_for(""), None);
    }
}
