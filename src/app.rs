use std::path::PathBuf;

use crate::cli::{Cli, Command, RiftCommand};
use crate::daemon::Daemon;
use crate::error::BarrsError;
use crate::ipc::{EventPayload, Request, Response, default_socket_path, send_request};
use crate::render::create_renderer;
use crate::{config, rift};

pub async fn run(cli: Cli) -> Result<(), BarrsError> {
    match cli.command {
        Command::Start(args) => {
            let config = config::load_config(&args.config)?;
            let daemon = Daemon::new(
                args.config,
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
            let config = config::load_config(&args.config)?;
            let response = Response::Ok {
                message: format!(
                    "validated {} with {} item(s)",
                    args.config.display(),
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
