use std::collections::HashMap;

use crate::{parse::CompilerError, utils::children_of, Component, Node};

pub fn fill_holes(
    root: &mut Node,
    components_map: &HashMap<String, Component>,
) -> Result<(), String> {
    let recursion_stack = vec![];
    _fill_holes(root, recursion_stack, components_map)
}

pub fn _fill_holes(
    root: &mut Node,
    recursion_stack: Vec<String>,
    components_map: &HashMap<String, Component>,
) -> Result<(), String> {
    if let Some(children) = children_of(root) {
        for node in children {
            let mut recursion_stack = recursion_stack.clone();

            if let Node::ComponentHole {
                name,
                position,
                props,
                file_contents,
            } = node
            {
                let component = components_map.get(name).ok_or_else(|| {
                    CompilerError {
                        position: position.clone(),
                        message: format!("Component {} not found.", name),
                    }
                    .format(file_contents)
                })?;

                let mut instance = component.clone();

                instance.props = props.clone();

                if recursion_stack.contains(name) {
                    instance.recursive = true;    
                }

                recursion_stack.push(name.clone());

                *node = Node::Component(instance);
            }

            if let Node::Component(c) = node {
                if c.recursive {
                    continue;
                }
            }

            _fill_holes(node, recursion_stack, components_map)?;
        }
    }

    Ok(())
}
