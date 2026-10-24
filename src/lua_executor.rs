//! The thread that owns the Lua runtime of one config generation.
//!
//! The executor thread evaluates the config itself and keeps the resulting
//! [`Lua`] for its whole life. The daemon talks to it only with serde values, so
//! no Lua value crosses a thread boundary, and the single thread serializes
//! every Lua call.

use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mlua::{FromLuaMulti, Function, Lua, LuaSerdeExt};
use serde_json::{Value, json};

use crate::config::{Config, load_config_with_runtime};
use crate::content::ItemContent;
use crate::error::BarrsError;

/// Longest time one Lua event handler may run before barrs stops it.
const HANDLER_TIMEOUT: Duration = Duration::from_secs(1);
/// Longest time one Lua update function may run before barrs stops it.
const UPDATE_TIMEOUT: Duration = Duration::from_secs(5);
/// Number of Lua VM instructions between two deadline checks.
const DEADLINE_CHECK_INTERVAL: u32 = 10_000;
/// Largest update result that barrs accepts, measured as serialized JSON.
const MAX_UPDATE_RESULT_BYTES: usize = 16 * 1024;

/// Receives the result of one Lua call on the executor thread.
type Done<V> = Box<dyn FnOnce(Result<V, BarrsError>) + Send>;

enum Job {
    Handler {
        function: String,
        context: Value,
        done: Done<()>,
    },
    Update {
        item_id: String,
        function: String,
        done: Done<ItemContent>,
    },
}

/// Handle to the thread that owns one Lua runtime.
///
/// Dropping every handle stops the thread after the calls already queued.
#[derive(Clone)]
pub struct LuaExecutor {
    jobs: std_mpsc::Sender<Job>,
}

impl LuaExecutor {
    /// Starts a Lua thread that evaluates the config at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread cannot start, or when the config cannot
    /// be read, evaluated, or validated.
    pub fn start(path: &Path) -> Result<(Self, Config), BarrsError> {
        let (jobs, job_receiver) = std_mpsc::channel();
        let (loaded_sender, loaded_receiver) = std_mpsc::sync_channel(1);
        let path = path.to_path_buf();
        thread::Builder::new()
            .name("barrs-lua".into())
            .spawn(move || {
                let lua = match load_config_with_runtime(&path) {
                    Ok((config, lua)) => {
                        if loaded_sender.send(Ok(config)).is_err() {
                            return;
                        }
                        lua
                    }
                    Err(error) => {
                        let _ = loaded_sender.send(Err(error));
                        return;
                    }
                };
                serve(&lua, &job_receiver);
            })?;
        let config = loaded_receiver.recv().map_err(|_| executor_stopped())??;
        Ok((Self { jobs }, config))
    }

    /// Queues a call of the global Lua function `function` with `context` as
    /// its argument.
    ///
    /// `done` gets the result exactly once: on the executor thread when the
    /// call ends, or at once with an error when the executor has stopped. The
    /// call fails when the function is missing, raises an error, or runs
    /// longer than the handler time limit.
    pub fn call_handler(
        &self,
        function: &str,
        context: Value,
        done: impl FnOnce(Result<(), BarrsError>) + Send + 'static,
    ) {
        self.submit(Job::Handler {
            function: function.to_owned(),
            context,
            done: Box::new(done),
        });
    }

    /// Queues a call of the update function `function` for `item_id`.
    ///
    /// `done` gets the item content, or the reason the call failed, exactly
    /// once: on the executor thread when the call ends, or at once when the
    /// executor has stopped.
    pub fn request_update(
        &self,
        item_id: &str,
        function: &str,
        done: impl FnOnce(Result<ItemContent, BarrsError>) + Send + 'static,
    ) {
        self.submit(Job::Update {
            item_id: item_id.to_owned(),
            function: function.to_owned(),
            done: Box::new(done),
        });
    }

    fn submit(&self, job: Job) {
        if let Err(std_mpsc::SendError(job)) = self.jobs.send(job) {
            match job {
                Job::Handler { done, .. } => done(Err(executor_stopped())),
                Job::Update { done, .. } => done(Err(executor_stopped())),
            }
        }
    }
}

fn executor_stopped() -> BarrsError {
    BarrsError::Lua("lua executor stopped".into())
}

fn serve(lua: &Lua, jobs: &std_mpsc::Receiver<Job>) {
    for job in jobs {
        match job {
            Job::Handler {
                function,
                context,
                done,
            } => done(run_handler(lua, &function, &context)),
            Job::Update {
                item_id,
                function,
                done,
            } => done(run_update(lua, &function, &item_id)),
        }
    }
}

fn run_handler(lua: &Lua, function: &str, context: &Value) -> Result<(), BarrsError> {
    let func: Function = lua
        .globals()
        .get(function)
        .map_err(|_| BarrsError::InvalidConfig(format!("missing handler {function}")))?;
    let context = lua.to_value(context)?;
    call_with_timeout::<()>(lua, &func, context, HANDLER_TIMEOUT)
}

fn run_update(lua: &Lua, function: &str, item_id: &str) -> Result<ItemContent, BarrsError> {
    let func: Function = lua
        .globals()
        .get(function)
        .map_err(|_| BarrsError::InvalidConfig(format!("missing update function {function}")))?;
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        });
    let context = lua.to_value(&json!({ "item_id": item_id, "timestamp_ms": timestamp_ms }))?;
    let returned = call_with_timeout::<mlua::Value>(lua, &func, context, UPDATE_TIMEOUT)?;
    update_content(lua, returned)
}

/// Converts the value an update function returned into text content.
///
/// A string becomes the text. A table must serialize to a JSON object with a
/// string `text`; its other fields stay in the item data. The result is always
/// text content, so a table field such as `workspaces` does not change the
/// layout.
fn update_content(lua: &Lua, returned: mlua::Value) -> Result<ItemContent, BarrsError> {
    let data = match returned {
        mlua::Value::String(text) => json!({ "text": &*text.to_str()? }),
        mlua::Value::Table(_) => lua.from_value::<Value>(returned)?,
        other => {
            return Err(BarrsError::Lua(format!(
                "update function returned a {}, expected a string or a table",
                other.type_name()
            )));
        }
    };
    let size = serde_json::to_vec(&data)?.len();
    let Value::Object(mut fields) = data else {
        return Err(missing_text());
    };
    let Some(Value::String(text)) = fields.remove("text") else {
        return Err(missing_text());
    };
    if size > MAX_UPDATE_RESULT_BYTES {
        return Err(BarrsError::Lua(format!(
            "update result is {size} bytes, more than the {MAX_UPDATE_RESULT_BYTES} byte limit"
        )));
    }
    Ok(ItemContent::Text { text, fields })
}

fn missing_text() -> BarrsError {
    BarrsError::Lua("update result must be a table with a string text field".into())
}

/// Calls `func` and stops it with an error if it runs longer than `timeout`.
///
/// The hook counts VM instructions, so it cannot stop a call that is blocked
/// inside one C function such as `os.execute`.
fn call_with_timeout<R: FromLuaMulti>(
    lua: &Lua,
    func: &Function,
    argument: mlua::Value,
    timeout: Duration,
) -> Result<R, BarrsError> {
    if let Some(deadline) = Instant::now().checked_add(timeout) {
        lua.set_global_hook(
            mlua::HookTriggers::new().every_nth_instruction(DEADLINE_CHECK_INTERVAL),
            move |_, _| {
                if Instant::now() >= deadline {
                    Err(mlua::Error::runtime(format!(
                        "call exceeded the {} ms time limit",
                        timeout.as_millis()
                    )))
                } else {
                    Ok(mlua::VmState::Continue)
                }
            },
        )?;
    }
    let result = func.call::<R>(argument);
    lua.remove_global_hook();
    result.map_err(BarrsError::from)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{LuaExecutor, call_with_timeout, update_content};
    use crate::content::ItemContent;
    use crate::error::BarrsError;

    /// Runs the handler `function` and waits for its result.
    async fn handle(
        executor: &LuaExecutor,
        function: &str,
        context: serde_json::Value,
    ) -> Result<(), BarrsError> {
        let (done, result) = tokio::sync::oneshot::channel();
        executor.call_handler(function, context, move |outcome| {
            let _ = done.send(outcome);
        });
        result.await.expect("handler result")
    }

    /// Runs the update function `function` for `item_id` and waits for its result.
    async fn update(
        executor: &LuaExecutor,
        item_id: &str,
        function: &str,
    ) -> Result<ItemContent, BarrsError> {
        let (done, result) = tokio::sync::oneshot::channel();
        executor.request_update(item_id, function, move |outcome| {
            let _ = done.send(outcome);
        });
        result.await.expect("update result")
    }

    #[test]
    fn call_that_never_returns_is_stopped() {
        let lua = mlua::Lua::new();
        lua.load("function spin() while true do end end\nfunction ok() return 1 end")
            .exec()
            .expect("load functions");
        let spin: mlua::Function = lua.globals().get("spin").expect("spin");
        let started = std::time::Instant::now();
        let error =
            call_with_timeout::<()>(&lua, &spin, mlua::Value::Nil, Duration::from_millis(50))
                .expect_err("spin must time out");
        assert!(error.to_string().contains("time limit"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));

        let ok: mlua::Function = lua.globals().get("ok").expect("ok");
        call_with_timeout::<()>(&lua, &ok, mlua::Value::Nil, Duration::from_millis(50))
            .expect("function runs after a timeout");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn handlers_share_globals_within_one_executor() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
clicks = 0
function count(ctx) clicks = clicks + ctx.step end
function check(ctx) assert(clicks == ctx.expected, "clicks = " .. clicks) end
return { items = { { id = "clock", handlers = { click = "count" } } } }
"#,
        )
        .expect("write config");

        let (executor, config) = LuaExecutor::start(&path).expect("start executor");
        assert_eq!(config.items.len(), 1);
        handle(&executor, "count", json!({ "step": 2 }))
            .await
            .expect("count");
        handle(&executor, "count", json!({ "step": 3 }))
            .await
            .expect("count");
        handle(&executor, "check", json!({ "expected": 5 }))
            .await
            .expect("globals persist between calls");
    }

    fn update_content_for(lua_value: &str) -> Result<ItemContent, crate::error::BarrsError> {
        let lua = mlua::Lua::new();
        let value: mlua::Value = lua.load(lua_value).eval().expect("evaluate value");
        update_content(&lua, value)
    }

    #[test]
    fn update_content_accepts_strings_and_tables_with_text() {
        assert_eq!(
            update_content_for(r#""42%""#).expect("string"),
            ItemContent::plain("42%")
        );
        assert_eq!(
            update_content_for(r#"{ text = "42%", icon = "M", used = 42 }"#).expect("table"),
            ItemContent::plain_with_fields("42%", [("icon", json!("M")), ("used", json!(42))])
        );
    }

    #[test]
    fn update_content_is_text_even_with_a_workspaces_field() {
        let content = update_content_for(r#"{ text = "1 2", workspaces = { { name = "1" } } }"#)
            .expect("table");
        assert_eq!(
            content,
            ItemContent::plain_with_fields("1 2", [("workspaces", json!([{ "name": "1" }]))])
        );
    }

    #[test]
    fn update_data_rejects_invalid_results() {
        let cases = [
            ("nil", "returned a nil"),
            ("42", "returned a integer"),
            (r#"{ icon = "M" }"#, "string text field"),
            ("{ text = 42 }", "string text field"),
            ("{ 1, 2 }", "string text field"),
            (r#"{ text = string.rep("x", 20000) }"#, "byte limit"),
        ];
        for (value, expected) in cases {
            let error = update_content_for(value).expect_err(value);
            assert!(error.to_string().contains(expected), "{value}: {error}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_results_carry_the_item_context() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
function usage(ctx) return { text = ctx.item_id, stamped = ctx.timestamp_ms > 0 } end
function broken(ctx) error("boom") end
return { items = { { id = "memory", plugin = { kind = "lua", update = "usage" } } } }
"#,
        )
        .expect("write config");
        let (executor, _) = LuaExecutor::start(&path).expect("start executor");

        assert_eq!(
            update(&executor, "memory", "usage").await.expect("outcome"),
            ItemContent::plain_with_fields("memory", [("stamped", json!(true))])
        );
        let error = update(&executor, "memory", "broken")
            .await
            .expect_err("broken update must fail");
        assert!(error.to_string().contains("boom"), "{error}");
    }

    #[test]
    fn start_reports_config_errors() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(&path, "return { items = {} }").expect("write config");

        let error = LuaExecutor::start(&path)
            .err()
            .expect("empty config must fail");
        assert!(error.to_string().contains("at least one item"), "{error}");
    }
}
