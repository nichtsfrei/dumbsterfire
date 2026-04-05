mod checksum;
mod cli;
mod config;
mod email;
mod filter;
mod imap;
mod labels;
mod matcher;
mod models;

mod error;

use crate::cli::Cli;
use anyhow::Result;
use clap::Parser;

use crate::config::{Config, default_label_dir, default_output_dir};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        for cause in e.chain() {
            eprintln!("  Caused by: {cause}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli {
        Cli::Download(args) => {
            let config: Config = args.try_into()?;
            imap::download(&config)
        }
        Cli::Label(args) => {
            // Use provided paths or fall back to defaults
            let output_dir = args.output_dir.unwrap_or_else(default_output_dir);
            let label_dir = args.label_dir.unwrap_or_else(default_label_dir);
            labels::process_emails(&output_dir, &label_dir, args.extract)
        }
        Cli::Email(args) => email::extract_email(&args.path),
    }
}
