//! Release checks and in-place binary replacement.
//!
//! This module deliberately has no egui dependency. Every function here is
//! blocking, so the GUI runs checks and downloads on a background thread and
//! feeds progress back through a channel.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Version compiled into the running binary.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const RELEASES_URL: &str = "https://api.github.com/repos/savvhurt/V-AutoClicker/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/savvhurt/V-AutoClicker/releases";
const CHECKSUMS_ASSET: &str = "SHA256SUMS";
const USER_AGENT: &str = concat!("v-autoclicker/", env!("CARGO_PKG_VERSION"));

/// Release metadata is tiny; anything bigger is not a GitHub API response.
const MAX_JSON_BYTES: u64 = 1024 * 1024;
/// The packaged binaries are a few megabytes. Refuse to buffer more than this.
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A `major.minor.patch` release version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    /// Parse a tag such as `v0.9.2` or `1.0.0`. Pre-release suffixes are
    /// ignored, and missing components default to zero.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix('v')
            .or_else(|| text.strip_prefix('V'))
            .unwrap_or(text);
        let core = text.split(['-', '+']).next()?;
        let mut parts = core.split('.');

        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        if parts.next().is_some() {
            return None;
        }

        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A newer release that can be installed on this platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub page_url: String,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_size: u64,
    pub checksums_url: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseJson {
    tag_name: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Debug, Deserialize)]
struct AssetJson {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// The raw binary published for the current platform.
pub fn asset_name_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("v-autoclicker-linux-x86_64"),
        ("windows", "x86_64") => Some("v-autoclicker-windows-x86_64.exe"),
        _ => None,
    }
}

fn asset_name() -> Result<&'static str> {
    asset_name_for(env::consts::OS, env::consts::ARCH)
        .ok_or_else(|| anyhow!("automatic updates are not available for this platform"))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn status_error(code: u16) -> anyhow::Error {
    match code {
        403 | 429 => anyhow!(
            "GitHub API rate limit reached; try again later or download the release from {RELEASES_PAGE}"
        ),
        404 => anyhow!("no published releases were found at {RELEASES_PAGE}"),
        _ => anyhow!("GitHub returned HTTP {code}"),
    }
}

/// Ask GitHub for the latest release. `Ok(None)` means the running version is
/// current.
pub fn check() -> Result<Option<Release>> {
    let agent = agent();
    let mut response = agent
        .get(RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .context("failed to reach GitHub")?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }

    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_JSON_BYTES)
        .read_to_string()
        .context("failed to read the GitHub release response")?;
    let json: ReleaseJson =
        serde_json::from_str(&text).context("GitHub returned an unexpected release response")?;
    let current = Version::parse(CURRENT_VERSION)
        .ok_or_else(|| anyhow!("the built-in version `{CURRENT_VERSION}` is invalid"))?;

    select_release(json, current, asset_name()?)
}

fn select_release(
    json: ReleaseJson,
    current: Version,
    asset_name: &str,
) -> Result<Option<Release>> {
    let version = Version::parse(&json.tag_name)
        .ok_or_else(|| anyhow!("unexpected release tag `{}`", json.tag_name))?;
    if version <= current {
        return Ok(None);
    }

    let page_url = json
        .html_url
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| RELEASES_PAGE.to_string());

    let asset = json
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
            anyhow!(
                "release {} does not contain {asset_name}; download it manually from {page_url}",
                json.tag_name
            )
        })?;

    if asset.browser_download_url.trim().is_empty() {
        bail!(
            "release {} has no download URL for {asset_name}",
            json.tag_name
        );
    }

    let checksums_url = json
        .assets
        .iter()
        .find(|asset| asset.name == CHECKSUMS_ASSET)
        .map(|asset| asset.browser_download_url.clone())
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| {
            anyhow!(
                "release {} does not publish {CHECKSUMS_ASSET}; download it manually from {page_url}",
                json.tag_name
            )
        })?;

    Ok(Some(Release {
        version,
        page_url,
        asset_name: asset.name.clone(),
        asset_url: asset.browser_download_url.clone(),
        asset_size: asset.size,
        checksums_url,
    }))
}

/// Download the release binary, verify it against the published SHA-256
/// checksum, and return the verified bytes.
///
/// The callback receives progress in `0.0..=1.0` when the server reports a
/// content length.
pub fn download(release: &Release, progress: &mut dyn FnMut(f32)) -> Result<Vec<u8>> {
    let agent = agent();
    let checksums = fetch_checksums(&agent, &release.checksums_url)?;
    let expected = checksums
        .get(&release.asset_name)
        .ok_or_else(|| anyhow!("{CHECKSUMS_ASSET} has no entry for {}", release.asset_name))?
        .clone();

    let mut response = agent
        .get(&release.asset_url)
        .call()
        .context("failed to download the update")?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }

    let total = response
        .body()
        .content_length()
        .filter(|total| *total > 0)
        .or(Some(release.asset_size))
        .filter(|total| *total > 0);

    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .reader();
    let mut bytes = Vec::new();
    let mut buffer = vec![0u8; 32 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .context("failed to read the update download")?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(total) = total {
            progress((bytes.len() as f32 / total as f32).clamp(0.0, 1.0));
        }
    }

    if bytes.is_empty() {
        bail!("the downloaded update is empty");
    }

    let actual = sha256_hex(&bytes);
    if !actual.eq_ignore_ascii_case(&expected) {
        bail!("the downloaded update failed its SHA-256 check");
    }

    progress(1.0);
    Ok(bytes)
}

fn fetch_checksums(agent: &ureq::Agent, url: &str) -> Result<HashMap<String, String>> {
    let mut response = agent
        .get(url)
        .call()
        .context("failed to download the update checksums")?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }

    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_JSON_BYTES)
        .read_to_string()
        .context("failed to read the update checksums")?;

    Ok(parse_checksums(&text))
}

/// Parse the output of `sha256sum` (`<hex>  <file>` lines).
fn parse_checksums(text: &str) -> HashMap<String, String> {
    let mut checksums = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let name = name.trim_start_matches('*');
        let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
        checksums.insert(name.to_string(), hash.to_ascii_lowercase());
    }
    checksums
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Replace the running executable with `bytes`.
///
/// Returns the path of the replaced executable. On Unix the rename is atomic;
/// on Windows the running image is first renamed aside because a loaded
/// executable cannot be overwritten.
pub fn install(bytes: &[u8]) -> Result<PathBuf> {
    let target = env::current_exe().context("cannot resolve the current executable path")?;
    replace_binary(&target, bytes)?;
    Ok(target)
}

fn replace_binary(target: &Path, bytes: &[u8]) -> Result<()> {
    let directory = target
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", target.display()))?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("{} has no usable file name", target.display()))?;
    let pid = std::process::id();
    let staged = directory.join(format!(".{file_name}.new-{pid}"));

    #[cfg(windows)]
    {
        let backup = directory.join(format!(".{file_name}.old-{pid}"));
        fs::rename(target, &backup).with_context(|| {
            format!(
                "failed to move the running executable aside; close any other copy and check permissions in {}",
                directory.display()
            )
        })?;

        if let Err(error) = write_staged(&staged, bytes).and_then(|()| {
            fs::rename(&staged, target)
                .with_context(|| format!("failed to replace {}", target.display()))
        }) {
            let _ = fs::remove_file(&staged);
            let _ = fs::rename(&backup, target);
            return Err(error);
        }
    }

    #[cfg(not(windows))]
    {
        write_staged(&staged, bytes)?;
        if let Err(error) = fs::rename(&staged, target) {
            let _ = fs::remove_file(&staged);
            return Err(error).with_context(|| {
                format!(
                    "failed to replace {}; check that {} is writable",
                    target.display(),
                    directory.display()
                )
            });
        }
    }

    Ok(())
}

fn write_staged(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o755);
    }

    let mut file = options
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

/// Start a fresh copy of the installed executable.
pub fn relaunch() -> Result<()> {
    let executable = env::current_exe().context("cannot resolve the current executable path")?;
    let mut command = std::process::Command::new(executable);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .context("failed to restart the application")?;
    Ok(())
}

/// Remove leftovers from an interrupted or completed update. Best effort: the
/// running process may not have permission to delete every entry.
pub fn cleanup_stale_files() {
    let Ok(executable) = env::current_exe() else {
        return;
    };
    let Some(directory) = executable.parent() else {
        return;
    };
    let Some(name) = executable.file_name().and_then(|name| name.to_str()) else {
        return;
    };

    cleanup_in(directory, name);
}

fn cleanup_in(directory: &Path, name: &str) {
    // A concurrent install could be writing its staged file right now, so only
    // old leftovers are removed. A backup from a previous Windows update is
    // normally older than this by the time the app starts again.
    const MIN_AGE: Duration = Duration::from_secs(60);

    let prefixes = [format!(".{name}.old-"), format!(".{name}.new-")];
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !prefixes
            .iter()
            .any(|prefix| file_name.starts_with(prefix.as_str()))
        {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= MIN_AGE);
        if old_enough {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_json(tag: &str) -> ReleaseJson {
        serde_json::from_str(&format!(
            r#"{{
                "tag_name": "{tag}",
                "body": "Notes",
                "html_url": "https://example.test/releases/{tag}",
                "assets": [
                    {{
                        "name": "v-autoclicker-linux-x86_64",
                        "browser_download_url": "https://example.test/linux",
                        "size": 2048
                    }},
                    {{
                        "name": "SHA256SUMS",
                        "browser_download_url": "https://example.test/SHA256SUMS",
                        "size": 128
                    }}
                ]
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(Version::parse("v0.9.2").unwrap().to_string(), "0.9.2");
        assert_eq!(Version::parse("1.0.0").unwrap().to_string(), "1.0.0");
        assert_eq!(Version::parse("V2.3").unwrap().to_string(), "2.3.0");
        assert_eq!(
            Version::parse("0.10.0-beta.1").unwrap().to_string(),
            "0.10.0"
        );

        assert!(Version::parse("0.10.0").unwrap() > Version::parse("0.9.9").unwrap());
        assert!(Version::parse("1.0.0").unwrap() > Version::parse("0.99.99").unwrap());
        assert_eq!(
            Version::parse("v0.9.2").unwrap(),
            Version::parse("0.9.2").unwrap()
        );

        for invalid in ["", "v", "one.two.three", "1.2.3.4", "1..2", "1.2.x"] {
            assert!(Version::parse(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn newer_release_is_selected() {
        let release = select_release(
            sample_json("v0.10.0"),
            Version::parse("0.9.2").unwrap(),
            "v-autoclicker-linux-x86_64",
        )
        .unwrap()
        .unwrap();

        assert_eq!(release.version.to_string(), "0.10.0");
        assert_eq!(release.asset_name, "v-autoclicker-linux-x86_64");
        assert_eq!(release.asset_url, "https://example.test/linux");
        assert_eq!(release.checksums_url, "https://example.test/SHA256SUMS");
        assert_eq!(release.asset_size, 2048);
    }

    #[test]
    fn same_or_older_release_is_ignored() {
        for tag in ["v0.9.2", "v0.9.1", "v0.8.9"] {
            assert!(
                select_release(
                    sample_json(tag),
                    Version::parse("0.9.2").unwrap(),
                    "v-autoclicker-linux-x86_64"
                )
                .unwrap()
                .is_none(),
                "{tag}"
            );
        }
    }

    #[test]
    fn missing_platform_asset_is_an_error() {
        let error = select_release(
            sample_json("v0.10.0"),
            Version::parse("0.9.2").unwrap(),
            "v-autoclicker-windows-x86_64.exe",
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("does not contain"));
    }

    #[test]
    fn missing_checksums_is_an_error() {
        let mut json = sample_json("v0.10.0");
        json.assets.retain(|asset| asset.name != CHECKSUMS_ASSET);
        let error = select_release(
            json,
            Version::parse("0.9.2").unwrap(),
            "v-autoclicker-linux-x86_64",
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("SHA256SUMS"));
    }

    #[test]
    fn malformed_tag_is_an_error() {
        let mut json = sample_json("nightly");
        json.assets.clear();
        assert!(
            select_release(
                json,
                Version::parse("0.9.2").unwrap(),
                "v-autoclicker-linux-x86_64"
            )
            .is_err()
        );
    }

    #[test]
    fn checksums_are_parsed() {
        let text = "\
            # generated by CI\n\
            0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  v-autoclicker-linux-x86_64\n\
            ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789 *dist/v-autoclicker-windows-x86_64.exe\n\
            not-a-hash  ignored\n";
        let checksums = parse_checksums(text);

        assert_eq!(
            checksums.get("v-autoclicker-linux-x86_64").unwrap(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            checksums.get("v-autoclicker-windows-x86_64.exe").unwrap(),
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
        assert!(!checksums.contains_key("ignored"));
    }

    #[test]
    fn sha256_matches_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn asset_names_follow_the_platform() {
        assert_eq!(
            asset_name_for("linux", "x86_64"),
            Some("v-autoclicker-linux-x86_64")
        );
        assert_eq!(
            asset_name_for("windows", "x86_64"),
            Some("v-autoclicker-windows-x86_64.exe")
        );
        assert_eq!(asset_name_for("macos", "aarch64"), None);
    }

    #[test]
    fn binary_is_replaced_in_place() {
        let directory = env::temp_dir().join(format!(
            "v-autoclicker-update-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("v-autoclicker");
        fs::write(&target, b"old").unwrap();

        replace_binary(&target, b"new binary").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new binary");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }

        cleanup_in(&directory, "v-autoclicker");
        let _ = fs::remove_dir_all(&directory);
    }
}
