use std::{collections::HashMap, sync::atomic::AtomicUsize};

use css::StyleSheet;

pub mod codegen;
pub mod compile;
pub mod css;
pub mod css_component_scoping;
pub mod job;
pub mod js_codegen;
pub mod js_component_scoping;
pub mod parse;
pub mod rs_codegen;
pub mod utils;

pub static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[wasm_bindgen::prelude::wasm_bindgen]
pub struct File {
    name: String,
    contents: String,
}

#[wasm_bindgen::prelude::wasm_bindgen]
pub fn new_file(name: String, contents: String) -> File {
    File { name, contents }
}

// #[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn wasm_compile(files: Vec<File>) -> String {
    match wasm_compile_(files) {
        Ok(output) => output,
        Err(e) => format!(
            "<span style=\"color: red; font-weight: bold;\">ERROR:</span> {}",
            e
        ),
    }
}

pub fn wasm_compile_(files: Vec<File>) -> Result<String, String> {
    let mut components_map = HashMap::new();

    let mut stylesheet = vec![];

    for File { name, contents } in files {
        let component =
            parse::parse_full(&contents, &name, Dialect::JsLilac).map_err(|err| err.format(&contents))?;

        let (styles, component) = job::collect_css(component);

        let (styles, component) = css_component_scoping::scope_css_to_component(component, styles);

        stylesheet.extend(styles);

        if let Some(_) = components_map.get(&component.name) {
            return Err(format!(
                "Conflicting definitions for component '{}'.",
                component.name
            ));
        }

        components_map.insert(component.name.clone(), component);
    }

    // stylesheet.extend(
    //     css::parse(get_root_css(&job.path)?.as_str())
    //         .map_err(|e| format!("Error parsing root.css: {}", e.message))?,
    // );

    let root = components_map
        .get("Root")
        .ok_or("No root component found.".to_string())?
        .clone();

    let mut root_node = Node::Component(root.clone());

    compile::fill_holes(&mut root_node, &components_map).map_err(|e| e.to_string())?;

    root_node.codegen(job::Target::Web, stylesheet)
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
    StyleTag(StyleSheet),
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
    id: Option<Id>,
    classes: Option<ClassList>,
    attributes: Vec<Attribute>,
    children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Id {
    Static(String),
    Reactive(JSExpression),
}

impl Id {
    pub fn is_reactive(&self) -> bool {
        match self {
            Id::Reactive(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ClassList {
    Static(Vec<String>),
    Reactive(JSExpression),
}

impl ClassList {
    pub fn is_reactive(&self) -> bool {
        match self {
            ClassList::Reactive(_) => true,
            _ => false,
        }
    }
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

    pub fn name<'a>(&'a self) -> &'a String {
        match self {
            Attribute::Reactive(ra) => &ra.name,
            Attribute::Static(sa) => &sa.name,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialect {
    JsLilac,
    TsLilac,
    RsLilac,
}

#[derive(Debug, Clone)]
pub struct Component {
    name: String,
    dialect: Dialect,
    props: Vec<Attribute>,
    children: Vec<Node>,
    /// If the component instance is a child of the same component
    recursive: bool,
}
