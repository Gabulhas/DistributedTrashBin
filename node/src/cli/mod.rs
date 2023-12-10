use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "DTB Node")]
#[command(version = "1.0")]
#[command(about = "Does some trash distribution", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// does testing things
    GenerateKey {
        #[arg(short, long, value_name = "FILE")]
        key_file: PathBuf,
    },
    Start {
        #[arg(short, long, value_name = "FILE")]
        key_file: Option<PathBuf>,

        // Optional name to operate on
        bootstrap_nodes: Option<Vec<String>>,
    },
}
