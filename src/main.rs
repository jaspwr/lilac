use std::{collections::HashMap, path::PathBuf, sync::atomic::AtomicUsize, usize};

use parse::{parse_full, CompilerError};

pub static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

mod codegen;
mod js_component_scoping;
mod parse;
mod utils;
mod job;
mod compile;

use clap::{Parser, Subcommand};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to output file to.
    #[arg(short, long, default_value_t = String::from("output.html"))]
    output: String, 

    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,

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
                let job = job::Job {
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

pub type JSExpression = String;

#[derive(Debug, Clone)]
pub enum Node {
    Component(Component),
    ComponentHole {
        name: String,
        position: usize,
        props: Vec<Attribute>,
        file_contents: Box<String>,
    },
    Element(Element),
    ScriptTag(ScriptTag),
    Text(String),
    ReactiveText(String),
    ConditionalElements {
        condition: JSExpression,
        children: Vec<Node>,
    },
    Loop {
        iterator_variable: String,
        iteratable: JSExpression,
        reactive_list: bool,
        children: Vec<Node>,
    },
}

#[derive(Debug, Clone)]
pub struct Element {
    name: String,
    attributes: Vec<Attribute>,
    children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct ScriptTag {
    attributes: Vec<Attribute>,
    code: JSExpression,
}

#[derive(Debug, Clone)]
pub enum Attribute {
    Static(StaticAttribute),
    Reactive(ReactiveAttribute),
}

impl Attribute {
    pub fn is_reactive(&self) -> bool {
        match self {
            Attribute::Reactive(_) => true,
            _ => false,
        }
    } 
}

#[derive(Debug, Clone)]
pub struct StaticAttribute {
    name: String,
    value: String,
}

#[derive(Debug, Clone)]
pub struct ReactiveAttribute {
    name: String,
    value: JSExpression,
}

#[derive(Debug, Clone)]
pub struct Component {
    name: String,
    props: Vec<Attribute>,
    children: Vec<Node>,
}
