use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::atomic::Ordering};

use ress::tokens::Token;

use crate::{utils::find_and_replace_js_identifiers, Component, JSExpression, Node, ID_COUNTER};

#[derive(Debug, Clone)]
pub struct ComponentVariableRenamer {
    prefix: String,
    identifiers: Rc<RefCell<Vec<String>>>,
}

impl ComponentVariableRenamer {
    pub fn new(component_name: &String) -> Self {
        ComponentVariableRenamer {
            prefix: format!(
                "__{}_{}_",
                component_name,
                ID_COUNTER.fetch_add(1, Ordering::SeqCst)
            ),
            identifiers: Rc::new(RefCell::new(vec![])),
        }
    }

    fn rename(&self, expr: &String) -> String {
        format!("{}{}", self.prefix, expr)
    }

    pub fn process(&self, expr: &JSExpression) -> JSExpression {
        let mut expr = expr.clone();

        self.identifiers
            .borrow_mut()
            .extend(get_declared_js_identifiers(&expr).into_iter());

        self.identifiers.borrow_mut().dedup();

        let expr = self.process_no_declared(&expr);

        expr
    }

    pub fn process_no_declared(&self, expr: &JSExpression) -> JSExpression {
        let mut expr = expr.clone();

        for ident in self.identifiers.borrow().iter() {
            let repl = self.rename(ident);
            expr = find_and_replace_js_identifiers(&expr, ident, &repl);
        }

        expr
    }
}

fn get_declared_js_identifiers(expr: &JSExpression) -> Vec<String> {
    let mut identifiers = vec![];

    let mut previous: Option<Token<&str>> = None;

    for item in ress::Scanner::new(&expr) {
        if let Ok(item) = item {
            if item.token.is_ident() {
                if let Some(ref prev) = previous {
                    if let Token::Keyword(kw) = prev {
                        let kw = kw.as_str();
                        if kw == "let" || kw == "const" || kw == "var" || kw == "function" {
                            let ident = item.token.to_string();
                            if !identifiers.contains(&ident) {
                                identifiers.push(ident);
                            }
                        }
                    }
                }
            }

            previous = Some(item.token);
        }
    }

    identifiers
}
