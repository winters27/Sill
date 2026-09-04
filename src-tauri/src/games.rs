//! Installed games, which no other source in the launcher can see.
//!
//! ## Why a game needs its own source
//!
//! Every other application in the index is found because somebody put a file
//! somewhere Windows lists: a shortcut in the Start Menu, a registration under
//! `App Paths`, an entry in an uninstall hive. A game installed through Steam
//! is in none of them. Steam writes a desktop shortcut only if the person
//! ticked the box during install, and it writes nothing at all to the Start
//! Menu for most titles. On the machine this was written on, seven games are
//! installed and **not one of them is reachable by name** from any of the five
//! sources that already existed, which `suite::real_games` prints rather than
//! asserts so the number stays honest as the library changes.
//!
//! ## Why the target is not a `steam://` address
//!
//! `steam://rungameid/730` is the documented way to start a game and it would
//! be the obvious target to store. It is deliberately not used, because the
//! only thing that opens an address here is `reach::url`, and adding `steam`
//! to that allow-list would let a scheme with a long history of argument
//! handling bugs be reached from an imported quicklink, an extension row and
//! anything the model names. Sill has no need to open a `steam://` address
//! that somebody else wrote; it needs to start a game it found itself.
//!
//! So a game's target is a string this module owns, `sill-game:steam/730`,
//! which nothing but [`command`] understands, and which [`command`] turns into
//! an executable and an argument list after checking that the identifier is
//! the shape it is supposed to be. The parallel is `shell:AppsFolder\`, which
//! is a target the launcher recognises rather than a path.
//!
//! ## What is proven and what is not
//!
//! The Steam half was built against the real library on this machine and the
//! probes in `suite::real_games` read it. **The Epic half has never run
//! against an Epic install**: the parsing follows the documented manifest
//! shape and is covered by fixtures, and `Epic Games` on this machine holds
//! only the Online Services runtime that games bring with them. Anybody
//! touching it should treat the fixtures as a description of what was
//! believed, not as evidence.

use std::path::{Path, PathBuf};

use crate::apps::AppRecord;

/// Prefix marking an entry that a game library launches.
///
/// Not a URL scheme, and it must not become one. See the module note.
pub const GAME: &str = "sill-game:";

// ------------------------------------------------------------------- Valve

/// One `"key" "value"` pair from a Valve text file, and how deep it sat.
///
/// Depth is what tells `libraryfolders.vdf`'s library paths apart from the
/// paths that appear inside a nested block, so it is carried rather than
/// flattened away.
#[derive(Debug, PartialEq)]
struct Pair {
    depth: usize,
    key: String,
    value: String,
}

/// Reads Valve's key-values text, which both of Steam's files are written in.
///
/// Written out rather than taken from a crate because the subset in use is
/// small and completely described by the two files being read: quoted strings,
/// nested blocks, and `//` comments. A dependency for that is a dependency to
/// keep up to date for no benefit.
///
/// Unterminated input yields whatever was complete before it ran out, rather
/// than an error. A half-written `appmanifest` is something Steam produces
/// during an install, and the right response is to see the games that did
/// parse, not to refuse the whole library.
fn pairs(text: &str) -> Vec<Pair> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut pending: Option<String> = None;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' => {
                // The string just read was this block's name, not a value.
                pending = None;
                depth += 1;
            }
            '}' => {
                pending = None;
                depth = depth.saturating_sub(1);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut read = String::new();
                let mut closed = false;

                while let Some(c) = chars.next() {
                    match c {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' => match chars.next() {
                            Some('n') => read.push('\n'),
                            Some('t') => read.push('\t'),
                            Some(other) => read.push(other),
                            None => break,
                        },
                        other => read.push(other),
                    }
                }

                if !closed {
                    break;
                }

                match pending.take() {
                    None => pending = Some(read),
                    Some(key) => out.push(Pair {
                        depth,
                        key,
                        value: read,
                    }),
                }
            }
            _ => {}
        }
    }

    out
}

/// Every folder Steam keeps games in, from `libraryfolders.vdf`.
///
/// Games do not all live under the Steam install. A second drive is the normal
/// arrangement once a library outgrows an SSD, and reading only
/// `steamapps` under the install directory finds none of them.
///
/// Depth two exactly: the file is `libraryfolders` containing numbered blocks
/// each containing a `path`. Anything deeper is inside the `apps` block, which
/// has no paths in it today and is not something to start trusting.
pub fn library_paths(vdf: &str) -> Vec<PathBuf> {
    pairs(vdf)
        .into_iter()
        .filter(|p| p.depth == 2 && p.key.eq_ignore_ascii_case("path"))
        .filter(|p| !p.value.trim().is_empty())
        .map(|p| PathBuf::from(p.value))
        .collect()
}

/// The app id and name in one `appmanifest_*.acf`.
///
/// Both are required. A manifest missing either is one Steam is part way
/// through writing, and a row with no name is a blank line in the list.
pub fn installed(acf: &str) -> Option<(String, String)> {
    let read = pairs(acf);
    let find = |key: &str| {
        read.iter()
            .find(|p| p.depth == 1 && p.key.eq_ignore_ascii_case(key))
            .map(|p| p.value.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    let id = find("appid")?;
    let name = find("name")?;

    // The id goes into a launch argument, so it is checked here rather than
    // only where it is used. A manifest is a file on disk that anything with
    // write access can change.
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    Some((id, name))
}

/// Entries in a Steam library that are not games somebody launches.
///
/// Found by reading the real library rather than guessed: every Steam install
/// carries `Steamworks Common Redistributables`, which is the DirectX and
/// Visual C++ bundle games depend on. It has an app id and a manifest exactly
/// like a game, it appears in nobody's library list, and starting it does
/// nothing anyone wants.
///
/// Deliberately one exact name. A rule such as "anything with redistributable
/// in the title" would take a game called that with it, and that is the same
/// mistake the application filter already learned once with "uninstall".
///
/// Proton and the Linux runtimes are the other entries of this kind and are
/// **not** listed, for two reasons: they never appear on Windows, and their
/// real titles carry a version, `Steam Linux Runtime 3.0 (sniper)`, so an
/// exact-match entry for them would be a line that reads as protection while
/// matching nothing.
pub fn is_plumbing(name: &str) -> bool {
    const NOT_GAMES: [&str; 1] = ["steamworks common redistributables"];

    let lower = name.trim().to_ascii_lowercase();
    NOT_GAMES.iter().any(|one| lower == *one)
}

// -------------------------------------------------------------------- Epic

/// One Epic manifest, as far as launching is concerned.
#[derive(serde::Deserialize)]
struct EpicItem {
    #[serde(rename = "DisplayName", default)]
    display_name: String,
    /// Epic's own identifier for the title, which is what its launcher takes.
    #[serde(rename = "AppName", default)]
    app_name: String,
    #[serde(rename = "InstallLocation", default)]
    install_location: String,
    #[serde(rename = "LaunchExecutable", default)]
    launch_executable: String,
    /// `games`, `applications`, `plugins` and so on. The launcher itself and
    /// every engine plugin has a manifest here too, and none of them belong in
    /// a list of games.
    #[serde(rename = "AppCategories", default)]
    categories: Vec<String>,
}

/// The name, identifier and icon of one Epic game, from a `.item` manifest.
///
/// `None` for anything that is not a game, which is most of what is in that
/// folder on a machine with the Unreal Engine installed.
///
/// **Never run against a real Epic install.** See the module note.
pub fn epic_item(json: &str) -> Option<(String, String, Option<String>)> {
    let item: EpicItem = serde_json::from_str(json).ok()?;

    if !item
        .categories
        .iter()
        .any(|one| one.eq_ignore_ascii_case("games"))
    {
        return None;
    }

    let name = item.display_name.trim();
    let id = item.app_name.trim();

    if name.is_empty() || id.is_empty() || !is_epic_id(id) {
        return None;
    }

    // The executable is not what gets launched, it is what has an icon in it.
    // Launching it directly skips Epic's own start up, which several titles
    // require to sign in.
    let icon =
        (!item.install_location.is_empty() && !item.launch_executable.is_empty()).then(|| {
            Path::new(&item.install_location)
                .join(item.launch_executable.replace('/', "\\"))
                .to_string_lossy()
                .to_string()
        });

    Some((id.to_string(), name.to_string(), icon))
}

/// Whether a string is shaped like an Epic app identifier.
///
/// Epic's identifiers are opaque words such as `Fortnite` or `CrabEA`, and the
/// one thing that matters here is that the string cannot carry a second
/// argument or a quote into the command line. Letters, digits, underscore and
/// hyphen, nothing else.
fn is_epic_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ----------------------------------------------------------------- launching

/// The executable and arguments that start the game a target names.
///
/// The whole decision, so it can be tested without a Steam install and so
/// there is one place that says what a `sill-game:` target is allowed to be.
///
/// **This validates rather than trusts.** A target normally comes from a scan
/// this module did, but `sill.launch` runs on whatever an extension row or the
/// model put in front of it, and a target such as
/// `sill-game:steam/730 --exec-something` would otherwise become an argument
/// list. An identifier that is not the shape it should be is refused, not
/// escaped: refusing is a rule that can be read, escaping is a rule that has
/// to be right.
pub fn command(
    target: &str,
    steam: Option<&Path>,
    epic: Option<&Path>,
) -> Result<(PathBuf, Vec<String>), String> {
    let rest = target
        .strip_prefix(GAME)
        .ok_or_else(|| format!("{target} is not a game."))?;

    let (library, id) = rest
        .split_once('/')
        .ok_or_else(|| format!("{rest} does not name a game library."))?;

    match library {
        "steam" => {
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("{id} is not a Steam app id."));
            }

            let steam = steam.ok_or("Steam is not installed.")?;

            // `-applaunch` rather than the `steam://` handler, so nothing has
            // to trust a protocol handler with an argument. Steam starts
            // itself first if it is not already running, which is the same
            // thing clicking a game in the library does.
            Ok((
                steam.join("steam.exe"),
                vec!["-applaunch".to_string(), id.to_string()],
            ))
        }
        "epic" => {
            if !is_epic_id(id) {
                return Err(format!("{id} is not an Epic app name."));
            }

            let epic = epic.ok_or("The Epic Games launcher is not installed.")?;

            // Epic takes its own address as a command line argument, which is
            // how it is started from a desktop shortcut. Handing it to the
            // launcher rather than to the shell means the `com.epicgames`
            // handler is never involved.
            Ok((
                epic.to_path_buf(),
                vec![format!(
                    "-com.epicgames.launcher://apps/{id}?action=launch&silent=true"
                )],
            ))
        }
        other => Err(format!("{other} is not a game library Sill knows.")),
    }
}

// ------------------------------------------------------------------ scanning

/// Where Steam is installed, if it is.
#[cfg(windows)]
pub fn steam_root() -> Option<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    // The machine-wide value first: it is written with the separators Windows
    // uses, where the per-user copy is lower case with forward slashes. Both
    // work, but only one of them reads sensibly if it ever reaches a message.
    let found = crate::apps::read_string(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\WOW6432Node\Valve\Steam",
        "InstallPath",
    )
    .or_else(|| {
        crate::apps::read_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath")
    })
    .or_else(|| {
        crate::apps::read_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
    })?;

    let root = PathBuf::from(found.replace('/', "\\"));
    root.join("steam.exe").is_file().then_some(root)
}

#[cfg(not(windows))]
pub fn steam_root() -> Option<PathBuf> {
    None
}

/// The Epic launcher executable, if it is installed.
///
/// The registry value names the launcher's data folder rather than the
/// executable, so the well known path under Program Files is tried too.
#[cfg(windows)]
pub fn epic_launcher() -> Option<PathBuf> {
    let candidates = std::env::var("ProgramFiles(x86)")
        .into_iter()
        .chain(std::env::var("ProgramFiles"))
        .map(|base| {
            PathBuf::from(base)
                .join("Epic Games")
                .join("Launcher")
                .join("Portal")
                .join("Binaries")
                .join("Win32")
                .join("EpicGamesLauncher.exe")
        });

    candidates.into_iter().find(|one| one.is_file())
}

#[cfg(not(windows))]
pub fn epic_launcher() -> Option<PathBuf> {
    None
}

/// Where Epic keeps one manifest per installed title.
fn epic_manifests() -> Option<PathBuf> {
    let root = std::env::var("ProgramData").ok()?;
    Some(
        PathBuf::from(root)
            .join("Epic")
            .join("EpicGamesLauncher")
            .join("Data")
            .join("Manifests"),
    )
}

/// Steam's cached artwork for a game, if it has any.
///
/// Only four of the eight games on this machine have a cached logo, so this
/// answers `None` often and the lettered tile is the normal outcome rather
/// than a sign something went wrong.
fn steam_logo(root: &Path, id: &str) -> Option<String> {
    let cache = root.join("appcache").join("librarycache").join(id);

    /*
     * Three shapes, because Steam has changed this twice and a machine can
     * hold all three at once.
     *
     * `logo.png` is the transparent wordmark and the only artwork shaped like
     * an icon; the rest of that folder is shelf art, which is a portrait
     * poster and reads as a smear at the size of a row.
     *
     * **Newer entries put it one level down, under a content hash.** Four of
     * the seven games on the machine this was written on are stored that way,
     * and looking only for `logo.png` beside the folder found nothing for any
     * of them: the launcher drew a lettered tile for Apex Legends,
     * Enshrouded, Split Fiction and Battlefield while three older games kept
     * their picture. That is what this reads as a bug.
     *
     * The hash is not something to look up. `assetcache.vdf` maps it, but the
     * directory is right here and holds exactly one thing, so reading the
     * folder answers the same question without parsing another file that
     * Steam is free to change again.
     */
    let named = cache.join("logo.png");
    if named.is_file() {
        return Some(named.to_string_lossy().to_string());
    }

    if let Ok(entries) = std::fs::read_dir(&cache) {
        for entry in entries.flatten() {
            let inside = entry.path().join("logo.png");
            if inside.is_file() {
                return Some(inside.to_string_lossy().to_string());
            }
        }
    }

    // Steam wrote a flat `<appid>_logo.png` before either of those, and an
    // install that has not refreshed its cache since still has that shape.
    let old = root
        .join("appcache")
        .join("librarycache")
        .join(format!("{id}_logo.png"));

    old.is_file().then(|| old.to_string_lossy().to_string())
}

/// Every installed game, as rows the index can hold.
pub fn scan() -> Vec<AppRecord> {
    let mut found = Vec::new();

    if let Some(root) = steam_root() {
        found.extend(steam_games(&root));
    }

    if let Some(folder) = epic_manifests() {
        found.extend(epic_games(&folder));
    }

    found
}

fn steam_games(root: &Path) -> Vec<AppRecord> {
    let mut libraries = vec![root.to_path_buf()];

    if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for path in library_paths(&text) {
            if !libraries.contains(&path) {
                libraries.push(path);
            }
        }
    }

    let mut found = Vec::new();

    for library in libraries {
        let Ok(entries) = std::fs::read_dir(library.join("steamapps")) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("acf") {
                continue;
            }

            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };

            let Some((id, name)) = installed(&text) else {
                continue;
            };

            if is_plumbing(&name) {
                continue;
            }

            found.push(AppRecord {
                icon_source: steam_logo(root, &id),
                path: format!("{GAME}steam/{id}"),
                name,
            });
        }
    }

    found
}

fn epic_games(folder: &Path) -> Vec<AppRecord> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut found = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };

        if let Some((id, name, icon)) = epic_item(&text) {
            found.push(AppRecord {
                icon_source: icon,
                path: format!("{GAME}epic/{id}"),
                name,
            });
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the real file on this machine, including the nested
    /// `apps` block that a depth-blind reader would take paths out of.
    const LIBRARIES: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"730"		"71644882396"
			// Not in the file today. Here because the depth rule is the
			// thing being tested, and a rule with no case that exercises
			// it is a rule that can be deleted without anything noticing.
			"path"		"D:\NotALibrary"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"label"		""
	}
}
"#;

    /// A real `appmanifest_730.acf`, with one addition.
    ///
    /// `UserConfig` really is there and really is a nested block; the `name`
    /// inside it is not, and is here because the depth rule is what stops a
    /// reader taking a title out of a block that is not about the game.
    const COUNTER_STRIKE: &str = r#"
"AppState"
{
	"appid"		"730"
	"universe"		"1"
	"name"		"Counter-Strike 2"
	"installdir"		"Counter-Strike Global Offensive"
	"UserConfig"
	{
		"name"		"something else entirely"
	}
}
"#;

    /// A second drive, and nothing out of a block that is not a library.
    #[test]
    fn a_library_on_another_drive_is_found() {
        let paths = library_paths(LIBRARIES);
        assert_eq!(
            paths,
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary"),
            ]
        );
    }

    /// A doubled backslash in the file is one backslash in the path.
    ///
    /// Every path in this file is written that way, so getting it wrong makes
    /// every library unreadable rather than some of them.
    #[test]
    fn the_escaping_is_undone() {
        let paths = library_paths(LIBRARIES);
        assert!(paths[0].is_absolute());
        assert!(!paths[0].to_string_lossy().contains("\\\\"));
    }

    #[test]
    fn a_manifest_gives_up_its_id_and_name() {
        assert_eq!(
            installed(COUNTER_STRIKE),
            Some(("730".to_string(), "Counter-Strike 2".to_string()))
        );
    }

    /// `UserConfig` has a `name` of its own, and it is not the game's.
    ///
    /// Reading the last `name` in the file, or any `name` at any depth, gives
    /// the row a title nobody would search for.
    #[test]
    fn a_name_inside_a_nested_block_is_not_the_games_name() {
        let (_, name) = installed(COUNTER_STRIKE).expect("a game");
        assert_eq!(name, "Counter-Strike 2");
    }

    /// A manifest Steam is part way through writing is skipped, not fatal.
    #[test]
    fn a_half_written_manifest_yields_nothing_rather_than_panicking() {
        assert_eq!(installed("\"AppState\"\n{\n\t\"appid\"\t\t\"73"), None);
    }

    /// An app id is checked where it is read as well as where it is used.
    #[test]
    fn an_app_id_that_is_not_a_number_is_refused() {
        let tampered = COUNTER_STRIKE.replace("\"730\"", "\"730 -applaunch 4\"");
        assert_eq!(installed(&tampered), None);
    }

    /// The filter matches the whole name, not part of it.
    ///
    /// Constructed rather than observed, like the depth fixtures above: the
    /// point is that `contains` and `==` are different rules and only one of
    /// them is the one written down. The launcher already learned this once
    /// with "uninstall", where a loose match ate real applications.
    #[test]
    fn a_title_that_merely_starts_with_the_filtered_name_is_kept() {
        assert!(is_plumbing("Steamworks Common Redistributables"));
        assert!(!is_plumbing("Steamworks Common Redistributables Deluxe"));
        assert!(!is_plumbing("Counter-Strike 2"));
    }

    /// Comments are legal in these files and are not values.
    #[test]
    fn a_comment_is_not_read_as_a_pair() {
        let text =
            "\"AppState\"\n{\n// \"name\" \"nonsense\"\n\"appid\" \"1\"\n\"name\" \"Real\"\n}";
        assert_eq!(installed(text), Some(("1".to_string(), "Real".to_string())));
    }

    #[test]
    fn a_steam_target_becomes_the_launcher_and_an_argument() {
        let steam = PathBuf::from(r"C:\Steam");
        let (exe, args) = command("sill-game:steam/730", Some(&steam), None).expect("a command");

        assert_eq!(exe, steam.join("steam.exe"));
        assert_eq!(args, vec!["-applaunch".to_string(), "730".to_string()]);
    }

    /// The reason this function exists.
    ///
    /// `sill.launch` runs on targets an extension row and the model can name,
    /// so an id carrying a second argument has to be refused rather than
    /// quoted. Quoting is a rule that has to be right every time; refusing is
    /// a rule that can be read.
    #[test]
    fn an_id_carrying_a_second_argument_is_refused() {
        let steam = PathBuf::from(r"C:\Steam");

        for hostile in [
            "sill-game:steam/730 -applaunch 4",
            "sill-game:steam/730\"",
            "sill-game:steam/../../windows/system32",
            "sill-game:steam/",
        ] {
            assert!(
                command(hostile, Some(&steam), None).is_err(),
                "{hostile} was accepted"
            );
        }
    }

    #[test]
    fn an_unknown_library_is_refused() {
        let steam = PathBuf::from(r"C:\Steam");
        assert!(command("sill-game:origin/1", Some(&steam), None).is_err());
    }

    /// A target with no prefix never reaches here, and says so if it does.
    #[test]
    fn something_that_is_not_a_game_target_is_refused() {
        assert!(command("C:\\Windows\\notepad.exe", None, None).is_err());
    }

    /// Refusing because Steam is missing is different from refusing because
    /// the id was wrong, and the message has to say which.
    #[test]
    fn a_missing_steam_says_so_rather_than_blaming_the_id() {
        let err = command("sill-game:steam/730", None, None).expect_err("no Steam");
        assert!(err.contains("Steam"), "{err}");
    }

    const FORTNITE: &str = r#"{
        "DisplayName": "Fortnite",
        "AppName": "Fortnite",
        "InstallLocation": "C:\\Games\\Fortnite",
        "LaunchExecutable": "FortniteGame/Binaries/Win64/FortniteClient.exe",
        "AppCategories": ["public", "games", "applications"]
    }"#;

    /// Not verified against a real Epic install. See the module note.
    #[test]
    fn an_epic_manifest_gives_up_its_name_and_icon() {
        let (id, name, icon) = epic_item(FORTNITE).expect("a game");

        assert_eq!(id, "Fortnite");
        assert_eq!(name, "Fortnite");
        assert_eq!(
            icon.as_deref(),
            Some(r"C:\Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient.exe")
        );
    }

    /// The launcher itself and every engine plugin has a manifest in the same
    /// folder, and none of them are games.
    #[test]
    fn an_epic_entry_that_is_not_a_game_is_skipped() {
        let plugin = FORTNITE.replace("\"games\", ", "");
        assert_eq!(epic_item(&plugin), None);
    }

    #[test]
    fn an_epic_target_is_handed_to_the_launcher_rather_than_the_shell() {
        let epic = PathBuf::from(r"C:\Epic\EpicGamesLauncher.exe");
        let (exe, args) = command("sill-game:epic/Fortnite", None, Some(&epic)).expect("a command");

        assert_eq!(exe, epic);
        assert_eq!(args.len(), 1);
        assert!(args[0].starts_with("-com.epicgames.launcher://apps/Fortnite?"));
    }

    #[test]
    fn an_epic_name_with_a_quote_in_it_is_refused() {
        let epic = PathBuf::from(r"C:\Epic\EpicGamesLauncher.exe");
        assert!(command("sill-game:epic/Fort\"nite", None, Some(&epic)).is_err());
    }
}
