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
