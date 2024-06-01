use std::sync::atomic::Ordering;

use crate::{JSExpression, Node, ID_COUNTER};

pub fn find_and_replace_js_identifiers(
    expr: &JSExpression,
    find: &String,
    replace: &String,
) -> JSExpression {
    let mut expr = expr.to_string();

    let mut places = vec![];

    for item in ress::Scanner::new(&expr) {
        if let Ok(item) = item {
            if item.token.is_ident() {
                if item.token.to_string() == *find {
                    places.push((item.span, false));
                } else if item.token.to_string() == format!("${}", find) {
                    places.push((item.span, true));
                }
            }
        }
    }

    for (place, has_dolar) in places.iter().rev() {
        let range = place.start..place.end;

        if *has_dolar {
            let replace = format!("${}", replace);
            expr.replace_range(range, &replace);
        } else {
            expr.replace_range(range, &replace);
        }
    }

    expr
}

pub fn children_of<'a>(node: &'a mut Node) -> Option<&'a mut Vec<Node>> {
    match node {
        Node::Component(c) => Some(&mut c.children),
        Node::Element(e) => Some(&mut e.children),
        Node::ConditionalElements { children, .. } => Some(children),
        Node::Loop { children, .. } => Some(children),
        _ => return None,
    }
}

pub fn uid() -> String {
    format!("{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

pub fn filter_whitespace_nodes(node: &mut Node) {
    if let Some(children) = children_of(node) {
        children.retain(|child| match child {
            Node::Text(t) => !is_all_whitespace(t),
            _ => true,
        });

        for child in children {
            filter_whitespace_nodes(child);
        }
    }
}

pub fn is_all_whitespace(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace())
}

pub trait StartsWithAt {
    fn starts_with_at(&self, start_index: usize, other: &str) -> bool;
}

impl StartsWithAt for String {
    fn starts_with_at(&self, start_index: usize, other: &str) -> bool {
        self.chars().skip(start_index).zip(other.chars()).all(|(a, b)| a == b)
    }
}

impl StartsWithAt for str {
    fn starts_with_at(&self, start_index: usize, other: &str) -> bool {
        self.chars().skip(start_index).zip(other.chars()).all(|(a, b)| a == b)
    }
}
