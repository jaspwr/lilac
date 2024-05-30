pub type StyleSheet = Vec<Rule>;

#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Selector,
    pub properties: Vec<Property>,
}

impl Rule {
    pub fn matches_classes(&self, classes: &Vec<String>) -> bool {
        self.selector.matches_classes(classes)
    } 
}

#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub value: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Selector {
    None,
    Tag(String),
    Class(String),
    ID(String),
    All,
}

impl Selector {
    pub fn matches_classes(&self, classes: &Vec<String>) -> bool {
        match self {
            Selector::Class(s) => classes.contains(s),
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CSSParseError {
    pub location: usize,
    pub message: String,
}

#[derive(PartialEq, Debug, Copy, Clone)]
enum ParsingContext {
    Selector,
    PropertyName,
    AwaitingColon,
    PropertyValue,
}

fn assert_state(
    state: ParsingContext,
    expected: ParsingContext,
    token: &str,
    pos: usize,
) -> Result<(), CSSParseError> {
    if state != expected {
        return Err(CSSParseError {
            location: pos,
            message: format!("Unexpected {:?}", token),
        });
    }
    Ok(())
}

fn parse_selector(s: String, pos: usize) -> Result<Selector, CSSParseError> {
    if s == "*" {
        return Ok(Selector::All);
    }

    if s.starts_with("#") {
        if s.len() < 2 {
            return Err(CSSParseError {
                location: pos,
                message: "Empty ID name.".to_string(),
            });
        }
        return Ok(Selector::ID(s[1..].to_string()));
    }
    if s.starts_with(".") {
        if s.len() < 2 {
            return Err(CSSParseError {
                location: pos,
                message: "Empty Class name.".to_string(),
            });
        }
        return Ok(Selector::Class(s[1..].to_string()));
    }

    assert!(s.len() > 0);

    Ok(Selector::Tag(s))
}

pub fn parse(input: &str) -> Result<StyleSheet, CSSParseError> {
    let mut pos = 0;

    let mut ss: StyleSheet = vec![];
    let mut state = ParsingContext::Selector;

    let mut current_style = Rule {
        selector: Selector::None,
        properties: vec![],
    };

    let mut current_property = Property {
        name: String::new(),
        value: vec![],
    };

    while let Some(token) = next_token(input, &mut pos)? {
        if state == ParsingContext::AwaitingColon {
            if token != Token::Colon {
                return Err(CSSParseError {
                    location: pos,
                    message: "Expected colon".to_string(),
                });
            }
            state = ParsingContext::PropertyValue;
            continue;
        }

        match token {
            Token::OpenBrace => {
                assert_state(state, ParsingContext::Selector, "{", pos)?;
                state = ParsingContext::PropertyName;
            }
            Token::CloseBrace => {
                assert_state(state, ParsingContext::PropertyName, "}.", pos)?;
                ss.push(current_style.clone());
                current_style.properties.clear();
                state = ParsingContext::Selector;
            }
            Token::Colon => {
                todo!();
            }
            Token::Semicolon => {
                assert_state(state, ParsingContext::PropertyValue, ";", pos)?;
                current_style.properties.push(current_property.clone());
                current_property.name.clear();
                current_property.value.clear();
                state = ParsingContext::PropertyName;
            }
            Token::Identifier(s) => match state {
                ParsingContext::Selector => {
                    current_style.selector = parse_selector(s, pos)?;
                    current_style.properties = vec![];
                }
                ParsingContext::PropertyName => {
                    current_property.name = s;
                    state = ParsingContext::AwaitingColon;
                }
                ParsingContext::PropertyValue => {
                    current_property.value.push(s);
                }
                ParsingContext::AwaitingColon => {
                    unreachable!();
                }
            },
            Token::String(s) => {
                assert_state(state, ParsingContext::PropertyValue, "string", pos)?;
                current_property.value.push(s);
            }
        }
    }

    if state != ParsingContext::Selector {
        return Err(CSSParseError {
            location: pos,
            message: "Unbalanced curly braces".to_string(),
        });
    }

    Ok(ss)
}

#[derive(PartialEq, Debug)]
enum Token {
    Identifier(String),
    String(String),
    Colon,
    Semicolon,
    OpenBrace,
    CloseBrace,
}

fn skip_whitespace_and_commnets(input: &str, pos: &mut usize) {
    let mut in_comment = false;

    while *pos < input.len() && (in_comment || input.chars().nth(*pos).unwrap().is_whitespace()) {
        if in_comment {
            if input[*pos..].starts_with("*/") {
                in_comment = false;
                *pos += 1;
            }
        } else {
            if input[*pos..].starts_with("/*") {
                in_comment = true;
                *pos += 1;
            }
        }

        *pos += 1;
    }
}

fn next_token(input: &str, pos: &mut usize) -> Result<Option<Token>, CSSParseError> {
    skip_whitespace_and_commnets(input, pos);
    if *pos >= input.len() {
        return Ok(None);
    }

    let start = *pos;
    let c = input.chars().nth(*pos).unwrap();
    if c == '{' {
        *pos += 1;
        return Ok(Some(Token::OpenBrace));
    }
    if c == '}' {
        *pos += 1;
        return Ok(Some(Token::CloseBrace));
    }
    if c == ':' {
        *pos += 1;
        return Ok(Some(Token::Colon));
    }
    if c == ';' {
        *pos += 1;
        return Ok(Some(Token::Semicolon));
    }
    if c == '"' {
        *pos += 1;
        let mut escaped = false;
        while *pos < input.len() {
            let c = input.chars().nth(*pos).unwrap();
            if !escaped && c == '\\' {
                escaped = true;
            } else if escaped {
                escaped = false;
            } else if c == '"' {
                *pos += 1;
                return Ok(Some(Token::String(input[start..*pos].to_string())));
            }
            *pos += 1;
        }
        return Err(CSSParseError {
            location: *pos,
            message: "Unterminated string".to_string(),
        });
    }

    while *pos < input.len() {
        let c = input.chars().nth(*pos).unwrap();
        if !id_char(c) {
            break;
        }
        *pos += 1;
    }

    if *pos > start {
        return Ok(Some(Token::Identifier(input[start..*pos].to_string())));
    }

    Ok(None)
}

fn id_char(c: char) -> bool {
    !c.is_whitespace() && c != '"' && c != ':' && c != ';' && c != '{' && c != '}'
}
