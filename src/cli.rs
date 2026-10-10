use std::path::PathBuf;

use usage_rs::{Args, Cli as UsageCli, Subcommands, ValueEnum};

use crate::render::RendererKind;

/// A low-overhead macOS bar daemon
#[derive(Debug, UsageCli)]
#[usage(bin = "barrs", version)]
pub struct Cli {
    #[usage(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommands)]
pub enum Command {
    Start(StartArgs),
    #[usage(hide = true)]
    Run(RunArgs),
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
    #[usage(long)]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[usage(long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct StartArgs {
    #[usage(long)]
    pub config: Option<PathBuf>,
    #[usage(long)]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    #[usage(long)]
    pub config: Option<PathBuf>,
    #[usage(long)]
    pub socket: Option<PathBuf>,
    #[usage(long, value_enum, default = "native")]
    pub renderer: RendererKind,
}

#[derive(Debug, Clone, Args)]
pub struct ItemArgs {
    #[usage(subcommand)]
    pub command: ItemCommand,
}

#[derive(Debug, Clone, Subcommands)]
pub enum ItemCommand {
    Trigger(TriggerArgs),
}

#[derive(Debug, Clone, Args)]
pub struct TriggerArgs {
    pub item_id: String,
    #[usage(value_enum)]
    pub event: TriggerEvent,
    #[usage(long)]
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
    #[usage(subcommand)]
    pub command: RiftCommand,
}

#[derive(Debug, Clone, Subcommands)]
pub enum RiftCommand {
    Backend(SocketArgs),
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{Cli, Command, ItemCommand, RiftCommand, TriggerEvent};
    use crate::render::RendererKind;

    fn parse(words: &[&str]) -> Cli {
        let argv: Vec<&OsStr> = words.iter().map(OsStr::new).collect();
        Cli::parse_from(&argv).expect("valid command line")
    }

    #[test]
    fn parses_item_trigger() {
        let cli = parse(&["item", "trigger", "cpu", "hover-enter"]);
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
        let cli = parse(&["rift", "backend"]);
        match cli.command {
            Command::Rift(args) => match args.command {
                RiftCommand::Backend(_) => {}
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_renderer_selection() {
        let cli = parse(&["run", "--renderer", "noop"]);
        match cli.command {
            Command::Run(args) => assert_eq!(args.renderer, RendererKind::Noop),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn defaults_renderer_to_native() {
        let cli = parse(&["run"]);
        match cli.command {
            Command::Run(args) => assert_eq!(args.renderer, RendererKind::Native),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
