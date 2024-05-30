use std::string::ParseError;

use owo_colors::OwoColorize;

use crate::{Attribute, ClassList, Component, Dialect, Element, Id, Node};

pub fn parse_full(
    input: &str,
    component_name: &str,
    dialect: Dialect,
) -> Result<Component, CompilerError> {
    let children = parse(input, 0, input.len())?;

    Ok(Component {
        name: component_name.to_string(),
        dialect,
        props: vec![],
        children,
        recursive: false,
    })
}

type Position = usize;

#[derive(Debug)]
pub struct CompilerError {
    pub position: Position,
    pub message: String,
}

impl CompilerError {
    pub fn format(&self, input: &str) -> String {
        format!("{} {}", format_position(input, self.position), self.message)
    }
}

fn format_position(input: &str, position: Position) -> String {
    let line = input[..position].chars().filter(|c| *c == '\n').count() + 1;
    let column = position - input[..position].rfind('\n').unwrap_or(0);
    format!("[{}:{}] ", line, column)
}

fn push_text(text: &mut String, nodes: &mut Vec<Node>) {
    if !text.is_empty() {
        nodes.push(Node::Text(text.clone()));
        text.clear();
    }
}

fn parse(input: &str, start: usize, end: usize) -> Result<Vec<Node>, CompilerError> {
    let mut pos = start;

    let mut nodes = vec![];

    let mut text = String::new();

    while pos < end {
        skip_comments(input, &mut pos);

        let c = input.chars().nth(pos).unwrap();

        if c == '<' {
            push_text(&mut text, &mut nodes);
            nodes.push(parse_elem(input, &mut pos)?);
        } else if c == '{' {
            push_text(&mut text, &mut nodes);

            let inner = curly_inner(input, &mut pos);

            if inner.starts_with("#") {
                nodes.push(handle_expression(input, &mut pos, inner)?);
            } else if inner.starts_with("/") {
                panic!();
            } else {
                nodes.push(Node::ReactiveText(inner));
            }
        } else {
            text.push(c);
        }
        pos += 1;
    }

    push_text(&mut text, &mut nodes);

    Ok(nodes)
}

fn handle_expression(input: &str, pos: &mut usize, opening: String) -> Result<Node, CompilerError> {
    assert!(opening.starts_with("#"));
    let opening_tokens = tokenize_expression(opening);

    if opening_tokens.len() < 2 {
        return Err(CompilerError {
            position: *pos,
            message: "Expected expression".to_string(),
        });
    }

    match opening_tokens[0].as_str() {
        "if" => {
            let expression = opening_tokens[1..].join(" ");

            let opening_pos = *pos;
            let closing_pos = search_for_closing(input, "{/if}", pos)?;

            let children = parse(input, opening_pos, closing_pos)?;

            Ok(Node::ConditionalElements {
                condition: expression,
                children,
            })
        }
        "for" => {
            if opening_tokens.len() < 4 || opening_tokens[2] != "in" {
                return Err(CompilerError {
                    position: *pos,
                    message: "Invalid for expression".to_string(),
                });
            }

            let iterator_variable = opening_tokens[1].clone();

            let mut iteratable = opening_tokens[3..].join(" ");
            let mut reactive_list = false;

            if opening_tokens[3] == "$lstate" {
                reactive_list = true;
                iteratable = opening_tokens[4..].join(" ");
            }

            let opening_pos = *pos;
            let closing_pos = search_for_closing(input, "{/for}", pos)?;

            let children = parse(input, opening_pos, closing_pos)?;

            Ok(Node::Loop {
                iterator_variable,
                reactive_list,
                iteratable,
                children,
            })
        }
        _ => Err(CompilerError {
            position: *pos,
            message: format!("Unknown directive {}", opening_tokens[0]),
        }),
    }
}

fn search_for_closing(
    haystack: &str,
    needle: &str,
    pos: &mut usize,
) -> Result<usize, CompilerError> {
    while *pos < haystack.len() {
        if haystack[*pos..].starts_with(needle) {
            let at = *pos;
            *pos += needle.len();
            return Ok(at);
        }
        *pos += 1;
    }

    Err(CompilerError {
        position: *pos,
        message: format!("expected {}", needle),
    })
}

fn tokenize_expression(expr: String) -> Vec<String> {
    let mut tokens = vec![];

    let mut token = String::new();

    let mut pos = 1;
    while pos < expr.len() {
        let c = expr.chars().nth(pos).unwrap();

        if c.is_whitespace() {
            if !token.is_empty() {
                tokens.push(token.clone());
                token.clear();
            }
        } else if c == '{' {
            let inner = curly_inner(&expr, &mut pos);
            if !inner.is_empty() {
                tokens.push(inner);
            }
        } else {
            token.push(c);
        }

        pos += 1;
    }

    if !token.is_empty() {
        tokens.push(token);
    }

    tokens
}

fn curly_inner(input: &str, pos: &mut usize) -> String {
    *pos += 1;

    let start = *pos;

    let mut depth = 1;

    while *pos < input.len() {
        if input[*pos..].starts_with("{") {
            depth += 1;
        } else if input[*pos..].starts_with("}") {
            depth -= 1;
        }

        *pos += 1;

        if depth == 0 {
            break;
        }
    }

    input[start..(*pos - 1)].to_string()
}

#[derive(Eq, PartialEq, Debug)]
enum AttrParsingContext {
    None,
    Quotes,
    Curly,
}

fn skip_comments(input: &str, pos: &mut usize) {
    let mut in_comment = false;

    if input[*pos..].starts_with("<!--") {
        in_comment = true;
        *pos += 4;
    }

    while *pos < input.len() && in_comment {
        if in_comment {
            if input[*pos..].starts_with("-->") {
                *pos += 3;
                return;
            }
        }

        *pos += 1;
    }
}

fn parse_elem(input: &str, pos: &mut usize) -> Result<Node, CompilerError> {
    let starting_pos = *pos;

    *pos += 1;
    skip_whitespace(input, pos);

    let name = grab_alphanum_token(input, pos);

    if name.is_empty() {
        return Err(CompilerError {
            position: *pos,
            message: "Expected element name".to_string(),
        });
    }

    let children_are_html = name != "style" && name != "script";

    let mut no_closer = false;

    let mut unparsed_attributes = vec![];

    let mut token = String::new();

    let mut attr_context = AttrParsingContext::None;

    let mut depth = 0;

    while *pos < input.len()
        && !(attr_context == AttrParsingContext::None && input[*pos..].starts_with(">"))
    {
        let c = input[*pos..].chars().next().unwrap();

        if attr_context == AttrParsingContext::None {
            if (c.is_whitespace() || c == '=') && !token.is_empty() {
                unparsed_attributes.push(token.clone());
                token.clear();
            }

            skip_whitespace(input, pos);

            let c = input[*pos..].chars().next().unwrap();

            if c == '"' {
                attr_context = AttrParsingContext::Quotes;
            } else if c == '{' {
                attr_context = AttrParsingContext::Curly;
            }

            if input[*pos..].starts_with("/>") {
                no_closer = true;
                break;
            }

            if input[*pos..].starts_with(">") {
                break;
            }

            if c == '=' {
                unparsed_attributes.push("=".to_string());
            } else {
                println!("{}", c);
                token.push(c);
            }
        } else {
            println!("{}", c);
            token.push(c);

            match attr_context {
                AttrParsingContext::None => {}
                AttrParsingContext::Quotes => {
                    if c == '"' {
                        attr_context = AttrParsingContext::None;
                    }
                }
                AttrParsingContext::Curly => {
                    if c == '}' {
                        if depth == 0 {
                            attr_context = AttrParsingContext::None;
                        } else {
                            depth -= 1;
                        }
                    } else if c == '{' {
                        depth += 1;
                    }
                }
            }
        }

        *pos += 1;
    }

    if !token.is_empty() {
        unparsed_attributes.push(token);
    }

    let mut attributes = vec![];

    println!("{:?}", unparsed_attributes);

    if !unparsed_attributes.is_empty() {
        while !unparsed_attributes.is_empty() {
            if unparsed_attributes.len() < 3 {
                return Err(CompilerError {
                    position: *pos,
                    message: "Expected attribute value".to_string(),
                });
            }
            let name = unparsed_attributes.remove(0);
            let eq = unparsed_attributes.remove(0);
            let value = unparsed_attributes.remove(0);

            if eq != "=" {
                return Err(CompilerError {
                    position: *pos,
                    message: "Expected =".to_string(),
                });
            }

            if value.starts_with("\"") {
                if !value.ends_with("\"") && value.len() > 1 {
                    if value.ends_with(",") {
                        return Err(CompilerError {
                            position: *pos,
                            message: "Commas should not be used to separate attribute arguments"
                                .to_string(),
                        });
                    }

                    return Err(CompilerError {
                        position: *pos,
                        message: "Expected \"".to_string(),
                    });
                }

                attributes.push(Attribute::Static(crate::StaticAttribute {
                    name,
                    value: value[1..value.len() - 1].to_string(),
                }));
            } else if value.starts_with("{") {
                if !value.ends_with("}") {
                    return Err(CompilerError {
                        position: *pos,
                        message: "Expected }".to_string(),
                    });
                }

                attributes.push(Attribute::Reactive(crate::ReactiveAttribute {
                    name,
                    value: value[1..value.len() - 1].to_string(),
                }));
            } else {
                return Err(CompilerError {
                    position: *pos,
                    message: "Invalid attribute value".to_string(),
                });
            }
        }
    }

    if *pos == input.len() {
        return Err(CompilerError {
            position: *pos,
            message: format!("Expected >. {} tag was never closed", name),
        });
    }

    *pos += 1;

    let mut children = vec![];

    let opening_tag_end = *pos;
    let mut closing_tag_pos = 0;

    if !no_closer {
        let (end_pos, closing_tag_pos_) = find_closing(input, &name, *pos)?;

        closing_tag_pos = closing_tag_pos_;

        if children_are_html {
            children = parse(input, *pos, closing_tag_pos)?;
        } else {
            let inner = &input[*pos..closing_tag_pos];
            // TODO: Be smart about JS and CSS.
            if !inner.is_empty() {
                children.push(Node::Text(inner.to_string()));
            }
        }

        *pos = end_pos;
    }

    if name == "script" {
        let js = input[opening_tag_end..closing_tag_pos].to_string();
        return Ok(Node::ScriptTag(crate::ScriptTag {
            attributes,
            code: js,
        }));
    }

    if name == "style" {
        let css = &input[opening_tag_end..closing_tag_pos];
        let css = crate::css::parse(css).map_err(|e| CompilerError {
            position: e.location + opening_tag_end,
            message: format!("{}: {}", "CSS syntax error".red(), e.message),
        })?;
        return Ok(Node::StyleTag(css));
    }

    if name.chars().next().unwrap().is_uppercase() {
        return Ok(Node::ComponentHole {
            name,
            position: starting_pos,
            props: attributes,
            file_contents: Box::new(input.to_string()),
        });
    }

    let id = attributes
        .iter()
        .find(|a| a.name() == "id")
        .map(|a| match a {
            Attribute::Static(sa) => Id::Static(sa.value.clone()),
            Attribute::Reactive(ra) => Id::Reactive(ra.value.clone()),
        });

    let classes = attributes
        .iter()
        .find(|a| a.name() == "class")
        .map(|a| match a {
            Attribute::Static(sa) => ClassList::Static(
                sa.value
                    .split(" ")
                    .map(|s| s.to_string())
                    .filter(|s| s.len() != 0)
                    .collect(),
            ),
            Attribute::Reactive(ra) => ClassList::Reactive(ra.value.clone()),
        });

    let attributes = attributes
        .into_iter()
        .filter(|a| a.name() != "id" && a.name() != "class")
        .collect();

    Ok(Node::Element(Element {
        name,
        id,
        classes,
        attributes,
        children,
    }))
}

fn grab_alphanum_token(input: &str, pos: &mut usize) -> String {
    let start = *pos;

    while *pos < input.len() && input[*pos..].starts_with(|c: char| c.is_alphanumeric()) {
        *pos += 1;
    }

    input[start..(*pos)].to_string()
}

fn skip_whitespace(input: &str, pos: &mut usize) {
    while *pos < input.len() && input[*pos..].starts_with(char::is_whitespace) {
        *pos += 1;
    }
}

fn find_closing(input: &str, name: &str, mut pos: usize) -> Result<(usize, usize), CompilerError> {
    let mut depth = 0;

    while pos < input.len() {
        skip_comments(input, &mut pos);
        if input[pos..].starts_with("<") {
            let mut pos = pos + 1;
            skip_whitespace(input, &mut pos);
            if input[pos..].starts_with(name) {
                depth += 1;
            }
        }

        if input[pos..].starts_with("</") {
            let closing_tag_pos = pos;
            let mut pos = pos + 2;
            skip_whitespace(input, &mut pos);

            if input[pos..].starts_with(name) {
                pos += name.len();
                skip_whitespace(input, &mut pos);

                if input[pos..].starts_with(">") {
                    if depth == 0 {
                        return Ok((pos, closing_tag_pos));
                    }
                    depth -= 1;
                }
            }
        }
        pos += 1;
    }

    Err(CompilerError {
        position: input.len(),
        message: format!("Could not find closing tag for {}", name),
    })
}
