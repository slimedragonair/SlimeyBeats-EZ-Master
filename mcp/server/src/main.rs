use clap::Parser;
use rmcp::ServiceExt;
use slimeybeats_mcp::{arguments::Arguments, protocol::Companion, state::Engine};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arguments::parse();
    if args.print_config {
        println!("{}", serde_json::to_string_pretty(&args.client_config()?)?);
        return Ok(());
    }
    let engine = Engine::new(&args)?;
    let result = async {
        let service = Companion::new(engine.clone())
            .serve(rmcp::transport::stdio())
            .await?;
        service.waiting().await?;
        anyhow::Ok(())
    }
    .await;
    engine.shutdown();
    result
}
