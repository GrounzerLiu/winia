//! Window lifecycle integration tests.
//!
//! These drive `fixture_all window_lifecycle` in one process per scenario. The fixture writes its
//! observations to a file and, for the scenarios that must stay alive, deliberately keeps running; the
//! test observes the process and then terminates it. They need a real desktop window, like the rest of
//! the UI suite.

use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());
static NEXT_PORT: AtomicU16 = AtomicU16::new(13_100);

fn workspace_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("winia is one level below the workspace root")
        .to_path_buf()
}

fn fixture_exe() -> PathBuf {
    let exe = workspace_root()
        .join("target/debug")
        .join(format!("fixture_all{}", std::env::consts::EXE_SUFFIX));
    assert!(
        exe.exists(),
        "fixture_all is not built; run `cargo build --bin fixture_all --features debug-server` (expected {})",
        exe.display()
    );
    exe
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = workspace_root().join("target/window-fixture").join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

struct Fixture {
    child: Child,
    out: PathBuf,
    _serial: MutexGuard<'static, ()>,
}

impl Fixture {
    fn spawn(mode: &str, tag: &str) -> Self {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch_dir(tag);
        let out = dir.join("out.txt");
        let port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);
        let child = Command::new(fixture_exe())
            .arg("window_lifecycle")
            .env("WINDOW_FIXTURE_MODE", mode)
            .env("WINDOW_FIXTURE_OUT", &out)
            .env("WINDOW_FIXTURE_STATE", dir.join("state"))
            .env("WINIA_DEBUG_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn fixture");
        Fixture {
            child,
            out,
            _serial: serial,
        }
    }

    fn wait(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return Some(status),
                None if Instant::now() >= deadline => return None,
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    fn is_alive(&mut self) -> bool {
        self.child.try_wait().expect("try_wait").is_none()
    }

    fn text(&self) -> String {
        std::fs::read_to_string(&self.out).unwrap_or_default()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_live_size_change_reaches_the_platform_and_the_layout() {
    let mut fixture = Fixture::spawn("size", "size");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(status.is_some(), "the size fixture should finish");
    let text = fixture.text();
    assert!(text.contains("state="), "missing state line: {text:?}");
    assert!(
        text.contains("520"),
        "state did not adopt the requested width: {text:?}"
    );
    assert!(
        text.contains("333"),
        "state did not adopt the requested height: {text:?}"
    );
    assert!(text.contains("layout=("), "missing layout line: {text:?}");
}

#[test]
fn a_vetoed_close_keeps_the_window_and_the_process_alive() {
    let mut fixture = Fixture::spawn("close-veto", "close-veto");
    std::thread::sleep(Duration::from_millis(1_800));
    assert!(
        fixture.is_alive(),
        "a vetoed close must not end the process"
    );
    let text = fixture.text();
    // The handler's write replaces the fixture's "closing" marker, so the request line is the
    // observable proof that the close actually reached the application.
    assert!(
        text.contains("requested=Command"),
        "the handler did not see the request: {text:?}"
    );
}

#[test]
fn an_accepted_close_ends_the_process() {
    let mut fixture = Fixture::spawn("close-accept", "close-accept");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(status.is_some(), "an accepted close should end the process");
    assert!(
        fixture.text().contains("requested=Command"),
        "the handler did not see the request"
    );
}

#[test]
fn destroying_the_last_window_obeys_the_policy() {
    let mut exit = Fixture::spawn("policy-exit", "policy-exit");
    assert!(
        exit.wait(Duration::from_secs(10)).is_some(),
        "the default policy should end the process"
    );
    // The fixture holds the suite's serial lock; release it before spawning the next one.
    drop(exit);

    let mut stay = Fixture::spawn("policy-stay", "policy-stay");
    std::thread::sleep(Duration::from_millis(1_800));
    assert!(
        stay.is_alive(),
        "StayResident should keep the process alive after its last window"
    );
    assert!(stay.text().contains("destroying"));
}

#[test]
fn a_destroyed_declaration_does_not_recreate_the_window() {
    let mut fixture = Fixture::spawn("destroy-stay", "destroy-stay");
    std::thread::sleep(Duration::from_millis(1_800));
    assert!(
        fixture.is_alive(),
        "StayResident should keep the process alive"
    );
    let text = fixture.text();
    assert!(
        text.contains("created"),
        "the fixture did not compose: {text:?}"
    );
    assert!(
        !text.contains("recreated"),
        "destroy() was undone by the still-present declaration: {text:?}"
    );
}

#[test]
fn a_keyed_window_restores_its_saved_state_across_runs() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("persist");
    let out = dir.join("out.txt");
    let port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);
    let mut first = Command::new(fixture_exe())
        .arg("window_lifecycle")
        .env("WINDOW_FIXTURE_MODE", "persist")
        .env("WINDOW_FIXTURE_OUT", &out)
        .env("WINDOW_FIXTURE_STATE", dir.join("state"))
        .env("WINIA_DEBUG_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn first persist run");
    let status = first.wait().expect("first persist run");
    assert!(status.success(), "first run failed: {status:?}");
    let first_text = std::fs::read_to_string(&out).unwrap_or_default();
    assert!(
        first_text.contains("counter=0"),
        "first run: {first_text:?}"
    );

    let _ = std::fs::remove_file(&out);
    let port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);
    let mut second = Command::new(fixture_exe())
        .arg("window_lifecycle")
        .env("WINDOW_FIXTURE_MODE", "persist")
        .env("WINDOW_FIXTURE_OUT", &out)
        .env("WINDOW_FIXTURE_STATE", dir.join("state"))
        .env("WINIA_DEBUG_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn second persist run");
    let status = second.wait().expect("second persist run");
    assert!(status.success(), "second run failed: {status:?}");
    let second_text = std::fs::read_to_string(&out).unwrap_or_default();
    assert!(
        second_text.contains("counter=1"),
        "second run did not restore the saved counter: {second_text:?}"
    );
}
