use crate::{css::*, utils::children_of, ClassList, Component, Element, Id, Node};

pub fn scope_css_to_component(mut component: Component, mut styles: StyleSheet) -> (StyleSheet, Component) {
    let prefix = &format!("-comp{}-", component.name.clone());

    let mut node = Node::Component(component);
    let (ids, classes, tags) = handle_css(&mut styles, prefix);

    replace_refs(&mut node, prefix, &ids, &classes, &tags);

    let component = match node {
        Node::Component(c) => c,
        _ => unreachable!(),
    };

    (styles, component)
}

fn add_class(e: &mut Element, class: String) {
    match &mut e.classes {
        None => {
            e.classes = Some(ClassList::Static(vec![class]));
        }
        Some(classes_) => match classes_ {
            ClassList::Static(s) => {
                let mut new_classes = vec![class];
                new_classes.extend(s.clone());
                e.classes = Some(ClassList::Static(new_classes));
            }
            ClassList::Reactive(expr) => {
                e.classes = Some(ClassList::Reactive(format!(
                    "({}) + \" {}\"",
                    expr.clone(),
                    class
                )));
            }
        },
    }
}

fn replace_refs(
    node: &mut Node,
    prefix: &str,
    ids: &Vec<String>,
    classes: &Vec<String>,
    tags: &Vec<String>,
) {
    if let Node::Element(e) = node {
        if let Some(id) = &mut e.id {
            match id {
                Id::Static(s) => {
                    let class = format!("id{}{}", prefix, s);
                    if ids.contains(s) {
                        add_class(e, class);
                    }
                }
                Id::Reactive(expr) => {
                    todo!();
                    // let class = format!("id{}{}", prefix, );
                    // e.id = Some(Id::Reactive(format!("\"{}\" + ({})", prefix , expr.clone())));
                }
            }
        }

        if let Some(classes_) = &mut e.classes {
            match classes_ {
                ClassList::Static(s) => {
                    let mut new_classes = vec![];
                    for c in s {
                        if classes.contains(c) {
                            new_classes.push(format!("class{}{}", prefix, c));
                        }
                    }
                    e.classes = Some(ClassList::Static(new_classes));
                }
                ClassList::Reactive(expr) => {
                    e.classes = Some(ClassList::Reactive(format!(
                        "({}).split(\" \").map((s) => \"class{}\" + s).join()",
                        expr.clone(),
                        prefix
                    )));
                }
            }
        }

        if tags.contains(&e.name) {
            add_class(e, format!("tag{}{}", prefix, e.name));
        }
    }

    if let Some(children) = children_of(node) {
        for node in children {
            replace_refs(node, prefix, ids, classes, tags);
        }
    }
}

fn handle_css(css: &mut StyleSheet, prefix: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut ids = vec![];
    let mut classes = vec![];
    let mut tags = vec![];

    for rule in css {
        if let Selector::Tag(t) = &mut rule.selector {
            tags.push(t.clone());
            rule.selector = Selector::Class(format!("tag{}{}", prefix, t));
        } else if let Selector::Class(c) = &mut rule.selector {
            classes.push(c.clone());
            rule.selector = Selector::Class(format!("class{}{}", prefix, c));
        } else if let Selector::ID(id) = &mut rule.selector {
            classes.push(id.clone());
            rule.selector = Selector::Class(format!("id{}{}", prefix, id));
        }
    }

    (ids, classes, tags)
}
