use std::{collections::HashMap, path::PathBuf};

use owo_colors::OwoColorize;

use crate::{compile::fill_holes, parse::parse_full, Component, Node};

pub struct Job {
    pub path: PathBuf,
    pub output: PathBuf,
}

impl Job {
    pub fn run(&self) {
        match compile(self) {
            Ok(_) => {}
            Err(err) => println!("{}: {}", "ERR".red(), err),
        }
    }
}

fn compile(job: &Job) -> Result<(), String> {
    let files = list_files(&job.path).map_err(|e| e.to_string())?;

    let mut components_map = HashMap::new();

    for path in files {
        let component = load_componet(path).map_err(|e| e.to_string())?;
        components_map.insert(component.name.clone(), component);
    }

    let root = components_map
        .get("Root")
        .ok_or("No root component found.".to_string())?
        .clone();

    let mut root_node = Node::Component(root.clone());

    fill_holes(&mut root_node, &components_map).map_err(|e| e.to_string())?;

    let code = format!(
        "<!DOCTYPE html>
<script>{}</script>
{}",
        include_str!("../prelude.js"),
        root_node.full_codegen()
    );

    println!("{}", code);

    write_file(&job.output, &code).map_err(|e| e.to_string())?;

    Ok(())
}

fn load_file(path: &PathBuf) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

fn write_file(path: &PathBuf, contents: &str) -> Result<(), std::io::Error> {
    std::fs::write(path, contents)
}

fn list_files(path: &PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = vec![];

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "lilac") {
            files.push(path.clone());
        }

        if path.is_dir() {
            files.extend(list_files(&path)?);
        }
    }

    Ok(files)
}

fn load_componet(path: PathBuf) -> Result<Component, String> {
    let contents = load_file(&path).map_err(|_| "Unable to load file.".to_string())?;

    let name = path.file_stem().unwrap().to_str().unwrap();

    parse_full(&contents, name).map_err(|err| err.format(&contents))
}
