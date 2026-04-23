use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::cli::{Cli, Command, RiftCommand, RunArgs};
use crate::daemon::Daemon;
use crate::error::BarrsError;
use crate::ipc::{EventPayload, Request, Response, default_socket_path, send_request};
use crate::render::create_renderer;
use crate::{config, rift};

pub async fn run(cli: Cli) -> Result<(), BarrsError> {
    match cli.command {
        Command::Start(args) => {
            let config_path = resolve_config_path(args.config);
            ensure_config_exists(&config_path)?;
            if cfg!(debug_assertions) {
                let config = config::load_config(&config_path)?;
                let daemon = Daemon::new(
                    config_path,
                    apply_socket_override(config, args.socket),
                    create_renderer(crate::render::RendererKind::Native)?,
                )?;
                return daemon.run().await;
            }
            spawn_background_process(RunArgs {
                config: Some(config_path.clone()),
                socket: args.socket,
                renderer: crate::render::RendererKind::Native,
            })?;
            print_response(Response::Ok {
                message: format!("started barrs with {}", config_path.display()),
            });
            Ok(())
        }
        Command::Run(args) => {
            let config_path = resolve_config_path(args.config);
            ensure_config_exists(&config_path)?;
            let config = config::load_config(&config_path)?;
            let daemon = Daemon::new(
                config_path,
                apply_socket_override(config, args.socket),
                create_renderer(args.renderer)?,
            )?;
            daemon.run().await
        }
        Command::Stop(args) => {
            print_response(send_request(&socket_or_default(args.socket), &Request::Stop).await?);
            Ok(())
        }
        Command::Reload(args) => {
            print_response(send_request(&socket_or_default(args.socket), &Request::Reload).await?);
            Ok(())
        }
        Command::Status(args) => {
            print_response(send_request(&socket_or_default(args.socket), &Request::Status).await?);
            Ok(())
        }
        Command::Ping(args) => {
            print_response(send_request(&socket_or_default(args.socket), &Request::Ping).await?);
            Ok(())
        }
        Command::ValidateConfig(args) => {
            let config_path = resolve_config_path(args.config);
            let config = config::load_config(&config_path)?;
            let response = Response::Ok {
                message: format!(
                    "validated {} with {} item(s)",
                    config_path.display(),
                    config.items.len()
                ),
            };
            print_response(response);
            Ok(())
        }
        Command::DumpState(args) => {
            print_response(
                send_request(&socket_or_default(args.socket), &Request::DumpState).await?,
            );
            Ok(())
        }
        Command::Rift(args) => match args.command {
            RiftCommand::Backend(socket) => {
                if socket.socket.is_some() {
                    print_response(
                        send_request(&socket_or_default(socket.socket), &Request::RiftBackend)
                            .await?,
                    );
                } else {
                    print_response(Response::RiftBackend {
                        backend: rift::select_backend().kind(),
                    });
                }
                Ok(())
            }
        },
        Command::Item(item) => match item.command {
            crate::cli::ItemCommand::Trigger(args) => {
                let payload = EventPayload::from_trigger(args.item_id, args.event);
                print_response(
                    send_request(
                        &socket_or_default(args.socket),
                        &Request::TriggerItem { payload },
                    )
                    .await?,
                );
                Ok(())
            }
        },
    }
}

fn resolve_config_path(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(default_config_path)
}

fn default_config_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("barrs")
        .join("barrs.lua")
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn ensure_config_exists(path: &Path) -> Result<(), BarrsError> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, include_str!("../barrs.lua"))?;
    Ok(())
}

fn spawn_background_process(args: RunArgs) -> Result<(), BarrsError> {
    let current_exe = env::current_exe()?;
    let mut command = ProcessCommand::new(current_exe);
    command
        .arg("run")
        .arg("--renderer")
        .arg(match args.renderer {
            crate::render::RendererKind::Native => "native",
            crate::render::RendererKind::Noop => "noop",
        })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(config) = args.config {
        command.arg("--config").arg(config);
    }
    if let Some(socket) = args.socket {
        command.arg("--socket").arg(socket);
    }
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn()?;
    Ok(())
}

fn socket_or_default(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(default_socket_path)
}

fn apply_socket_override(
    mut config: config::Config,
    socket_path: Option<PathBuf>,
) -> config::Config {
    if socket_path.is_some() {
        config.socket_path = socket_path;
    }
    config
}

fn print_response(response: Response) {
    match response {
        Response::Pong => println!("pong"),
        Response::Ok { message } => println!("{message}"),
        Response::Status {
            running,
            items,
            backend,
            config_path,
        } => {
            println!(
                "running={running} items={items} backend={} config={}",
                serde_json::to_string(&backend).unwrap_or_else(|_| "\"unknown\"".into()),
                config_path.display()
            );
        }
        Response::State(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
        ),
        Response::RiftBackend { backend } => {
            println!(
                "{}",
                serde_json::to_string(&backend).unwrap_or_else(|_| "\"unknown\"".into())
            )
        }
        Response::Error { message } => eprintln!("{message}"),
    }
}
