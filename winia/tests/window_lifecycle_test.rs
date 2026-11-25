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

impl Fixture {
    /// Poll the fixture's observation file until `done` accepts it, so a check does not depend on a
    /// fixed sleep matching this machine's startup time.
    fn wait_for_text(&mut self, timeout: Duration, done: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let text = self.text();
            if done(&text) || Instant::now() >= deadline {
                return text;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
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
fn closing_a_parent_finalizes_its_child_windows() {
    let mut fixture = Fixture::spawn("nested-close", "nested-close");
    let status = fixture.wait(Duration::from_secs(15));
    let text = fixture.text();
    assert!(
        status.is_some(),
        "parent close left a child window alive; observations: {text:?}"
    );
    assert!(
        text.contains("child-closed"),
        "the child window was not finalized by the owner cascade: {text:?}"
    );
}

#[test]
fn closing_a_sibling_window_reruns_the_remaining_content() {
    let mut fixture = Fixture::spawn("sibling-close", "sibling-close");
    let text = fixture.wait_for_text(Duration::from_secs(15), |text| {
        text.lines()
            .any(|line| line.starts_with("seq=") && line.contains("windows=1"))
            && text.matches("seq=").count() >= 3
    });
    assert!(
        text.contains("child=composed"),
        "the secondary window never composed: {text:?}"
    );
    // The secondary window closes itself (the write its Close button makes). Seeing this marker
    // means the primary window re-ran its content without any pointer input.
    assert!(
        text.contains("parent-saw-closed"),
        "the primary window kept its stale composition after the sibling closed: {text:?}"
    );
    // The window-list label lives in a group that reads nothing reactive: the State-driven
    // recompose skips it, so only the close path's whole-content rebuild refreshes it. Removing
    // that rebuild leaves the label at `windows=2` and this assertion fails.
    let last_seq = text
        .lines()
        .filter_map(|line| line.strip_prefix("seq="))
        .last()
        .unwrap_or_default();
    assert!(
        last_seq.contains("windows=1"),
        "the window list label still counts the closed sibling: {text:?}"
    );
}

#[test]
fn an_existing_window_receives_the_latest_content_closure() {
    let mut fixture = Fixture::spawn("content-update", "content-update");
    let status = fixture.wait(Duration::from_secs(15));
    let text = fixture.text();
    assert!(
        status.is_some(),
        "updated child content never composed: {text:?}"
    );
    assert!(
        text.contains("content=1"),
        "the child kept its original content closure: {text:?}"
    );
}

#[test]
fn closing_a_parent_cancels_its_pending_child() {
    let mut fixture = Fixture::spawn("pending-child-close", "pending-child-close");
    std::thread::sleep(Duration::from_millis(1_800));
    assert!(
        fixture.is_alive(),
        "StayResident should keep the process alive after the parent closes"
    );
    let text = fixture.text();
    assert!(
        !text.contains("pending-child-composed"),
        "a pending child escaped the owner cascade: {text:?}"
    );
    assert!(
        text.contains("pending-child-closed"),
        "canceling a pending child skipped its on_close callback: {text:?}"
    );
    assert!(
        text.contains("reentrant-composed"),
        "on_close could not re-enter window creation: {text:?}"
    );
}

#[test]
fn an_empty_title_is_reflected_in_window_state() {
    let mut fixture = Fixture::spawn("empty-title", "empty-title");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(status.is_some(), "the empty-title fixture should finish");
    let text = fixture.text();
    assert!(
        text.contains("declared-title=Winia") && text.contains("title=Winia"),
        "state did not reflect the platform default title: {text:?}"
    );
}

#[test]
fn an_empty_snapshot_clears_the_previous_persisted_state() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("persist-clear");
    let out = dir.join("out.txt");
    let mut port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);

    let mut run = |mode: &str| {
        let _ = std::fs::remove_file(&out);
        let mut child = Command::new(fixture_exe())
            .arg("window_lifecycle")
            .env("WINDOW_FIXTURE_MODE", mode)
            .env("WINDOW_FIXTURE_OUT", &out)
            .env("WINDOW_FIXTURE_STATE", dir.join("state"))
            .env("WINIA_DEBUG_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn persist-clear run");
        port += 1;
        let status = child.wait().expect("persist-clear run");
        assert!(status.success(), "{mode} failed: {status:?}");
        std::fs::read_to_string(&out).unwrap_or_default()
    };

    let first = run("persist");
    assert!(first.contains("counter=0"), "first run: {first:?}");
    run("persist-clear");
    let third = run("persist");
    assert!(
        third.contains("counter=0"),
        "the empty snapshot did not clear the old value: {third:?}"
    );
}

#[test]
fn commands_queued_before_realization_apply_before_the_first_frame() {
    let mut fixture = Fixture::spawn("initial-commands", "initial-commands");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(
        status.is_some(),
        "the initial-commands fixture should finish"
    );
    let text = fixture.text();
    assert!(text.contains("initial-title=early title"), "{text:?}");
    assert!(text.contains("initial-size=(640.0, 480.0)"), "{text:?}");
    assert!(text.contains("initial-visible=false"), "{text:?}");
    assert!(text.contains("initial-realized=true"), "{text:?}");
}

#[test]
fn destroying_before_realization_skips_content_but_runs_on_close() {
    let mut fixture = Fixture::spawn("initial-destroy", "initial-destroy");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(
        status.is_some(),
        "the initial-destroy fixture should finish"
    );
    let text = fixture.text();
    assert!(text.contains("initial-on-close"), "{text:?}");
    assert!(!text.contains("initial-content-composed"), "{text:?}");
}

#[test]
fn a_window_title_read_recomposes_without_a_manual_refresh() {
    let mut fixture = Fixture::spawn("reactive-title", "reactive-title");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(status.is_some(), "the reactive-title fixture should finish");
    assert!(
        fixture.text().contains("reactive-title=reactive title"),
        "{:?}",
        fixture.text()
    );
}

#[test]
fn a_window_size_read_recomposes_without_a_manual_refresh() {
    let mut fixture = Fixture::spawn("reactive-size", "reactive-size");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(status.is_some(), "the reactive-size fixture should finish");
    assert!(
        fixture.text().contains("reactive-size=(560.0, 360.0)"),
        "{:?}",
        fixture.text()
    );
}

#[test]
fn an_existing_windows_command_runs_before_a_new_roots_first_frame() {
    let mut fixture = Fixture::spawn("existing-command-new-root", "existing-command-new-root");
    let status = fixture.wait(Duration::from_secs(10));
    assert!(status.is_some(), "the ordering fixture should finish");
    let text = fixture.text();
    let closed = text.find("first-closed");
    let composed = text.find("second-composed");
    assert!(closed.is_some() && composed.is_some(), "{text:?}");
    assert!(closed.unwrap() < composed.unwrap(), "{text:?}");
}

#[test]
fn destroying_before_first_content_does_not_clear_persisted_state() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("persist-initial-destroy");
    let out = dir.join("out.txt");
    let mut port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);
    let mut run = |mode: &str| {
        let _ = std::fs::remove_file(&out);
        let mut child = Command::new(fixture_exe())
            .arg("window_lifecycle")
            .env("WINDOW_FIXTURE_MODE", mode)
            .env("WINDOW_FIXTURE_OUT", &out)
            .env("WINDOW_FIXTURE_STATE", dir.join("state"))
            .env("WINIA_DEBUG_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn persist-initial-destroy run");
        port += 1;
        let status = child.wait().expect("persist-initial-destroy run");
        assert!(status.success(), "{mode} failed: {status:?}");
        std::fs::read_to_string(&out).unwrap_or_default()
    };

    assert!(run("persist").contains("counter=0"));
    let destroyed = run("persist-initial-destroy");
    assert!(destroyed.contains("initial-on-close"), "{destroyed:?}");
    assert!(
        !destroyed.contains("initial-content-composed"),
        "{destroyed:?}"
    );
    assert!(
        run("persist").contains("counter=1"),
        "pre-content destruction published an empty snapshot"
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
