//! Click profiles, built-in presets and portable profile files.
//!
//! A profile only carries click parameters. Theme, language and hotkeys stay
//! global, so switching profiles never changes how the window looks or how the
//! global shortcut works.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::i18n::Key;
use crate::state::{Button, Settings};

use super::config::PersistedClickSettings;

pub const MAX_PROFILE_NAME_CHARS: usize = 40;
const MAX_IMPORT_BYTES: u64 = 1024 * 1024;
const PROFILE_FORMAT: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Default,
    Precise,
    Fast,
    Double,
}

impl Preset {
    pub const ALL: [Self; 4] = [Self::Default, Self::Precise, Self::Fast, Self::Double];

    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Precise => "precise",
            Self::Fast => "fast",
            Self::Double => "double",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" | "normal" | "обычный" => Some(Self::Default),
            "precise" | "accurate" | "точный" => Some(Self::Precise),
            "fast" | "rapid" | "быстрый" => Some(Self::Fast),
            "double" | "doubleclick" | "double-click" | "двойной" => Some(Self::Double),
            _ => None,
        }
    }

    pub fn label_key(self) -> Key {
        match self {
            Self::Default => Key::ProfilePresetDefault,
            Self::Precise => Key::ProfilePresetPrecise,
            Self::Fast => Key::ProfilePresetFast,
            Self::Double => Key::ProfilePresetDouble,
        }
    }

    pub fn settings(self) -> Settings {
        match self {
            Self::Default => Settings::default(),
            Self::Precise => {
                Settings::new(10.0, Button::Left, 40.0, 0.0).expect("the precise preset is valid")
            }
            Self::Fast => {
                Settings::new(100.0, Button::Left, 5.0, 2.0).expect("the fast preset is valid")
            }
            Self::Double => Settings::new(10.0, Button::Left, 10.0, 0.0)
                .expect("the double preset is valid")
                .with_multi_click(2)
                .expect("the double preset is valid"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub settings: Settings,
}

impl Profile {
    pub fn new(name: impl Into<String>, settings: Settings) -> Self {
        Self {
            name: name.into(),
            settings,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveProfile {
    Preset(Preset),
    User(String),
}

impl ActiveProfile {
    pub fn encode(&self) -> String {
        match self {
            Self::Preset(preset) => format!("preset:{}", preset.id()),
            Self::User(name) => format!("user:{name}"),
        }
    }

    pub fn decode(raw: &str, profiles: &[Profile]) -> Self {
        let raw = raw.trim();
        let explicit = raw
            .strip_prefix("preset:")
            .and_then(Preset::parse)
            .map(Self::Preset)
            .or_else(|| {
                raw.strip_prefix("user:")
                    .and_then(|name| find_user(name, profiles))
                    .map(|name| Self::User(name.to_string()))
            });
        if let Some(active) = explicit {
            return active;
        }

        // 0.9.x stored the active profile as a bare name. Keep those configs
        // usable, then fall back to the default preset for anything unknown.
        if let Some(profile) = profiles
            .iter()
            .find(|profile| profile.name.eq_ignore_ascii_case(raw))
        {
            return Self::User(profile.name.clone());
        }
        if let Some(preset) = Preset::parse(raw) {
            return Self::Preset(preset);
        }
        Self::Preset(Preset::Default)
    }
}

fn find_user<'a>(name: &str, profiles: &'a [Profile]) -> Option<&'a str> {
    profiles
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case(name.trim()))
        .map(|profile| profile.name.as_str())
}

/// Append ` (2)`, ` (3)`, ... until the name is free.
pub fn unique_name(base: &str, profiles: &[Profile]) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "Profile" } else { base };
    let taken = |name: &str| {
        profiles
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(name))
    };

    if !taken(base) {
        return base.to_string();
    }
    for index in 2..=9999 {
        let candidate = format!("{base} ({index})");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base} (copy)")
}

/// A portable profile file. The nested `[settings]` table keeps it readable
/// and leaves room for future metadata.
#[derive(Debug, Serialize, Deserialize)]
struct ProfileFile {
    #[serde(default = "default_format")]
    format: u32,
    #[serde(default)]
    name: String,
    settings: PersistedClickSettings,
}

fn default_format() -> u32 {
    PROFILE_FORMAT
}

pub fn export_file(path: &Path, name: &str, settings: &Settings) -> Result<PathBuf> {
    let path = prepare_export_path(path.to_path_buf(), name);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    if !parent.as_os_str().is_empty() && !parent.is_dir() {
        anyhow::bail!("folder {} does not exist", parent.display());
    }

    let file = ProfileFile {
        format: PROFILE_FORMAT,
        name: name.trim().to_string(),
        settings: PersistedClickSettings::new(settings),
    };
    let text = toml::to_string_pretty(&file).context("failed to serialize the profile")?;
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

pub fn import_file(path: &Path) -> Result<Profile> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to read {}", path.display()))?;
    if metadata.len() > MAX_IMPORT_BYTES {
        anyhow::bail!("the file is larger than 1 MiB");
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let file: ProfileFile = toml::from_str(&text).context("the file is not a valid profile")?;
    if file.format > PROFILE_FORMAT {
        anyhow::bail!("unsupported profile format {}", file.format);
    }

    let name = if file.name.trim().is_empty() {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Profile")
            .to_string()
    } else {
        file.name.trim().to_string()
    };
    if name.chars().count() > MAX_PROFILE_NAME_CHARS {
        anyhow::bail!("the profile name is longer than {MAX_PROFILE_NAME_CHARS} characters");
    }

    Ok(Profile::new(
        name,
        file.settings
            .to_settings()
            .context("the profile has invalid click settings")?,
    ))
}

/// Turn user input into an absolute-ish export path. A directory is completed
/// with a safe file name, and a missing extension becomes `.toml`.
pub fn resolve_text_path(text: &str, name: &str) -> Result<PathBuf> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("the file path is empty");
    }
    let path = expand_path(text);
    Ok(prepare_export_path(path, name))
}

fn prepare_export_path(path: PathBuf, name: &str) -> PathBuf {
    let mut path = if path.is_dir() {
        path.join(file_name(name))
    } else {
        path
    };
    if path.extension().is_none() {
        path.set_extension("toml");
    }
    path
}

fn expand_path(text: &str) -> PathBuf {
    if text == "~" {
        return home_directory().unwrap_or_else(|| PathBuf::from(text));
    }
    if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        if let Some(home) = home_directory() {
            return home.join(rest);
        }
    }
    PathBuf::from(text)
}

/// Suggested `~/Documents/<name>.toml`, falling back to the home directory or
/// the current directory when Documents does not exist.
pub fn default_export_path(name: &str) -> PathBuf {
    let base = home_directory()
        .map(|home| {
            let documents = home.join("Documents");
            if documents.is_dir() { documents } else { home }
        })
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(file_name(name))
}

pub fn file_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut previous_dash = false;
    for character in name.chars() {
        let invalid = character.is_control()
            || character.is_whitespace()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            );
        if invalid {
            if !previous_dash {
                slug.push('-');
                previous_dash = true;
            }
        } else {
            slug.push(character);
            previous_dash = false;
        }
    }
    let slug = slug.trim_matches(['-', '.']).trim();
    let slug = if slug.is_empty() { "profile" } else { slug };
    let mut slug: String = slug.chars().take(60).collect();
    while slug.ends_with(['-', '.']) {
        slug.pop();
    }
    format!("{slug}.toml")
}

fn home_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(home) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(home));
        }
    }
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ClickMode;

    #[test]
    fn presets_are_valid_and_distinct() {
        let mut seen = Vec::new();
        for preset in Preset::ALL {
            let settings = preset.settings();
            assert_eq!(settings.mode, ClickMode::Cursor);
            assert!(settings.readiness().is_ok());
            seen.push(preset.id());
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), Preset::ALL.len());
    }

    #[test]
    fn active_profile_round_trips_and_accepts_legacy_names() {
        let profiles = vec![
            Profile::new("Gaming", Preset::Fast.settings()),
            Profile::new("Работа", Preset::Precise.settings()),
        ];
        assert_eq!(
            ActiveProfile::decode("preset:double", &profiles),
            ActiveProfile::Preset(Preset::Double)
        );
        assert_eq!(
            ActiveProfile::decode("user:Gaming", &profiles),
            ActiveProfile::User("Gaming".to_string())
        );
        assert_eq!(
            ActiveProfile::decode("Работа", &profiles),
            ActiveProfile::User("Работа".to_string())
        );
        assert_eq!(
            ActiveProfile::decode("", &profiles),
            ActiveProfile::Preset(Preset::Default)
        );
        for active in [
            ActiveProfile::Preset(Preset::Fast),
            ActiveProfile::User("Gaming".to_string()),
        ] {
            assert_eq!(ActiveProfile::decode(&active.encode(), &profiles), active);
        }
    }

    #[test]
    fn duplicate_names_get_a_suffix() {
        let profiles = vec![
            Profile::new("Gaming", Preset::Fast.settings()),
            Profile::new("gaming (2)", Preset::Fast.settings()),
        ];
        assert_eq!(unique_name("Gaming", &profiles), "Gaming (3)");
        assert_eq!(unique_name("Office", &profiles), "Office");
        assert_eq!(unique_name("  ", &profiles), "Profile");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("My profile"), "My-profile.toml");
        assert_eq!(file_name("a/b:c*?"), "a-b-c.toml");
        assert_eq!(file_name("..."), "profile.toml");
        assert!(file_name(&"x".repeat(120)).len() <= 65);
    }

    #[test]
    fn export_and_import_round_trip() {
        let settings = Preset::Double.settings();
        let directory =
            std::env::temp_dir().join(format!("v-autoclicker-profile-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("My profile");
        let written = export_file(&path, "My profile", &settings).unwrap();
        assert_eq!(
            written.extension().and_then(|value| value.to_str()),
            Some("toml")
        );

        let imported = import_file(&written).unwrap();
        assert_eq!(imported.name, "My profile");
        assert!(imported.settings.equivalent(&settings));

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn import_falls_back_to_the_file_stem() {
        let directory =
            std::env::temp_dir().join(format!("v-autoclicker-profile-stem-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Office.toml");
        let file = ProfileFile {
            format: PROFILE_FORMAT,
            name: String::new(),
            settings: PersistedClickSettings::new(&Preset::Precise.settings()),
        };
        fs::write(&path, toml::to_string_pretty(&file).unwrap()).unwrap();
        assert_eq!(import_file(&path).unwrap().name, "Office");

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn import_rejects_bad_files() {
        let directory =
            std::env::temp_dir().join(format!("v-autoclicker-profile-bad-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("bad.toml");
        fs::write(&path, "not toml at all").unwrap();
        assert!(import_file(&path).is_err());

        fs::write(
            &path,
            "format = 99\nname = \"x\"\n[settings]\ncps = 50\nbutton = \"left\"\nhold_ms = 10\njitter_pct = 0\n",
        )
        .unwrap();
        assert!(import_file(&path).is_err());

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn text_paths_expand_tilde_and_add_extension() {
        let path = resolve_text_path("~/profiles/Work", "Work").unwrap();
        if let Some(home) = home_directory() {
            assert_eq!(path, home.join("profiles").join("Work.toml"));
        }

        let path = resolve_text_path("/tmp/office", "Office").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/office.toml"));
        assert!(resolve_text_path("   ", "Office").is_err());
    }
}
