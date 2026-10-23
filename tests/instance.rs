use sequoia::instance::Instance;
use std::{path::PathBuf, process::Command};

#[test]
fn child_owner() {
    let Some(root) = std::env::var_os("SEQUOIA_OWNER_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let _owner = Instance::start(&root, vec![]).unwrap().unwrap();
    std::fs::write(root.join("ready"), "ready").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}

#[test]
fn process_termination_releases_lock() {
    let root = tempfile::tempdir_in("target").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_owner"])
        .env("SEQUOIA_OWNER_TEST_ROOT", root.path())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !root.path().join("ready").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let ready = root.path().join("ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "child did not acquire the lock");
    assert!(Instance::start(root.path(), vec![]).unwrap().is_some());
}

#[test]
fn child_launch() {
    let Some(root) = std::env::var_os("SEQUOIA_INSTANCE_TEST_ROOT") else {
        return;
    };
    let paths = vec![
        PathBuf::from("музыка с пробелами.flac"),
        PathBuf::from("https://example.com/video"),
    ];
    assert!(
        Instance::start(&PathBuf::from(root), paths)
            .unwrap()
            .is_none()
    );
}

#[test]
fn second_process_forwards_paths_and_lock_is_released() {
    let root = tempfile::tempdir_in("target").unwrap();
    let first = Instance::start(root.path(), vec![]).unwrap().unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_launch"])
        .env("SEQUOIA_INSTANCE_TEST_ROOT", root.path())
        .status()
        .unwrap();
    assert!(child.success());
    assert_eq!(
        first.requests().unwrap(),
        vec![vec![
            std::env::current_dir()
                .unwrap()
                .join("музыка с пробелами.flac"),
            PathBuf::from("https://example.com/video"),
        ]]
    );
    assert!(first.requests().unwrap().is_empty());
    drop(first);
    assert!(Instance::start(root.path(), vec![]).unwrap().is_some());
}

#[test]
fn empty_launch_activates_and_profiles_are_independent() {
    let root = tempfile::tempdir_in("target").unwrap();
    let first = Instance::start(root.path(), vec![]).unwrap().unwrap();
    assert!(Instance::start(root.path(), vec![]).unwrap().is_none());
    assert_eq!(first.requests().unwrap(), vec![Vec::<PathBuf>::new()]);
    assert!(
        Instance::start(&root.path().join("other-profile"), vec![])
            .unwrap()
            .is_some()
    );
    // Incomplete writes are never consumed by the UI.
    std::fs::write(root.path().join("launch-requests/incomplete.tmp"), "[").unwrap();
    assert!(first.requests().unwrap().is_empty());
}
