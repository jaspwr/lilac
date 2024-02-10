use std::collections::HashMap;

use crate::{parse::CompilerError, utils::children_of, Component, Node};

pub fn fill_holes(
    root: &mut Node,
    components_map: &HashMap<String, Component>,
) -> Result<(), String> {
    if let Some(children) = children_of(root) {
        for node in children {
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

                *node = Node::Component(instance);
            } else {
                fill_holes(node, components_map)?;
            }
        }
    }

    Ok(())
}
