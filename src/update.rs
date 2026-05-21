//! Self-update support.

use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

pub const DEFAULT_REPO: &str = "MohamedElashri/radd";

#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub repo: String,
    pub target: Option<String>,
    pub install_path: Option<PathBuf>,
    pub yes: bool,
    pub check_only: bool,
}

#[derive(Debug, Clone)]
struct ReleaseAsset {
    tag: String,
    version: String,
    package: String,
    archive: String,
    checksum: String,
    base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdateStatus {
    Current,
    Available,
    OlderThanCurrent,
}

pub fn run(options: &UpdateOptions) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    let tag = match &options.target {
        Some(target) => normalize_tag(target),
        None => resolve_latest_tag(&options.repo)?,
    };
    let asset = release_asset(&options.repo, &tag)?;
    let status = compare_versions(&asset.version, current_version);

    println!("current version: v{current_version}");
    println!("latest version: {}", asset.tag);

    match status {
        UpdateStatus::Current => {
            println!("radd is already up to date");
            return Ok(());
        }
        UpdateStatus::OlderThanCurrent => {
            bail!(
                "release {} is older than the running radd version v{}",
                asset.tag,
                current_version
            );
        }
        UpdateStatus::Available => {}
    }

    println!("update available: v{current_version} -> {}", asset.tag);

    if options.check_only {
        return Ok(());
    }

    let install_path = options
        .install_path
        .clone()
        .map_or_else(env::current_exe, Ok)
        .context("could not determine current executable path")?;

    if !options.yes && !confirm_update(&asset, &install_path)? {
        println!("update cancelled");
        return Ok(());
    }

    install_update(&asset, &install_path)?;
    println!(
        "updated radd to {} at {}",
        asset.tag,
        install_path.display()
    );
    Ok(())
}

fn resolve_latest_tag(repo: &str) -> Result<String> {
    let output = Command::new("curl")
        .args([
            "-fsSLI",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            &format!("https://github.com/{repo}/releases/latest"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .context("could not run curl to resolve latest release")?;

    if !output.status.success() {
        bail!("curl failed while resolving the latest release for {repo}");
    }

    let latest_url =
        String::from_utf8(output.stdout).context("latest release URL was not UTF-8")?;
    let tag = latest_url
        .trim()
        .rsplit('/')
        .next()
        .filter(|tag| !tag.is_empty())
        .context("could not parse latest release tag from GitHub redirect")?;

    Ok(normalize_tag(tag))
}

fn release_asset(repo: &str, tag: &str) -> Result<ReleaseAsset> {
    let platform = current_platform()?;
    let tag = normalize_tag(tag);
    let version = tag.trim_start_matches('v').to_string();
    let package = format!("radd-v{version}-{platform}");
    let archive = format!("{package}.tar.gz");
    let checksum = format!("{archive}.sha256");
    let base_url = format!("https://github.com/{repo}/releases/download/{tag}");

    Ok(ReleaseAsset {
        tag,
        version,
        package,
        archive,
        checksum,
        base_url,
    })
}

fn current_platform() -> Result<String> {
    let os = match env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        other => bail!("unsupported operating system for release update: {other}"),
    };

    let arch = match env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => bail!("unsupported CPU architecture for release update: {other}"),
    };

    Ok(format!("{os}-{arch}"))
}

fn normalize_tag(tag: &str) -> String {
    let trimmed = tag.trim();
    if trimmed.starts_with('v') {
        trimmed.to_string()
    } else {
        format!("v{trimmed}")
    }
}

fn compare_versions(candidate: &str, current: &str) -> UpdateStatus {
    let candidate_parts = parse_version_parts(candidate);
    let current_parts = parse_version_parts(current);
    let part_count = candidate_parts.len().max(current_parts.len());

    for index in 0..part_count {
        let candidate_part = candidate_parts.get(index).copied().unwrap_or_default();
        let current_part = current_parts.get(index).copied().unwrap_or_default();

        match candidate_part.cmp(&current_part) {
            std::cmp::Ordering::Greater => return UpdateStatus::Available,
            std::cmp::Ordering::Less => return UpdateStatus::OlderThanCurrent,
            std::cmp::Ordering::Equal => {}
        }
    }

    UpdateStatus::Current
}

fn parse_version_parts(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn confirm_update(asset: &ReleaseAsset, install_path: &Path) -> Result<bool> {
    print!(
        "Download and install {} to {}? [y/N] ",
        asset.tag,
        install_path.display()
    );
    io::stdout().flush().context("could not flush prompt")?;

    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .context("could not read update confirmation")?;
    Ok(matches!(response.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

fn install_update(asset: &ReleaseAsset, install_path: &Path) -> Result<()> {
    ensure_command("curl")?;
    ensure_command("tar")?;
    let parent = install_path
        .parent()
        .context("install path must have a parent directory")?;
    if !parent.is_dir() {
        bail!("install directory does not exist: {}", parent.display());
    }

    let temp_dir = create_temp_dir()?;
    let result = download_extract_and_replace(asset, install_path, &temp_dir);
    let cleanup = fs::remove_dir_all(&temp_dir);
    result?;
    cleanup.with_context(|| {
        format!(
            "could not remove temporary directory {}",
            temp_dir.display()
        )
    })
}

fn download_extract_and_replace(
    asset: &ReleaseAsset,
    install_path: &Path,
    temp_dir: &Path,
) -> Result<()> {
    let archive_path = temp_dir.join(&asset.archive);
    let checksum_path = temp_dir.join(&asset.checksum);

    println!("downloading {}", asset.archive);
    download_file(
        &format!("{}/{}", asset.base_url, asset.archive),
        &archive_path,
    )?;
    download_file(
        &format!("{}/{}", asset.base_url, asset.checksum),
        &checksum_path,
    )?;
    verify_checksum(&archive_path, &checksum_path)?;
    println!("checksum verified");

    extract_archive(&archive_path, temp_dir)?;
    let extracted_binary = temp_dir.join(&asset.package).join("radd");
    if !extracted_binary.is_file() {
        bail!(
            "release archive did not contain expected binary: {}",
            extracted_binary.display()
        );
    }

    replace_binary(&extracted_binary, install_path)
}

fn ensure_command(command: &str) -> Result<()> {
    which::which(command)
        .with_context(|| format!("required command not found on PATH: {command}"))?;
    Ok(())
}

fn download_file(url: &str, destination: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-fsSLo"])
        .arg(destination)
        .arg(url)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("could not run curl for {url}"))?;

    if !status.success() {
        bail!("curl failed while downloading {url}");
    }

    Ok(())
}

fn verify_checksum(archive_path: &Path, checksum_path: &Path) -> Result<()> {
    let checksum_text = fs::read_to_string(checksum_path)
        .with_context(|| format!("could not read checksum file {}", checksum_path.display()))?;
    let expected = checksum_text
        .split_whitespace()
        .next()
        .context("checksum file was empty")?;
    let actual = sha256_hex(archive_path)?;

    if !actual.eq_ignore_ascii_case(expected) {
        bail!(
            "checksum mismatch for {}: expected {}, got {}",
            archive_path.display(),
            expected,
            actual
        );
    }

    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .with_context(|| format!("could not read {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_archive(archive_path: &Path, temp_dir: &Path) -> Result<()> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(temp_dir)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("could not run tar for {}", archive_path.display()))?;

    if !status.success() {
        bail!("tar failed while extracting {}", archive_path.display());
    }

    Ok(())
}

fn replace_binary(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("install path must have a parent directory")?;
    let temporary_destination = parent.join(format!(
        ".radd-update-{}-{}",
        std::process::id(),
        monotonic_suffix()
    ));

    fs::copy(source, &temporary_destination).with_context(|| {
        format!(
            "could not copy update binary from {} to {}",
            source.display(),
            temporary_destination.display()
        )
    })?;
    make_executable(&temporary_destination)?;
    fs::rename(&temporary_destination, destination).with_context(|| {
        format!(
            "could not replace {} with downloaded update",
            destination.display()
        )
    })?;

    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("could not inspect {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("could not set executable permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn create_temp_dir() -> Result<PathBuf> {
    let path = env::temp_dir().join(format!(
        "radd-update.{}.{}",
        std::process::id(),
        monotonic_suffix()
    ));
    fs::create_dir(&path)
        .with_context(|| format!("could not create temporary directory {}", path.display()))?;
    Ok(path)
}

fn monotonic_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::{UpdateStatus, compare_versions, normalize_tag, release_asset};

    #[test]
    fn normalizes_release_tags() {
        assert_eq!(normalize_tag("0.2.0"), "v0.2.0");
        assert_eq!(normalize_tag("v0.2.0"), "v0.2.0");
    }

    #[test]
    fn compares_numeric_versions() {
        assert_eq!(compare_versions("0.2.0", "0.1.9"), UpdateStatus::Available);
        assert_eq!(compare_versions("0.1.0", "0.1.0"), UpdateStatus::Current);
        assert_eq!(compare_versions("0.1", "0.1.0"), UpdateStatus::Current);
        assert_eq!(
            compare_versions("0.1.0", "0.2.0"),
            UpdateStatus::OlderThanCurrent
        );
    }

    #[test]
    fn builds_release_asset_names() {
        let asset = release_asset("owner/repo", "0.2.0").expect("asset");

        assert_eq!(asset.tag, "v0.2.0");
        assert!(asset.archive.starts_with("radd-v0.2.0-"));
        assert!(asset.archive.ends_with(".tar.gz"));
        assert_eq!(asset.checksum, format!("{}.sha256", asset.archive));
        assert_eq!(
            asset.base_url,
            "https://github.com/owner/repo/releases/download/v0.2.0"
        );
    }
}
