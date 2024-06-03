use std::{collections::HashMap, path::PathBuf, process::exit};

use owo_colors::OwoColorize;

use crate::{
    compile::fill_holes,
    config::load_config,
    css::{self, StyleSheet},
    css_component_scoping::scope_css_to_component,
    js_codegen::codegen_stylesheet,
    parse::parse_full,
    utils::children_of,
    Component, Dialect, Node,
};

pub struct Job {
    pub path: PathBuf,
    pub output: PathBuf,
}

pub enum Target {
    Unknown,
    Web,
    GL,
}

impl Job {
    pub fn run(&self) {
        match compile(self) {
            Ok(_) => {}
            Err(err) => {
                println!("{}: {}", "ERR".red(), err);
                exit(1);
            }
        }
    }
}

fn compile(job: &Job) -> Result<(), String> {
    let config = load_config(&job.path)?;

    if config.lilac_version != env!("CARGO_PKG_VERSION") {
        return Err(format!(
            "Version mismatch. Expected: {}, Found: {}",
            env!("CARGO_PKG_VERSION"),
            config.lilac_version
        ));
    }

    let files = list_files(&job.path).map_err(|e| e.to_string())?;

    let mut components_map = HashMap::new();

    let mut stylesheet = vec![];

    for path in files {
        let component = load_componet(path).map_err(|e| e.to_string())?;

        let (styles, component) = collect_css(component);

        let (styles, component) = scope_css_to_component(component, styles);

        stylesheet.extend(styles);

        if let Some(_) = components_map.get(&component.name) {
            return Err(format!(
                "Conflicting definitions for component '{}'.",
                component.name
            ));
        }

        components_map.insert(component.name.clone(), component);
    }

    stylesheet.extend(
        css::parse(get_root_css(&job.path)?.as_str())
            .map_err(|e| format!("Error parsing root.css: {}", e.message))?,
    );

    let root = components_map
        .get("Root")
        .ok_or("No root component found.".to_string())?
        .clone();

    let target = match root.dialect {
        Dialect::JsLilac => Target::Web,
        Dialect::TsLilac => Target::Web,
        Dialect::RsLilac => Target::GL,
    };

    let mut root_node = Node::Component(root.clone());

    fill_holes(&mut root_node, &components_map).map_err(|e| e.to_string())?;

    let code = root_node.codegen(target, stylesheet)?;

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

    const EXTENSIONS: [&str; 4] = ["lilac", "rslilac", "tslilac", "jslilac"];

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| {
            EXTENSIONS.contains(&e.to_str().unwrap_or_default())
        }) {
            files.push(path.clone());
        }

        if path.is_dir() {
            files.extend(list_files(&path)?);
        }
    }

    Ok(files)
}

fn get_root_css(path: &PathBuf) -> Result<String, String> {
    let path = path.join("root.css");

    if path.exists() {
        load_file(&path).map_err(|_| "Unable to root.css.".to_string())
    } else {
        Ok("".to_string())
    }
}

pub fn collect_css(component: Component) -> (StyleSheet, Component) {
    let mut node = Node::Component(component.clone());
    let styles = _collect_css(&mut node);
    let component = match node {
        Node::Component(c) => c,
        _ => unreachable!(),
    };
    (styles, component)
}

fn _collect_css(node: &mut Node) -> StyleSheet {
    let mut ss = vec![];

    if let Node::StyleTag(css) = node {
        ss.extend(css.clone());
    }

    if let Some(children) = children_of(node) {
        for i in (0..children.len()).rev() {
            let node = &mut children[i];
            ss.extend(_collect_css(node));
            if let Node::StyleTag(_) = node {
                children.remove(i);
            }
        }
    }
    ss
}

fn load_componet(path: PathBuf) -> Result<Component, String> {
    let contents = load_file(&path).map_err(|_| "Unable to load file.".to_string())?;

    let name = path.file_stem().unwrap().to_str().unwrap();

    let dialect = match path
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
    {
        "lilac" => Dialect::JsLilac,
        "jslilac" => Dialect::JsLilac,
        "tslilac" => Dialect::TsLilac,
        "rslilac" => Dialect::RsLilac,
        _ => unreachable!(),
    };

    parse_full(&contents, name, dialect).map_err(|err| err.format(name, &contents))
}
