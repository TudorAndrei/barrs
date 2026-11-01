//! Cucumber features for the running daemon (`tests/features/daemon`).
//!
//! Each scenario starts the `barrs` binary with the no-op renderer and its
//! own config and socket, and drives it with the `barrs` commands.
#![expect(
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    reason = "cucumber steps take the World as `&mut` and their captures by value"
)]

use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context as _, Result, bail, ensure};
use cucumber::{World as _, given, then, when};
use serde_json::Value;
use tempfile::TempDir;

const BARRS: &str = env!("CARGO_BIN_EXE_barrs");
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const POLL_ATTEMPTS: u32 = 200;

/// One scenario: a daemon process, its directory, and the last results.
#[derive(Debug, cucumber::World)]
#[world(init = Self::new)]
struct DaemonWorld {
    dir: TempDir,
    daemon: Option<Child>,
    command: Option<Output>,
    reply: Option<Value>,
}

impl Drop for DaemonWorld {
    fn drop(&mut self) {
        if let Some(daemon) = &mut self.daemon {
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
    }
}

impl DaemonWorld {
    fn new() -> std::io::Result<Self> {
        Ok(Self {
            dir: tempfile::tempdir()?,
            daemon: None,
            command: None,
            reply: None,
        })
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("barrs.lua")
    }

    fn socket_path(&self) -> PathBuf {
        self.dir.path().join("barrs.sock")
    }

    /// The file that the clock handlers of the fixture configs write.
    fn handler_record_path(&self) -> PathBuf {
        self.dir.path().join("handler.txt")
    }

    /// Writes `config` and starts a daemon for it. Returns when the daemon
    /// reports that its socket accepts requests.
    fn start_daemon(&mut self, config: &str) -> Result<()> {
        fs::write(self.config_path(), config)?;
        let mut daemon = Command::new(BARRS)
            .arg("run")
            .args(["--renderer", "noop"])
            .arg("--config")
            .arg(self.config_path())
            .arg("--socket")
            .arg(self.socket_path())
            .env("BARRS_STARTUP_READY", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(self.dir.path().join("daemon.log"))?)
            .spawn()?;
        let stdout = daemon.stdout.take().context("daemon stdout")?;
        self.daemon = Some(daemon);

        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let line = receiver
            .recv_timeout(STARTUP_TIMEOUT)
            .context("the daemon did not report readiness")??;
        ensure!(
            line.trim_end() == "barrs-ready",
            "the daemon reported {line:?} instead of readiness"
        );
        Ok(())
    }

    /// Runs a `barrs` command line against the daemon of this scenario.
    fn run(&self, command_line: &str) -> Result<Output> {
        let mut words = command_line.split_whitespace();
        ensure!(
            words.next() == Some("barrs"),
            "{command_line:?} is not a barrs command"
        );
        Ok(Command::new(BARRS)
            .args(words)
            .arg("--socket")
            .arg(self.socket_path())
            .stdin(Stdio::null())
            .output()?)
    }

    fn last_command(&self) -> Result<&Output> {
        self.command.as_ref().context("no command has run")
    }

    /// The item states that `barrs dump-state` prints.
    fn item_state(&self, id: &str) -> Result<Value> {
        let output = self.run("barrs dump-state")?;
        ensure!(output.status.success(), "dump-state failed: {output:?}");
        let mut states: Value = serde_json::from_slice(&output.stdout)?;
        states
            .get_mut(id)
            .map(Value::take)
            .with_context(|| format!("the daemon shows no item {id}"))
    }

    fn wait_for_item_text(&self, id: &str, text: &str) -> Result<()> {
        let mut state = Value::Null;
        for _ in 0..POLL_ATTEMPTS {
            state = self.item_state(id)?;
            if state.get("text").and_then(Value::as_str) == Some(text) {
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        bail!("item {id} did not show {text:?}; it shows {state}")
    }

    fn item_data(&self, id: &str, field: &str) -> Result<Value> {
        let mut state = self.item_state(id)?;
        state
            .get_mut("data")
            .and_then(|data| data.get_mut(field))
            .map(Value::take)
            .with_context(|| format!("item {id} has no data field {field}: {state}"))
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

#[given("a running daemon with a clock item that records its clicks")]
fn daemon_recording_clicks(world: &mut DaemonWorld) -> Result<()> {
    let config = format!(
        r#"
function handle_click(ctx)
  local file = io.open([=[{}]=], "w")
  file:write(ctx.item_id .. ":" .. ctx.event)
  file:close()
end

return {{
  items = {{ {{ id = "clock", label = "clock", handlers = {{ click = "handle_click" }} }} }},
}}
"#,
        world.handler_record_path().display()
    );
    world.start_daemon(&config)
}

#[given("a running daemon with a clock item that counts its clicks")]
fn daemon_counting_clicks(world: &mut DaemonWorld) -> Result<()> {
    let config = format!(
        r#"
counter = 0

function handle_click(ctx)
  counter = counter + 1
  local file = io.open([=[{}]=], "w")
  file:write(tostring(counter))
  file:close()
end

return {{
  items = {{ {{ id = "clock", label = "clock", handlers = {{ click = "handle_click" }} }} }},
}}
"#,
        world.handler_record_path().display()
    );
    world.start_daemon(&config)
}

#[given("a running daemon with Lua update items")]
fn daemon_with_lua_updates(world: &mut DaemonWorld) -> Result<()> {
    world.start_daemon(
        r#"
calls = 0
function usage(ctx)
  calls = calls + 1
  return { text = ctx.item_id .. ":" .. calls, used = 42 }
end
function broken(ctx) error("boom") end
function noop(ctx) end

return {
  items = {
    { id = "memory", interval = 3600, plugin = { kind = "lua", update = "usage" },
      handlers = { click = "noop" } },
    { id = "broken", interval = 3600, plugin = { kind = "lua", update = "broken" } },
  },
}
"#,
    )
}

#[given(expr = "the config file now contains {string}")]
fn the_config_file_now_contains(world: &mut DaemonWorld, text: String) -> Result<()> {
    fs::write(world.config_path(), text)?;
    Ok(())
}

#[given(expr = "item {string} shows {string}")]
fn given_item_shows(world: &mut DaemonWorld, id: String, text: String) -> Result<()> {
    world.wait_for_item_text(&id, &text)
}

#[when(expr = "I run {string}")]
fn i_run(world: &mut DaemonWorld, command_line: String) -> Result<()> {
    world.command = Some(world.run(&command_line)?);
    Ok(())
}

#[when(expr = "a client sends the request line {string}")]
fn a_client_sends(world: &mut DaemonWorld, line: String) -> Result<()> {
    let mut stream = UnixStream::connect(world.socket_path())?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    world.reply = Some(serde_json::from_str(&reply)?);
    Ok(())
}

#[then(expr = "the command prints {string}")]
fn the_command_prints(world: &mut DaemonWorld, expected: String) -> Result<()> {
    let output = world.last_command()?;
    ensure!(output.status.success(), "the command failed: {output:?}");
    let actual = stdout(output);
    ensure!(actual == expected, "the command printed {actual:?}");
    Ok(())
}

#[then("the command succeeds")]
fn the_command_succeeds(world: &mut DaemonWorld) -> Result<()> {
    let output = world.last_command()?;
    ensure!(output.status.success(), "the command failed: {output:?}");
    Ok(())
}

#[then("the command fails")]
fn the_command_fails(world: &mut DaemonWorld) -> Result<()> {
    let output = world.last_command()?;
    ensure!(
        !output.status.success(),
        "the command succeeded: {output:?}"
    );
    Ok(())
}

#[then(expr = "the command fails with {string}")]
fn the_command_fails_with(world: &mut DaemonWorld, message: String) -> Result<()> {
    let output = world.last_command()?;
    ensure!(
        !output.status.success(),
        "the command succeeded: {output:?}"
    );
    let error = stderr(output);
    ensure!(
        error.contains(&message),
        "the command failed with {error:?}"
    );
    Ok(())
}

#[then("the daemon exits")]
fn the_daemon_exits(world: &mut DaemonWorld) -> Result<()> {
    let daemon = world.daemon.as_mut().context("no daemon is running")?;
    for _ in 0..POLL_ATTEMPTS {
        if let Some(status) = daemon.try_wait()? {
            ensure!(status.success(), "the daemon exited with {status}");
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
    bail!("the daemon is still running")
}

#[then("the daemon answers with an error")]
fn the_daemon_answers_with_an_error(world: &mut DaemonWorld) -> Result<()> {
    let reply = world.reply.as_ref().context("no request was sent")?;
    ensure!(
        reply.get("status").and_then(Value::as_str) == Some("error"),
        "the daemon answered {reply}"
    );
    Ok(())
}

#[then("the daemon answers a ping")]
fn the_daemon_answers_a_ping(world: &mut DaemonWorld) -> Result<()> {
    let output = world.run("barrs ping")?;
    ensure!(
        output.status.success() && stdout(&output) == "pong",
        "the ping failed: {output:?}"
    );
    Ok(())
}

#[then(expr = "the clock handler recorded {string}")]
fn the_clock_handler_recorded(world: &mut DaemonWorld, expected: String) -> Result<()> {
    let actual = fs::read_to_string(world.handler_record_path())
        .context("the clock handler recorded nothing")?;
    ensure!(actual == expected, "the clock handler recorded {actual:?}");
    Ok(())
}

#[then("the clock handler recorded nothing")]
fn the_clock_handler_recorded_nothing(world: &mut DaemonWorld) -> Result<()> {
    ensure!(
        !world.handler_record_path().exists(),
        "the clock handler ran"
    );
    Ok(())
}

#[then(expr = "item {string} shows {string}")]
fn then_item_shows(world: &mut DaemonWorld, id: String, text: String) -> Result<()> {
    world.wait_for_item_text(&id, &text)
}

#[then(expr = "item {string} has the data field {string} set to {int}")]
fn item_has_data_field(
    world: &mut DaemonWorld,
    id: String,
    field: String,
    expected: i64,
) -> Result<()> {
    let actual = world.item_data(&id, &field)?;
    ensure!(
        actual.as_i64() == Some(expected),
        "item {id} has {field} = {actual}"
    );
    Ok(())
}

#[then(expr = "item {string} is unavailable")]
fn item_is_unavailable(world: &mut DaemonWorld, id: String) -> Result<()> {
    let available = world.item_data(&id, "available")?;
    ensure!(
        available == Value::Bool(false),
        "item {id} has available = {available}"
    );
    Ok(())
}

fn main() {
    // cargo-nextest lists tests with `--list`; Cucumber does not support it.
    if std::env::args().any(|arg| arg == "--list") {
        return;
    }
    futures::executor::block_on(
        DaemonWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/features/daemon"
            )),
    );
}
