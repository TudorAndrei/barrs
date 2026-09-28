use barrs::app;
use barrs::cli::Cli;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), barrs::error::BarrsError> {
    let cli = Cli::parse();
    app::run(cli).await
}
