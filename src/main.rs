use barrs::app;
use barrs::cli::Cli;
use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), barrs::error::BarrsError> {
    let cli = Cli::parse();
    app::run(cli).await
}
