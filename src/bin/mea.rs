use clap::Parser;
use mea::cli::Cli;

fn main() {
    let output = mea::app::run(Cli::parse());
    println!("{output}");
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output)
        && parsed.get("status").and_then(|status| status.as_str()) == Some("error")
    {
        std::process::exit(1);
    }
}
