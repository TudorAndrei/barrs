use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::render::RendererKind;

#[derive(Debug, Parser)]
#[command(name = "barrs", version, about = "A low-overhead macOS bar daemon")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Start(StartArgs),
    Stop(SocketArgs),
    Reload(SocketArgs),
    Status(SocketArgs),
    Ping(SocketArgs),
    ValidateConfig(ConfigArgs),
    DumpState(SocketArgs),
    Rift(RiftArgs),
    Item(ItemArgs),
}

#[derive(Debug, Clone, Args)]
pub struct SocketArgs {
    #[arg(long)]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[arg(long, default_value = "barrs.lua")]
    pub config: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct StartArgs {
    #[arg(long, default_value = "barrs.lua")]
    pub config: PathBuf,
    #[arg(long)]
    pub socket: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = RendererKind::Noop)]
    pub renderer: RendererKind,
}

#[derive(Debug, Clone, Args)]
pub struct ItemArgs {
    #[command(subcommand)]
    pub command: ItemCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ItemCommand {
    Trigger(TriggerArgs),
}

#[derive(Debug, Clone, Args)]
pub struct TriggerArgs {
    pub item_id: String,
    pub event: TriggerEvent,
    #[arg(long)]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TriggerEvent {
    Click,
    RightClick,
    Scroll,
    HoverEnter,
    HoverLeave,
    HoverUpdate,
}

#[derive(Debug, Clone, Args)]
pub struct RiftArgs {
    #[command(subcommand)]
    pub command: RiftCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RiftCommand {
    Backend(SocketArgs),
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command, ItemCommand, RiftCommand, TriggerEvent};
    use crate::render::RendererKind;

    #[test]
    fn parses_item_trigger() {
        let cli = Cli::parse_from(["barrs", "item", "trigger", "cpu", "hover-enter"]);
        match cli.command {
            Command::Item(item) => match item.command {
                ItemCommand::Trigger(args) => {
                    assert_eq!(args.item_id, "cpu");
                    assert_eq!(args.event, TriggerEvent::HoverEnter);
                }
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_rift_backend() {
        let cli = Cli::parse_from(["barrs", "rift", "backend"]);
        match cli.command {
            Command::Rift(args) => match args.command {
                RiftCommand::Backend(_) => {}
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_renderer_selection() {
        let cli = Cli::parse_from(["barrs", "start", "--renderer", "noop"]);
        match cli.command {
            Command::Start(args) => assert_eq!(args.renderer, RendererKind::Noop),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
