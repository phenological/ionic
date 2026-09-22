use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde::Deserialize;

fn main() -> ExitCode {
    let command = env::args().nth(1).unwrap_or_default();
    let outcome = match command.as_str() {
        "sync" => run_sync(),
        "check" => run_check(),
        "show" => run_show(),
        "package-version" => run_package_version(),
        other => Err(format!(
            "unknown command '{other}', expected: sync | check | show | package-version"
        )),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    package: String,
    format: FormatVersions,
    #[serde(default)]
    allow_max_above_current: bool,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormatVersions {
    current: u16,
    min_supported: u16,
    max_supported: u16,
}

fn parse_config(text: &str) -> Result<Vec<Release>, String> {
    serde_json::from_str(text).map_err(|error| format!("config.json is not valid: {error}"))
}

fn validate_releases(releases: &[Release]) -> Result<(), String> {
    if releases.is_empty() {
        return Err("config.json must hold at least one release".into());
    }
    for release in releases {
        validate_release(release)?;
    }
    let mut previous_version: Option<(u64, u64, u64)> = None;
    for release in releases {
        let version = parse_semver(&release.package)?;
        if let Some(above) = previous_version
            && version >= above
        {
            return Err(format!(
                "config.json must be newest first: {} is not below the entry above it",
                release.package
            ));
        }
        previous_version = Some(version);
    }
    validate_format_does_not_regress(releases)
}

fn validate_release(release: &Release) -> Result<(), String> {
    let format = &release.format;
    if format.min_supported > format.current {
        return Err(format!(
            "release {}: min_supported must not exceed current",
            release.package
        ));
    }
    if release.allow_max_above_current {
        if format.max_supported < format.current {
            return Err(format!(
                "release {}: max_supported must be at least current",
                release.package
            ));
        }
    } else if format.max_supported != format.current {
        return Err(format!(
            "release {}: max_supported must equal current (set allow_max_above_current to read a future format on purpose)",
            release.package
        ));
    }
    Ok(())
}

fn validate_format_does_not_regress(releases: &[Release]) -> Result<(), String> {
    for pair in releases.windows(2) {
        let newer = &pair[0];
        let older = &pair[1];
        if newer.format.current < older.format.current {
            return Err(format!(
                "format current must not go backwards: {} writes {} but the older {} writes {}",
                newer.package, newer.format.current, older.package, older.format.current
            ));
        }
        if newer.format.max_supported < older.format.max_supported {
            return Err(format!(
                "format max_supported must not shrink: {} reads up to {} but the older {} reads up to {}",
                newer.package,
                newer.format.max_supported,
                older.package,
                older.format.max_supported
            ));
        }
    }
    Ok(())
}

fn parse_semver(version: &str) -> Result<(u64, u64, u64), String> {
    let mut parts = version.split('.');
    let major = take_number(&mut parts, version)?;
    let minor = take_number(&mut parts, version)?;
    let patch = take_number(&mut parts, version)?;
    if parts.next().is_some() {
        return Err(format!("version '{version}' must be major.minor.patch"));
    }
    Ok((major, minor, patch))
}

fn take_number(parts: &mut std::str::Split<'_, char>, version: &str) -> Result<u64, String> {
    parts
        .next()
        .and_then(|part| part.parse::<u64>().ok())
        .ok_or_else(|| format!("version '{version}' must be major.minor.patch"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn read_config(root: &Path) -> Result<Vec<Release>, String> {
    let path = root.join("config.json");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let releases = parse_config(&text)?;
    validate_releases(&releases)?;
    Ok(releases)
}

fn current_release(root: &Path) -> Result<Release, String> {
    read_config(root)?
        .into_iter()
        .next()
        .ok_or_else(|| "config.json has no releases".into())
}

fn generated_path(root: &Path) -> PathBuf {
    root.join("crates/parser/src/ion/version_generated.rs")
}

fn cargo_path(root: &Path) -> PathBuf {
    root.join("Cargo.toml")
}

fn render_generated(release: &Release) -> String {
    format!(
        "pub const CURRENT_VERSION: u16 = {};\n\
         pub const MIN_SUPPORTED_VERSION: u16 = {};\n\
         pub const MAX_SUPPORTED_VERSION: u16 = {};\n",
        release.format.current, release.format.min_supported, release.format.max_supported
    )
}

fn run_sync() -> Result<(), String> {
    let root = repo_root();
    let release = current_release(&root)?;
    write_cargo_version(&root, &release.package)?;
    write_atomically(
        &generated_path(&root),
        render_generated(&release).as_bytes(),
    )?;
    println!(
        "synced package {} and format reads {}..={} (writes {})",
        release.package,
        release.format.min_supported,
        release.format.max_supported,
        release.format.current
    );
    Ok(())
}

fn read_cargo_version(root: &Path) -> Result<String, String> {
    let path = cargo_path(root);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("Cargo.toml is not valid toml: {error}"))?;
    document
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(|version| version.as_str())
        .map(|version| version.to_string())
        .ok_or_else(|| "Cargo.toml is missing [workspace.package] version".into())
}

fn write_cargo_version(root: &Path, package_version: &str) -> Result<(), String> {
    let path = cargo_path(root);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("Cargo.toml is not valid toml: {error}"))?;
    let version_slot = document
        .get_mut("workspace")
        .and_then(|workspace| workspace.get_mut("package"))
        .and_then(|package| package.get_mut("version"))
        .ok_or("Cargo.toml is missing [workspace.package] version")?;
    *version_slot = toml_edit::value(package_version);
    write_atomically(&path, document.to_string().as_bytes())
}

fn run_check() -> Result<(), String> {
    let root = repo_root();
    let release = current_release(&root)?;

    let cargo_version = read_cargo_version(&root)?;
    if cargo_version != release.package {
        return Err(format!(
            "Cargo.toml version {cargo_version} does not match config.json {} (run `make sync`)",
            release.package
        ));
    }

    let path = generated_path(&root);
    let generated_text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if generated_text != render_generated(&release) {
        return Err("version_generated.rs does not match config.json (run `make sync`)".into());
    }

    println!("config.json, Cargo.toml and version_generated.rs agree");
    Ok(())
}

fn run_show() -> Result<(), String> {
    let root = repo_root();
    let releases = read_config(&root)?;
    let current = &releases[0];
    println!("package: {}", current.package);
    println!(
        "format: writes {}, reads {}..={}",
        current.format.current, current.format.min_supported, current.format.max_supported
    );
    println!("history:");
    for release in &releases {
        println!(
            "  {} -> format reads {}..={} (writes {})",
            release.package,
            release.format.min_supported,
            release.format.max_supported,
            release.format.current
        );
    }
    Ok(())
}

fn run_package_version() -> Result<(), String> {
    let root = repo_root();
    println!("{}", current_release(&root)?.package);
    Ok(())
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("cannot build a temporary file name")?;
    let temp_path = path.with_file_name(format!("{file_name}.tmp"));
    fs::write(&temp_path, bytes)
        .map_err(|error| format!("cannot write {}: {error}", temp_path.display()))?;
    fs::rename(&temp_path, path)
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_entry() -> &'static str {
        r#"[{"package":"0.1.0","format":{"current":1,"min_supported":1,"max_supported":1}}]"#
    }

    #[test]
    fn accepts_a_simple_config() {
        let releases = parse_config(one_entry()).unwrap();
        assert!(validate_releases(&releases).is_ok());
    }

    #[test]
    fn rejects_empty_config() {
        let releases = parse_config("[]").unwrap();
        assert!(validate_releases(&releases).is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        let text = r#"[{"package":"0.1.0","format":{"current":1,"min_supported":1,"max_supported":1},"extra":true}]"#;
        assert!(parse_config(text).is_err());
    }

    #[test]
    fn allows_min_supported_zero() {
        let text =
            r#"[{"package":"0.1.0","format":{"current":1,"min_supported":0,"max_supported":1}}]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_ok());
    }

    #[test]
    fn rejects_max_above_current_without_flag() {
        let text =
            r#"[{"package":"0.1.0","format":{"current":1,"min_supported":1,"max_supported":2}}]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_err());
    }

    #[test]
    fn allows_max_above_current_with_flag() {
        let text = r#"[{"package":"0.1.0","format":{"current":1,"min_supported":1,"max_supported":2},"allow_max_above_current":true}]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_ok());
    }

    #[test]
    fn rejects_invalid_semver_even_with_one_entry() {
        let text =
            r#"[{"package":"v1","format":{"current":1,"min_supported":1,"max_supported":1}}]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_err());
    }

    #[test]
    fn allows_package_only_releases_that_share_a_format() {
        let text = r#"[
            {"package":"0.1.2","format":{"current":1,"min_supported":1,"max_supported":1}},
            {"package":"0.1.1","format":{"current":1,"min_supported":1,"max_supported":1}},
            {"package":"0.1.0","format":{"current":1,"min_supported":1,"max_supported":1}}
        ]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_ok());
    }

    #[test]
    fn rejects_history_that_is_not_newest_first() {
        let text = r#"[
            {"package":"0.1.0","format":{"current":1,"min_supported":1,"max_supported":1}},
            {"package":"0.2.0","format":{"current":1,"min_supported":1,"max_supported":1}}
        ]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_err());
    }

    #[test]
    fn rejects_format_that_goes_backwards() {
        let text = r#"[
            {"package":"0.2.0","format":{"current":1,"min_supported":1,"max_supported":1}},
            {"package":"0.1.0","format":{"current":2,"min_supported":1,"max_supported":2}}
        ]"#;
        let releases = parse_config(text).unwrap();
        assert!(validate_releases(&releases).is_err());
    }

    #[test]
    fn sorts_versions_numerically_not_as_text() {
        assert!(parse_semver("0.10.0").unwrap() > parse_semver("0.9.9").unwrap());
    }
}
