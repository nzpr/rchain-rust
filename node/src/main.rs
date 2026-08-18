//! The node entry point (port of `Main.scala` + `NodeMain.startNode`).

use clap::Parser;

use rchain_node::configuration::commandline::options::Options;
use rchain_node::configuration::configuration::Configuration;
use rchain_node::runtime::{node_environment, node_runtime};

#[tokio::main]
async fn main() {
    let options = Options::parse();

    let (node_conf, _profile, _config_file) = match Configuration::build(&options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            std::process::exit(1);
        }
    };

    let id = match node_environment::create(&node_conf) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Initialization error: {e}");
            std::process::exit(1);
        }
    };

    let program = match node_runtime::setup(&node_conf, &id).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Setup error: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = program.serve().await {
        eprintln!("Server error: {e}");
        std::process::exit(1);
    }
}
