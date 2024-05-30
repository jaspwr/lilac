use std::{collections::HashMap, path::PathBuf, sync::atomic::AtomicUsize, usize};

use lilac::css::StyleSheet;
use lilac::job::Target;
use lilac::parse::{parse_full, CompilerError};

use clap::{Parser, Subcommand};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to output file to.
    #[arg(short, long, default_value_t = String::from("output.html"))]
    output: String, 

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Build the project
    Build {
        /// Path to the project
        #[arg(default_value_t = String::from("."))]
        path: String,
    }
}

fn main() {
    let args = Args::parse();

    if let Some(command) = args.command {
        match command {
            Command::Build { path } => {
                let job = lilac::job::Job {
                    path: PathBuf::from(path),
                    output: PathBuf::from(args.output.clone()),
                };
                
                job.run();
            }
        }
    } else {
        println!("No command provided.");
    }
}
