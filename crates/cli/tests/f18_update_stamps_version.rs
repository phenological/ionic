mod common;
use std::fs;

use common::*;

const FIXTURE: &str = "tiny.msdata.mzML0.99.9.ion";

fn version_of(path: &std::path::Path) -> u16 {
    let bytes = fs::read(path).unwrap();
    u16::from_le_bytes([bytes[9], bytes[10]])
}

fn header_crc_matches(path: &std::path::Path) -> bool {
    let bytes = fs::read(path).unwrap();
    crc32fast::hash(&bytes[0..1020]) == u32::from_le_bytes(bytes[1020..1024].try_into().unwrap())
}

fn copy_fixture_into(dir: &std::path::Path) -> std::path::PathBuf {
    let target = dir.join(FIXTURE);
    read_bytes_to_tempfile_copy(&ion_fixture(FIXTURE), &target);
    target
}

fn run_update(args: &[&std::ffi::OsStr]) -> (bool, String, String) {
    let output = ionic()
        .arg("convert")
        .arg("--update")
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        strip_ansi(&String::from_utf8_lossy(&output.stdout)),
        strip_ansi(&String::from_utf8_lossy(&output.stderr)),
    )
}

fn check_passes(path: &std::path::Path) -> bool {
    ionic()
        .arg("cat")
        .arg("--check")
        .arg(path)
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn update_in_place_stamps_version_then_skips() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = copy_fixture_into(dir.path());
    assert_eq!(version_of(&file), 0, "fixture must start at version 0");
    let original_len = fs::metadata(&file).unwrap().len();

    let (ok, stdout, _) = run_update(&["-i".as_ref(), dir.path().as_ref()]);
    assert!(ok, "got: {stdout}");
    assert!(stdout.contains("ok=1"), "got: {stdout}");
    assert_eq!(version_of(&file), ionic::format::CURRENT_VERSION);
    assert!(header_crc_matches(&file));
    assert_eq!(fs::metadata(&file).unwrap().len(), original_len);
    assert!(check_passes(&file));

    let stamped = fs::read(&file).unwrap();
    let (ok, stdout, _) = run_update(&["-i".as_ref(), dir.path().as_ref()]);
    assert!(ok, "got: {stdout}");
    assert!(stdout.contains("skipped=1"), "got: {stdout}");
    assert_eq!(fs::read(&file).unwrap(), stamped);
}

#[test]
fn update_rejects_encoder_flags() {
    let dir = tempfile::TempDir::new().unwrap();
    copy_fixture_into(dir.path());

    for flags in [
        vec!["--mz-window", "50"],
        vec!["--level", "3"],
        vec!["--block-size", "4"],
        vec!["--force-f32"],
        vec!["--storage", "memory"],
    ] {
        let output = ionic()
            .arg("convert")
            .arg("--update")
            .arg("-i")
            .arg(dir.path())
            .args(&flags)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{flags:?} must be rejected with --update, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn update_to_output_path_leaves_source_untouched() {
    let input_dir = tempfile::TempDir::new().unwrap();
    let output_dir = tempfile::TempDir::new().unwrap();
    let source = copy_fixture_into(input_dir.path());
    let target = output_dir.path().join(FIXTURE);

    let args: [&std::ffi::OsStr; 4] = [
        "-i".as_ref(),
        input_dir.path().as_ref(),
        "-o".as_ref(),
        output_dir.path().as_ref(),
    ];
    let (ok, stdout, _) = run_update(&args);
    assert!(ok, "got: {stdout}");
    assert!(stdout.contains("ok=1"), "got: {stdout}");
    assert_eq!(version_of(&source), 0);
    assert_eq!(version_of(&target), ionic::format::CURRENT_VERSION);
    assert!(check_passes(&target));

    let (ok, stdout, _) = run_update(&args);
    assert!(ok, "got: {stdout}");
    assert!(stdout.contains("skipped=1"), "got: {stdout}");

    fs::copy(&source, &target).unwrap();
    assert_eq!(version_of(&target), 0);
    let (ok, stdout, _) = run_update(&args);
    assert!(ok, "got: {stdout}");
    assert!(stdout.contains("ok=1"), "got: {stdout}");
    assert_eq!(version_of(&target), ionic::format::CURRENT_VERSION);
}

#[test]
fn update_fails_on_unsupported_version_and_leaves_file_alone() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join(FIXTURE);
    let mut bytes = fs::read(ion_fixture(FIXTURE)).unwrap();
    bytes[9..11].copy_from_slice(&u16::MAX.to_le_bytes());
    let header_crc = crc32fast::hash(&bytes[0..1020]);
    bytes[1020..1024].copy_from_slice(&header_crc.to_le_bytes());
    fs::write(&file, &bytes).unwrap();

    let (ok, stdout, stderr) = run_update(&["-i".as_ref(), dir.path().as_ref()]);
    assert!(!ok);
    assert!(stdout.contains("failed=1"), "got: {stdout}");
    assert!(
        stderr.contains("unsupported format version"),
        "got: {stderr}"
    );
    assert_eq!(fs::read(&file).unwrap(), bytes);
}
