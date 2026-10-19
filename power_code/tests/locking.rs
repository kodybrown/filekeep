use power_code::locking::DirectoryLock;
use std::{
    fs,
    process::{Child, Command},
    time::{Duration, Instant},
};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn lock_child_helper() {
    let Some(directory) = std::env::var_os("FILEKEEP_LOCK_TEST_DIRECTORY") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let _lock = DirectoryLock::try_acquire(&directory).unwrap();
    fs::write(directory.join("ready"), "ready").unwrap();
    loop {
        std::thread::park_timeout(Duration::from_secs(1));
    }
}

#[test]
fn process_termination_releases_lock_and_removes_the_marker() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lock_child_helper", "--nocapture"])
            .env("FILEKEEP_LOCK_TEST_DIRECTORY", directory.path())
            .spawn()
            .unwrap(),
    );
    let started = Instant::now();
    while !directory.path().join("ready").exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "Child did not acquire the lock"
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "Child exited before acquiring the lock"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(DirectoryLock::try_acquire(directory.path()).is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(!directory.path().join("lock").exists());
    assert!(DirectoryLock::try_acquire(directory.path()).is_ok());
}
