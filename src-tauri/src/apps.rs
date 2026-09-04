//! Finding the applications a user can launch.
//!
//! Windows has no single registry of installed apps, but the Start Menu is the
//! closest thing: anything meant to be launched by a human puts a shortcut
//! there, in one of two well-known folders.
//!
//! Shortcuts are deliberately not resolved to their targets. A `.lnk` already
//! carries the working directory, arguments and icon the publisher intended,
//! and handing the shortcut itself to the shell launches it exactly as the
//! Start Menu would. Resolving to the raw executable would silently drop all
//! of that, and requires COM for no benefit.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One launchable application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRecord {
    /// Display name.
    pub name: String,
    /// What to hand the shell: a shortcut path, or a `shell:AppsFolder\<id>`.
    pub path: String,
    /// A real file we can pull an icon out of, when there is one.
    ///
    /// AppsFolder entries are identified by an AppUserModelID rather than a
    /// path, and a packaged app's icon lives in its manifest, so those have
    /// nothing here and fall back to a lettered tile.
    #[serde(default)]
    pub icon_source: Option<String>,
}

/// Prefix marking an entry that is launched through the Apps folder.
pub const APPS_FOLDER: &str = "shell:AppsFolder\\";

/// How deep to walk. Start Menu trees are shallow; this only stops a symlink
/// loop or a pathological vendor folder from running away.
const MAX_DEPTH: usize = 6;

/// Entries that are not applications a person means to launch.
///
/// Vendors ship uninstallers, help files and marketing links alongside the
/// real program, and they are pure noise in a launcher. Applied to **both**
/// scans: the Apps folder carries the same clutter, so filtering only the
/// Start Menu walk lets "7-Zip Help" back in through the other door.
fn is_noise(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();

    /*
     * Deliberately narrow.
     *
     * An earlier version also dropped documentation, FAQs, manuals, release
     * notes and website links. Comparing against Raycast's own application
     * list showed it keeps every one of those: "Git FAQs", "Node.js
     * documentation", "Node.js website", "Python 3.13 Module Docs",
     * "SageThumbs Online", "Steam Support Center". Filtering them made Sill's
     * list shorter than the thing it is being compared to, for no gain: they
     * are launchable, and someone who types "node docs" wants them.
     *
     * An uninstaller is different in kind. It is destructive, it is never what
     * someone reaching for an app meant, and every vendor ships one.
     */
    lower.starts_with("uninstall") || lower.contains("uninstall ")
}

/// Directories worth walking for shortcuts.
///
/// These are the same five roots Raycast scans on Windows, taken from its own
/// Search Scopes settings. Two of them are easy to miss:
///
/// - **The taskbar's pinned items.** Anything a user pinned to their taskbar
///   is by definition something they launch, and it lives in a Quick Launch
///   folder still named after Internet Explorer.
/// - **The Start Menu root, not `Start Menu\Programs`.** Some installers write
///   directly into the parent, and scanning only `Programs` never sees them.
///   Walking the parent picks up `Programs` anyway, since the walk recurses.
/// Where Windows keeps the shortcuts that stand for installed applications.
///
/// Also read by the watcher, so a folder added here starts being watched by
/// the same edit that starts it being scanned.
pub(crate) fn shortcut_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        let base = PathBuf::from(appdata);
        roots.push(
            base.join("Microsoft")
                .join("Internet Explorer")
                .join("Quick Launch")
                .join("User Pinned")
                .join("TaskBar"),
        );
        roots.push(base.join("Microsoft").join("Windows").join("Start Menu"));
    }

    if let Ok(program_data) = std::env::var("ProgramData") {
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu"),
        );
    }

    if let Ok(profile) = std::env::var("USERPROFILE") {
        roots.push(PathBuf::from(profile).join("Desktop"));
    }

    if let Ok(public) = std::env::var("PUBLIC") {
        roots.push(PathBuf::from(public).join("Desktop"));
    }

    roots
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<AppRecord>) {
    if depth > MAX_DEPTH {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        // Directory symlinks are skipped rather than followed: the Start Menu
        // contains junctions that would otherwise be walked repeatedly.
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            walk(&path, depth + 1, out);
            continue;
        }

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);

        // The same suffix set Flow Launcher indexes, plus `.url`.
        // `.appref-ms` is how ClickOnce apps are launched, and a bare `.exe`
        // dropped into one of these folders is a real application that a
        // shortcut-only scan never sees.
        if !matches!(
            extension.as_deref(),
            Some("lnk") | Some("url") | Some("exe") | Some("appref-ms")
        ) {
            continue;
        }

        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        if is_noise(name) {
            continue;
        }

        let full = path.to_string_lossy().to_string();
        out.push(AppRecord {
            name: name.to_string(),
            icon_source: Some(full.clone()),
            path: full,
        });
    }
}

/// Shortcuts found by walking the Start Menu directories.
///
/// Fast and dependency-free, and every entry carries a real file path, which
/// is what makes icon extraction possible. It does **not** see packaged apps.
pub fn scan_shortcuts() -> Vec<AppRecord> {
    let mut found = Vec::new();

    for root in shortcut_roots() {
        walk(&root, 0, &mut found);
    }

    dedupe(&mut found);
    found
}

/// Shortcuts and executables in folders somebody added themselves.
///
/// The five roots Windows itself lists are not everywhere people keep things.
/// A portable applications folder, a drive of tools carried between machines
/// and a scripts directory are all normal, and none of them are in the Start
/// Menu; before this the only way to reach one was to make a shortcut to it in
/// a folder Sill already read, which is an errand rather than a setting.
///
/// Walked with exactly the same code as the Start Menu, so an entry found here
/// carries the same icon, the same noise filtering and the same suffix set. A
/// folder that does not exist is skipped in silence: somebody who names a
/// folder on a drive that is not always plugged in should not be told off for
/// it every time they open the launcher.
pub fn scan_folders(roots: &[String]) -> Vec<AppRecord> {
    let mut found = Vec::new();

    for root in roots {
        let expanded = crate::icons::expand_env(root);
        if expanded.trim().is_empty() {
            continue;
        }
        walk(Path::new(&expanded), 0, &mut found);
    }

    dedupe(&mut found);
    found
}

/// Everything the Apps folder lists, which is what Explorer and Raycast show.
///
/// The Start Menu walk only sees `.lnk` files, so it misses every packaged
/// app: Calculator, Terminal, Photos, Settings, anything from the Store. Those
/// live in `shell:AppsFolder`, keyed by AppUserModelID rather than by path.
///
/// Enumerating that folder properly means COM (`SHGetKnownFolderItem` plus
/// `IEnumShellItems`), and the `windows` crate feature carrying `IShellItem`
/// is the one that pushed rustc into an out-of-memory abort on this machine.
/// `Get-StartApps` returns the same list, so it is used instead, off the
/// startup path because spawning PowerShell costs about a second.
/// One PowerShell round trip covering both registration-based sources.
///
/// Three registration-based sources:
///
/// - **`Get-StartApps`** is the Apps folder, and the only way to see packaged
///   apps.
/// - **`App Paths`** is the registry of executables an installer registered by
///   name, which is what the Run dialog resolves. It holds things that never
///   got a Start Menu entry.
/// - **`Uninstall`** is every installed program. Most of it is not launchable,
///   so an entry is kept only when `DisplayIcon` names a real `.exe`, and
///   updates and redistributables are dropped via `SystemComponent`,
///   `ParentKeyName` and `ReleaseType`.
///
/// Each registry source is read from three hives: 32-bit installers land in
/// the WOW6432Node view and per-user installs land under HKCU, so missing any
/// one of them loses real applications.
#[cfg(windows)]
const APPS_QUERY: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$out = @()

# Packaged apps keep their icon in the install directory, which Get-StartApps
# does not report, so the package list is joined in to find it.
$installed = @{}
Get-AppxPackage | ForEach-Object {
    if ($_.InstallLocation) { $installed[$_.PackageFamilyName] = $_.InstallLocation }
}

Get-StartApps | ForEach-Object {
    $location = ''
    if ($_.AppID -like '*!*') {
        $family = $_.AppID.Split('!')[0]
        if ($installed.ContainsKey($family)) { $location = $installed[$family] }
    }
    $out += [PSCustomObject]@{
        Name = $_.Name; Target = ''; AppID = $_.AppID; Install = $location
    }
}

$hives = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths'
)

foreach ($hive in $hives) {
    if (-not (Test-Path $hive)) { continue }
    Get-ChildItem $hive | ForEach-Object {
        $exe = (Get-ItemProperty $_.PSPath).'(default)'
        if ($exe) {
            $exe = $exe.Trim('"')
            if (Test-Path $exe) {
                $out += [PSCustomObject]@{
                    Name   = [System.IO.Path]::GetFileNameWithoutExtension($exe)
                    Target = $exe
                    AppID  = ''
                }
            }
        }
    }
}

$uninstall = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'
)

foreach ($hive in $uninstall) {
    if (-not (Test-Path $hive)) { continue }
    Get-ChildItem $hive | ForEach-Object {
        $p = Get-ItemProperty $_.PSPath
        if (-not $p.DisplayName) { return }
        if ($p.SystemComponent -eq 1) { return }
        if ($p.ParentKeyName) { return }
        if ($p.ReleaseType) { return }
        $icon = $p.DisplayIcon
        if (-not $icon) { return }
        $icon = ($icon -split ',')[0].Trim('"')
        if ($icon -notlike '*.exe') { return }
        if (-not (Test-Path $icon)) { return }
        $out += [PSCustomObject]@{
            Name   = $p.DisplayName
            Target = $icon
            AppID  = ''
        }
    }
}

$out | ConvertTo-Json -Compress
"#;

#[cfg(windows)]
pub fn scan_apps_folder() -> Vec<AppRecord> {
    use std::process::Command;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", APPS_QUERY])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    #[derive(Deserialize)]
    struct Entry {
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "AppID", default)]
        app_id: String,
        #[serde(rename = "Target", default)]
        target: String,
        /// Install directory of a packaged app, used to find its logo.
        #[serde(rename = "Install", default)]
        install: String,
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let Ok(entries) = serde_json::from_str::<Vec<Entry>>(&text) else {
        return Vec::new();
    };

    let mut found: Vec<AppRecord> = entries
        .into_iter()
        .filter(|e| !e.name.trim().is_empty() && !is_noise(&e.name))
        .map(|e| {
            if e.target.is_empty() {
                AppRecord {
                    // An AppID is sometimes a real path and sometimes a
                    // package identifier. When it is a package, the icon
                    // comes from the manifest in its install directory.
                    icon_source: std::path::Path::new(&e.app_id)
                        .is_file()
                        .then(|| e.app_id.clone())
                        .or_else(|| package_logo(&e.install)),
                    path: format!("{APPS_FOLDER}{}", e.app_id),
                    name: e.name,
                }
            } else {
                // An App Paths entry is a plain executable, so it launches and
                // draws its icon from the same file.
                AppRecord {
                    icon_source: Some(e.target.clone()),
                    path: e.target,
                    name: tidy_name(&e.name),
                }
            }
        })
        .collect();

    dedupe(&mut found);
    found
}

#[cfg(not(windows))]
pub fn scan_apps_folder() -> Vec<AppRecord> {
    Vec::new()
}

/// The Start Menu walk plus the Apps folder, merged.
///
/// Shortcuts win on a name collision because they carry a file path and so
/// can show a real icon; the Apps folder then contributes everything the walk
/// could not see, which is mostly packaged apps.
pub fn scan_all() -> Vec<AppRecord> {
    let mut found = scan_shortcuts();
    let extra = scan_path_executables();

    let mut names: std::collections::HashSet<String> =
        found.iter().map(|a| a.name.to_lowercase()).collect();
    let mut targets: std::collections::HashSet<String> =
        found.iter().filter_map(target_key).collect();

    for app in scan_apps_folder().into_iter().chain(extra) {
        // Both checks matter. An App Paths entry is named after its
        // executable, so "7zFM" and "7-Zip File Manager" share no name at all
        // while running the same binary; only the target catches that.
        if names.contains(&app.name.to_lowercase()) {
            continue;
        }
        if let Some(target) = target_key(&app) {
            if !targets.insert(target) {
                continue;
            }
        }

        names.insert(app.name.to_lowercase());
        found.push(app);
    }

    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

/// Trims installer metadata off a display name.
///
/// Uninstall entries are named for the installer's benefit, not the user's:
/// "7-Zip 26.02 (x64)", "Python 3.13.2 (64-bit)". A launcher wants the name
/// someone would actually type, so a trailing architecture tag and version
/// number are dropped.
///
/// Conservative on purpose. Only a trailing token that is unambiguously a
/// version (digits and dots) is removed, so "Windows 11" and "Python 3" keep
/// the number that is part of their name.
pub fn tidy_name_for_test(name: &str) -> String {
    tidy_name(name)
}

fn tidy_name(name: &str) -> String {
    let mut out = name.trim();

    for tag in [
        " (x64)",
        " (x86)",
        " (64-bit)",
        " (32-bit)",
        " (64 bit)",
        " (32 bit)",
    ] {
        if let Some(stripped) = out.strip_suffix(tag) {
            out = stripped.trim_end();
        }
    }

    // A trailing version needs at least one dot, so a bare trailing digit that
    // belongs to the name is left alone.
    if let Some((head, last)) = out.rsplit_once(' ') {
        let looks_like_version = last.contains('.')
            && last.chars().all(|c| c.is_ascii_digit() || c == '.')
            && last.chars().next().is_some_and(|c| c.is_ascii_digit());

        if looks_like_version && !head.is_empty() {
            out = head.trim_end();
        }
    }

    out.to_string()
}

/// The icon a packaged app declares in its manifest.
///
/// A packaged app has no executable to read an icon out of; its logo is a PNG
/// beside the manifest. The manifest names a base path like
/// `Assets\Square44x44Logo.png`, and that exact file usually does not exist:
/// Windows ships the scaled variants instead, `...targetsize-48.png`,
/// `...scale-200.png`. So the base name is matched as a prefix and the closest
/// variant to a list row is preferred.
fn package_logo(install_location: &str) -> Option<String> {
    if install_location.is_empty() {
        return None;
    }

    let manifest = std::path::Path::new(install_location).join("AppxManifest.xml");
    let text = std::fs::read_to_string(&manifest).ok()?;

    // Read as text rather than parsed: one attribute is wanted and an XML
    // dependency for it is not worth carrying.
    let declared = ["Square44x44Logo=\"", "Square150x150Logo=\"", "Logo=\""]
        .iter()
        .find_map(|needle| {
            let start = text.find(needle)? + needle.len();
            let end = text[start..].find('"')? + start;
            Some(text[start..end].to_string())
        })?;

    let relative = declared.replace('/', "\\");
    let full = std::path::Path::new(install_location).join(&relative);

    if full.is_file() {
        return Some(full.to_string_lossy().to_string());
    }

    // Fall back to whichever scaled variant sits next to it.
    let dir = full.parent()?;
    let stem = full.file_stem()?.to_string_lossy().to_string();

    let mut best: Option<(u32, String)> = None;

    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy().to_string();

        if !name.starts_with(&stem) || !name.to_ascii_lowercase().ends_with(".png") {
            continue;
        }

        // Roughly list-row sized beats a huge tile asset.
        let rank = if name.contains("targetsize-48") {
            0
        } else if name.contains("targetsize-32") || name.contains("targetsize-64") {
            1
        } else if name.contains("scale-100") {
            2
        } else if name.contains("targetsize") {
            3
        } else {
            4
        };

        if best.as_ref().is_none_or(|(current, _)| rank < *current) {
            best = Some((rank, path.to_string_lossy().to_string()));
        }
    }

    best.map(|(_, path)| path)
}

/// What kind of thing an entry is, judged by where it resolves to.
///
/// The source path says more than the name does. Everything under System32 is
/// a Windows tool whatever it calls itself, a `.msc` is a management console,
/// and a `.url` is a bookmark rather than a program. Labelling these beats
/// filtering them: an inclusive list stays scannable if each row says what it
/// is, and it means "Node.js documentation" can be listed without pretending
/// it is an application.
pub fn categorize(record: &AppRecord) -> &'static str {
    if record.path.starts_with(APPS_FOLDER) {
        return "Store App";
    }

    // A game's target is an identifier rather than a path, so nothing below
    // would recognise it, and every rule below is about where a file sits.
    if record.path.starts_with(crate::games::GAME) {
        return "Game";
    }

    let lower_path = record.path.to_ascii_lowercase();

    if lower_path.ends_with(".url") {
        return "Web Link";
    }

    let target = target_key(record).unwrap_or_else(|| lower_path.clone());
    let name = record.name.to_ascii_lowercase();

    // Management consoles and control panel applets are launched through a
    // host process, so the extension is the only thing that identifies them.
    if target.ends_with(".msc") || target.ends_with(".cpl") {
        return "System";
    }

    if target.ends_with(".chm") || target.ends_with(".hlp") {
        return "Documentation";
    }

    let windows_dir = std::env::var("SystemRoot")
        .unwrap_or_else(|_| r"C:\Windows".to_string())
        .to_ascii_lowercase();

    if target.starts_with(&windows_dir) {
        return "System";
    }

    // Documentation and support links are kept in the index, but they are not
    // applications and should not read as if they were.
    const DOCS: [&str; 7] = [
        "documentation",
        "manual",
        "readme",
        " docs",
        "release notes",
        "faq",
        "reference",
    ];
    if DOCS.iter().any(|needle| name.contains(needle)) {
        return "Documentation";
    }

    const WEB: [&str; 4] = ["website", "home page", "online", "support center"];
    if WEB.iter().any(|needle| name.contains(needle)) {
        return "Web Link";
    }

    "Application"
}

/// The executable an entry ultimately runs, lowercased, when that is knowable.
///
/// Name matching alone is not enough to merge sources: the Start Menu calls it
/// "Google Chrome" and App Paths calls it "chrome", and both run the same
/// binary. Comparing targets collapses those; comparing names would list the
/// browser twice.
///
/// Packaged apps have no executable path, so they can only ever be matched by
/// name and return `None` here.
pub fn target_key(record: &AppRecord) -> Option<String> {
    if record.path.starts_with(APPS_FOLDER) {
        return None;
    }

    let path = std::path::Path::new(&record.path);

    let resolved = if record.path.to_ascii_lowercase().ends_with(".lnk") {
        crate::lnk::target_of(path)?
    } else {
        record.path.clone()
    };

    Some(resolved.to_ascii_lowercase())
}

/// Executables reachable on `%PATH%`.
///
/// The source that separates a short list from an extensive one. Every CLI
/// tool, SDK and dev utility on the machine is here and in none of the other
/// sources: on this machine 36 PATH directories hold about 1,200 unique
/// executables, against roughly 200 entries from every other source combined.
///
/// Not recursive, matching Flow Launcher: `%PATH%` names the exact directories
/// a command resolves from, and descending into them would pull in bundled
/// helpers that are not commands anyone runs.
///
/// Directories already covered by the shortcut walk are skipped, so a program
/// that is both on PATH and in the Start Menu keeps its proper display name.
pub fn scan_path_executables() -> Vec<AppRecord> {
    let Ok(path) = std::env::var("PATH") else {
        return Vec::new();
    };

    let covered: Vec<PathBuf> = shortcut_roots();
    let mut seen_dirs = std::collections::HashSet::new();
    let mut found = Vec::new();

    for entry in path.split(';') {
        let dir = entry.trim();
        if dir.is_empty() {
            continue;
        }

        let dir = PathBuf::from(dir);
        if !seen_dirs.insert(dir.to_string_lossy().to_lowercase()) {
            continue;
        }
        if covered.iter().any(|root| dir.starts_with(root)) {
            continue;
        }

        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for file in entries.flatten() {
            let path = file.path();

            if !file.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                != Some("exe".to_string())
            {
                continue;
            }

            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if is_noise(name) {
                continue;
            }

            found.push(AppRecord {
                name: name.to_string(),
                icon_source: Some(path.to_string_lossy().to_string()),
                path: path.to_string_lossy().to_string(),
            });
        }
    }

    dedupe(&mut found);
    found
}

/// Sorts by name and drops genuine repeats.
///
/// The same app commonly appears in both the all-users and per-user trees, and
/// showing it twice is worse than picking either one.
///
/// **Matched on name AND target, not name alone.** Two entries can share a
/// name and be different programs: Raycast lists "Print Management" twice
/// because the 32-bit and 64-bit consoles are separate tools, and collapsing
/// on name would silently lose one. When neither has a resolvable target the
/// name is all there is, so it is used on its own.
fn dedupe(found: &mut Vec<AppRecord>) {
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found.dedup_by(|a, b| {
        if !a.name.eq_ignore_ascii_case(&b.name) {
            return false;
        }
        match (target_key(a), target_key(b)) {
            (Some(x), Some(y)) => x == y,
            _ => true,
        }
    });
}

// ----------------------------------------------------------- uninstalling

/*
 * An installed program's own uninstaller.
 *
 * `is_noise` above drops every shortcut a vendor ships called "Uninstall Foo",
 * and it is right to: somebody typing "foo" wants the program, and an
 * uninstaller sitting beside it in a list you are arrowing through quickly is
 * a hazard rather than a feature. This is the other half of that decision.
 * Uninstalling stays possible, and it is reached by name, from the action
 * panel, on the row for the program itself.
 *
 * Sill removes nothing. It finds the command line the installer wrote down and
 * runs it, and the vendor's own uninstaller is what asks and what deletes.
 */

/// One installed program, as the Uninstall hives describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub display_name: String,
    /// The command line to run to remove it.
    pub uninstall: String,
    /// `DisplayIcon`, which for most entries is the program's own executable.
    pub icon: Option<String>,
}

/// The executable a `DisplayIcon` names, without its quotes or icon index.
///
/// A display icon is usually the program's own path followed by a comma and a
/// number, and the number is which icon inside the file rather than part of
/// the path.
fn icon_executable(icon: &str) -> String {
    let head = icon.split(',').next().unwrap_or(icon);
    head.trim().trim_matches('"').to_ascii_lowercase()
}

/// Whether a registry entry is the program a row names.
///
/// Two keys, and both are exact. A path is the stronger of them: a row built
/// from the Uninstall hive carries that entry's own executable, so the match
/// is an identity rather than a resemblance. The name is the fallback for a
/// row that came from a Start Menu shortcut, compared after `tidy_name` has
/// taken the version and the architecture tag off both sides.
///
/// **Nothing fuzzy, on purpose.** Running the wrong uninstaller is the same
/// class of mistake as ending the wrong process, and "the display name
/// contains what the row is called" is not evidence: it would offer to
/// uninstall Visual Studio from a row for Visual Studio Code.
pub fn same_program(row_title: &str, row_target: &str, entry: &Installed) -> bool {
    if let Some(icon) = entry.icon.as_deref() {
        let named = icon_executable(icon);
        if !named.is_empty() && named == row_target.trim_matches('"').to_ascii_lowercase() {
            return true;
        }
    }

    tidy_name(&entry.display_name).eq_ignore_ascii_case(&tidy_name(row_title))
}

/// A command line split the way `CreateProcess` splits one.
///
/// The registry holds a command line rather than a path, and the two shapes it
/// comes in need different handling: a quoted program with arguments after it,
/// and an unquoted name such as the Windows installer with a product code.
/// Windows resolves an unquoted one by trying each space as if it were the end
/// of the path, first match winning, which is why a file test is passed in
/// rather than done here: the rule is then testable without any of those files
/// existing.
///
/// The arguments come back as one untouched string. They are handed on with
/// `raw_arg`, so what the vendor wrote is what their uninstaller receives,
/// rather than something re-quoted on the way through.
pub fn split_command(line: &str, exists: impl Fn(&str) -> bool) -> Option<(String, String)> {
    let line = line.trim();

    if line.is_empty() {
        return None;
    }

    if let Some(rest) = line.strip_prefix('"') {
        let (program, tail) = rest.split_once('"')?;
        return Some((program.to_string(), tail.trim().to_string()));
    }

    for (at, ch) in line.char_indices() {
        if ch != ' ' {
            continue;
        }

        let candidate = &line[..at];
        if exists(candidate) {
            return Some((candidate.to_string(), line[at..].trim().to_string()));
        }
    }

    // Nothing on disk matched, which is ordinary: the Windows installer is
    // found on the path rather than by a full name. The first word is it.
    let (program, arguments) = line.split_once(' ').unwrap_or((line, ""));
    Some((program.to_string(), arguments.trim().to_string()))
}

/// Every program the Uninstall hives know about.
///
/// The same three hives the app scan reads, and missing any one of them loses
/// real programs: a 32-bit installer lands in the WOW6432Node view and a
/// per-user install lands under HKCU.
///
/// Read directly rather than through PowerShell, which is what the app scan
/// uses. That scan runs once in the background and can afford a second; this
/// runs because somebody pressed a key and is waiting for the answer.
#[cfg(windows)]
pub fn installed() -> Vec<Installed> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const WOW: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    let mut out = Vec::new();

    for (root, path) in [
        (HKEY_LOCAL_MACHINE, UNINSTALL),
        (HKEY_LOCAL_MACHINE, WOW),
        (HKEY_CURRENT_USER, UNINSTALL),
    ] {
        out.extend(entries_under(root, path));
    }

    out
}

#[cfg(not(windows))]
pub fn installed() -> Vec<Installed> {
    Vec::new()
}

#[cfg(windows)]
fn entries_under(root: windows::Win32::System::Registry::HKEY, path: &str) -> Vec<Installed> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, KEY_READ,
    };

    let mut out = Vec::new();
    let mut hive = HKEY::default();

    // SAFETY: the path is a valid wide string and the handle is closed below
    // on every path out.
    let opened = unsafe { RegOpenKeyExW(root, &HSTRING::from(path), Some(0), KEY_READ, &mut hive) };

    if opened.is_err() {
        return out;
    }

    let mut index = 0u32;
    loop {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;

        // SAFETY: `length` says how much room `name` has, and the call writes
        // no more than that.
        let read = unsafe {
            RegEnumKeyExW(
                hive,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut length,
                None,
                None,
                None,
                None,
            )
        };

        if read.is_err() {
            break;
        }

        let key = String::from_utf16_lossy(&name[..length as usize]);
        let under = format!(r"{path}\{key}");

        // An entry with no name is not a program somebody can be shown, and
        // one with no uninstall string is not one this can do anything with.
        let display_name =
            read_string(root, &under, "DisplayName").filter(|name| !name.trim().is_empty());
        let uninstall =
            read_string(root, &under, "UninstallString").filter(|line| !line.trim().is_empty());

        if let (Some(display_name), Some(uninstall)) = (display_name, uninstall) {
            out.push(Installed {
                display_name,
                uninstall,
                icon: read_string(root, &under, "DisplayIcon"),
            });
        }

        index += 1;
    }

    // SAFETY: the handle came from the matching open above.
    unsafe {
        let _ = RegCloseKey(hive);
    }

    out
}

/// One string value out of the registry.
///
/// `pub(crate)` because the game libraries record where they are installed the
/// same way, and a second copy of this would be a second set of buffer sizing
/// mistakes to make.
#[cfg(windows)]
pub(crate) fn read_string(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    value: &str,
) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};

    let mut size: u32 = 0;

    // SAFETY: a null buffer asks for the size, which is what `size` receives.
    unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(path),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }

    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u16; size as usize / 2 + 1];
    let mut got = size;

    // SAFETY: the buffer is the size the call just asked for.
    unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(path),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut got),
        )
        .ok()
        .ok()?;
    }

    let text = String::from_utf16_lossy(&buffer);
    Some(text.trim_end_matches('\0').to_string())
}

/// The command that removes the program a row names, if Windows knows one.
///
/// `None` rather than a best guess. Somebody told that Windows lists no
/// uninstaller can go and look for themselves; somebody whose Visual Studio
/// was uninstalled from a row for Visual Studio Code cannot undo it.
pub fn uninstaller_for(row_title: &str, row_target: &str) -> Option<String> {
    let matched: Vec<Installed> = installed()
        .into_iter()
        .filter(|entry| same_program(row_title, row_target, entry))
        .collect();

    // Two entries answering to one name is the all-users and the per-user
    // install of the same thing, or two genuinely different programs. Either
    // way there is no way to tell from here which was meant, and picking one
    // is picking at random.
    if matched.len() != 1 {
        return None;
    }

    matched.into_iter().next().map(|entry| entry.uninstall)
}

/// Starts an uninstaller and leaves it to get on with it.
///
/// Not waited on. An uninstaller is a program with its own window that a
/// person is about to answer questions in, and waiting for it would hold the
/// action open for as long as they take.
#[cfg(windows)]
pub fn run_uninstaller(command: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let (program, arguments) = split_command(command, |candidate| {
        std::path::Path::new(candidate).is_file()
    })
    .ok_or("Windows recorded an empty uninstall command")?;

    let mut run = std::process::Command::new(&program);

    if !arguments.is_empty() {
        run.raw_arg(&arguments);
    }

    run.spawn()
        .map(|_| ())
        .map_err(|err| format!("{program} would not start: {err}"))
}

#[cfg(not(windows))]
pub fn run_uninstaller(_command: &str) -> Result<(), String> {
    Err("Only Windows has this.".to_string())
}

#[cfg(test)]
mod uninstalling {
    use super::*;

    fn entry(display_name: &str, icon: Option<&str>) -> Installed {
        Installed {
            display_name: display_name.to_string(),
            uninstall: r"C:\Program Files\App\unins000.exe".to_string(),
            icon: icon.map(str::to_string),
        }
    }

    /// The failure this exists to refuse.
    ///
    /// One name beginning with another is how a substring match runs the wrong
    /// thing, and it has cost this project a session before on a different
    /// surface. Here the wrong thing removes a program.
    #[test]
    fn a_name_that_merely_starts_with_the_row_is_not_the_row() {
        let studio = entry("Microsoft Visual Studio 2022", None);

        assert!(
            !same_program("Visual Studio Code", "", &studio),
            "Visual Studio Code matched the entry for Visual Studio"
        );
        assert!(
            !same_program("Visual Studio", "", &studio),
            "a leading substring is not an identity"
        );
    }

    #[test]
    fn a_version_and_an_architecture_tag_are_not_part_of_the_name() {
        // The row is built from this same entry with `tidy_name` applied, so
        // the two sides have to agree about what the program is called.
        let zip = entry("7-Zip 26.02 (x64)", None);
        assert!(same_program("7-Zip", "", &zip));
    }

    #[test]
    fn the_executable_settles_it_when_the_names_do_not() {
        // An App Paths row is named after its binary, so two spellings of one
        // program share no name at all while being the same thing.
        let zip = entry(
            "7-Zip File Manager",
            Some(r#""C:\Program Files\7-Zip\7zFM.exe",0"#),
        );

        assert!(same_program(
            "7zFM",
            r"C:\Program Files\7-Zip\7zFM.exe",
            &zip
        ));
        assert!(
            same_program("7zFM", r"c:\program files\7-zip\7zfm.exe", &zip),
            "a path is not case sensitive"
        );
        assert!(
            !same_program("7zFM", r"C:\Program Files\Other\other.exe", &zip),
            "a different executable is a different program"
        );
    }

    #[test]
    fn an_entry_with_no_icon_is_matched_by_name_alone_rather_than_by_nothing() {
        let app = entry("Some App", None);
        assert!(same_program("Some App", r"C:\somewhere\app.exe", &app));
    }

    #[test]
    fn a_quoted_program_keeps_its_arguments_exactly_as_written() {
        let (program, arguments) = split_command(
            r#""C:\Program Files\App\unins000.exe" /SILENT /NORESTART"#,
            |_| false,
        )
        .expect("a command");

        assert_eq!(program, r"C:\Program Files\App\unins000.exe");
        assert_eq!(arguments, "/SILENT /NORESTART");
    }

    #[test]
    fn an_installer_found_on_the_path_is_the_first_word() {
        let (program, arguments) = split_command(
            "MsiExec.exe /X{90160000-008C-0000-1000-0000000FF1CE}",
            |_| false,
        )
        .expect("a command");

        assert_eq!(program, "MsiExec.exe");
        assert_eq!(arguments, "/X{90160000-008C-0000-1000-0000000FF1CE}");
    }

    /// The rule Windows itself uses for an unquoted path with a space in it.
    #[test]
    fn an_unquoted_path_is_resolved_by_asking_which_prefix_is_a_file() {
        let line = r"C:\Program Files\App\unins000.exe /S";

        let (program, arguments) =
            split_command(line, |candidate| candidate.ends_with("unins000.exe"))
                .expect("a command");

        assert_eq!(program, r"C:\Program Files\App\unins000.exe");
        assert_eq!(arguments, "/S");

        // With nothing on disk it falls back to the first word, which is the
        // wrong program, and that is exactly why an uninstaller that is really
        // there is found by the test above.
        let (guessed, _) = split_command(line, |_| false).expect("a command");
        assert_eq!(guessed, r"C:\Program");
    }

    #[test]
    fn an_empty_command_is_nothing_rather_than_a_program_called_nothing() {
        assert_eq!(split_command("", |_| false), None);
        assert_eq!(split_command("   ", |_| false), None);
    }
}

#[cfg(test)]
mod discovering {
    use super::*;

    /// A folder somebody named is walked on exactly the terms the Start Menu
    /// is, which is the whole reason it reuses `walk`.
    ///
    /// Three claims in one place because they are one behaviour: the shortcut
    /// is found, the uninstaller beside it is not, and a folder that is not
    /// there is passed over rather than being an error somebody has to dismiss
    /// every time a removable drive is unplugged.
    #[test]
    fn a_folder_of_your_own_is_walked_like_the_start_menu() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let root = dir.path();

        std::fs::write(root.join("Portable Editor.lnk"), b"").expect("a shortcut");
        std::fs::write(root.join("Uninstall Portable Editor.lnk"), b"").expect("a shortcut");
        std::fs::write(root.join("notes.txt"), b"").expect("a file");

        let nested = root.join("Tools");
        std::fs::create_dir(&nested).expect("a folder");
        std::fs::write(nested.join("Deeper.exe"), b"").expect("an executable");

        let found = scan_folders(&[
            root.to_string_lossy().to_string(),
            root.join("not-here").to_string_lossy().to_string(),
        ]);

        let names: Vec<&str> = found.iter().map(|one| one.name.as_str()).collect();

        assert!(names.contains(&"Portable Editor"), "{names:?}");
        assert!(
            names.contains(&"Deeper"),
            "the walk did not recurse: {names:?}"
        );
        assert!(
            !names.iter().any(|one| one.starts_with("Uninstall")),
            "the noise filter did not apply: {names:?}"
        );
        assert!(
            !names.contains(&"notes"),
            "a file that is not launchable was indexed: {names:?}"
        );
    }

    /// A folder written with an environment variable in it is expanded.
    ///
    /// The reason it matters is that a settings field is typed by a person,
    /// and `%USERPROFILE%\Tools` is how somebody writes a folder that has to
    /// work on more than one machine. Without expansion it is a literal path
    /// that exists nowhere and fails silently, which is the worst of both.
    #[test]
    fn a_folder_written_with_an_environment_variable_is_expanded() {
        let dir = tempfile::tempdir().expect("a temp directory");
        std::fs::write(dir.path().join("Named.lnk"), b"").expect("a shortcut");

        // Set rather than borrowed from the machine, so the test says what it
        // depends on. Scoped to this process.
        std::env::set_var("SILL_TEST_FOLDER", dir.path());

        let found = scan_folders(&["%SILL_TEST_FOLDER%".to_string()]);
        assert_eq!(
            found
                .iter()
                .map(|one| one.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Named"]
        );
    }

    /// An empty list walks nothing, which is what most machines will do.
    #[test]
    fn no_folders_means_no_work() {
        assert!(scan_folders(&[]).is_empty());
        assert!(scan_folders(&["   ".to_string()]).is_empty());
    }
}
