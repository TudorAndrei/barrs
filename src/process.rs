use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn run_command_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::run_command_with_timeout;

    #[cfg(unix)]
    #[test]
    fn returns_none_when_command_exceeds_timeout() {
        let started = Instant::now();
        let output = run_command_with_timeout("sleep", &["2"], Duration::from_millis(100));

        assert!(output.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn captures_successful_command_output() {
        let output = run_command_with_timeout("printf", &["{\"ok\":true}"], Duration::from_secs(1))
            .expect("output");

        assert!(output.status.success());
        assert_eq!(output.stdout, br#"{"ok":true}"#);
    }
}
