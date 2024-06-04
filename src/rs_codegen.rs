use rustc_lexer::tokenize;

use crate::{
    codegen::CodegenResult,
    css::{self, StyleSheet},
    utils::{filter_whitespace_nodes, is_all_whitespace, uid},
    Attribute, ClassList, Component, Element, Node, ReactiveAttribute, ScriptTag, StaticAttribute,
};

impl Node {
    pub fn full_gl_codegen(&self, stylesheet: StyleSheet) -> CodegenResult {
        let mut root = self.clone();
        filter_whitespace_nodes(&mut root);

        let styles = codegen_stylesheet(&stylesheet);

        let ctx = CodegenContext { styles: stylesheet };

        let rs = root.gl_codegen(ctx);

        let rs = format!(
            "
use std::collections::HashMap;

use crate::{{
    d,
    element::{{Element, PhantomElement}},
    global::Globals,
    node::{{FrameRef, Node}},
    p,
    reactive::Reactive,
    reactive_list::{{ReactiveList, ReactiveListKey}},
    style::{{c, Colour, Style, StyleProperty}},
    text::{{Font, Text}},
    utils::{{rc_ref_cell, RcRefCell}},
    BoundingBoxRef, Coordinate, Size,
    element_creation_queue::{{queue_element, CreateElementFn}}
}};

pub fn root(
    gl: &glow::Context,
    globals: &mut Globals,
    frame_ref: FrameRef,
) -> (Vec<Node>, HashMap<String, Style>) {{

    {creation_code}

    {styles}

    return (vec![{elem_var_name}], stylesheet);
}}
        ",
            creation_code = rs.creation_code,
            elem_var_name = rs.elem_var_name
        );

        Ok(rs)
    }

    fn gl_codegen(&self, ctx: CodegenContext) -> ElementCode {
        match self {
            Node::Component(c) => c.rs_codegen(ctx),
            Node::Element(e) => e.rs_codegen(ctx),
            Node::Text(t) => create_text_node(t, ctx),
            Node::ReactiveText(expr) => create_reactive_text(expr, ctx),
            Node::Loop {
                iterator_variable,
                iteratable,
                reactive_list,
                children,
            } => loop_codegen(iterator_variable, iteratable, *reactive_list, children, ctx),
            Node::ConditionalElements {
                condition,
                children,
            } => conditional_codegen(condition, children, ctx),
            Node::ScriptTag(ScriptTag { attributes, code }) => ElementCode {
                creation_code: code.to_string(),
                elem_var_name: "".to_string(),
            },
            Node::StyleTag(css) => unimplemented!(),
            Node::ComponentHole {
                name,
                position,
                props,
                file_contents,
                children,
            } => todo!(),
        }
    }
}

type RustExpression = String;

fn rename_var(expr: &str) -> String {
    format!("__{}", expr.replace(".", "dot"))
}

#[derive(Debug, Clone)]
struct CodegenContext {
    styles: StyleSheet,
}

fn create_text_node(text: &str, ctx: CodegenContext) -> ElementCode {
    let elem_var_name = format!("__text{}", uid());

    let creation_code = format!(
        "
        let {elem_var_name} = Text::new(
            gl,
            \"{text}\".to_string(),
            10.0,
            &globals.main_font,
            c(\"ffffff\"),
            p(0., 0.),
            frame_ref.clone(),
        );
    "
    );

    ElementCode {
        creation_code,
        elem_var_name,
    }
}

fn codegen_stylesheet(ss: &StyleSheet) -> String {
    let mut code = "let mut stylesheet: HashMap<String, Style> = HashMap::new();".to_string();

    for rule in ss {
        match rule.selector {
            css::Selector::Class(ref class) => {
                code.push_str(&format!(
                    "
                    let mut __s = Style::default();
                    {}
                    stylesheet.insert(\"{}\".to_string(), __s);
                ",
                    codegen_rules(&rule.properties),
                    class
                ));
            }
            _ => todo!(),
        }
    }

    code
}

/// Returns (expr, reactive_clones, subscribes)
fn reactive_expression(
    expr: &RustExpression,
    unsub_point: &RustExpression,
    update_fn: &RustExpression,
) -> (RustExpression, RustExpression, RustExpression) {
    let mut vars = vec![];

    let mut seen_tokens: Vec<String> = vec![];

    let mut cursor = 0;

    let mut last_was_dollar_sign = false;

    for token in tokenize(expr) {
        let token_str = &expr[cursor..cursor + token.len];
        cursor += token.len;

        if token.kind == rustc_lexer::TokenKind::Dollar {
            last_was_dollar_sign = true;
            continue;
        }

        match token.kind {
            rustc_lexer::TokenKind::Ident => {
                if last_was_dollar_sign {
                    let mut var_name = token_str.to_string();

                    let mut looking_for_dot = true;

                    while let Some(c) = seen_tokens.pop() {
                        if is_all_whitespace(&c) {
                            continue;
                        }

                        if looking_for_dot {
                            if c != "." {
                                seen_tokens.push(c);
                                break;
                            }
                        }

                        var_name = format!("{}{}", c, var_name);

                        looking_for_dot = !looking_for_dot;
                    }

                    vars.push(var_name.to_string());
                    seen_tokens.push(format!("{}.get_copy()", rename_var(&var_name)));
                } else {
                    seen_tokens.push(token_str.to_string());
                }
            }
            _ => {
                seen_tokens.push(token_str.to_string());
            }
        }

        last_was_dollar_sign = false;
    }

    let reactive_clones = vars
        .iter()
        .map(|r| {
            let no_dots = rename_var(r);
            format!("let {no_dots} = {r}.clone();\n")
        })
        .collect::<String>();

    let subscribes = vars
        .iter()
        .map(|r| {
            let no_dots = rename_var(r);

            format!(
                "
            let __sub = {{
                let {update_fn} = {update_fn}.clone();
                
                {r}.clone().subscribe(Box::new(move |_| {{
                    ({update_fn}.clone())();
                }}))
            }};

            let {no_dots} = {r}.clone();

            {unsub_point}.add_cleanup_fn(Box::new(move || {{
                {no_dots}.unsubscribe(__sub);
            }}));
            "
            )
        })
        .collect::<String>();

    let expr = seen_tokens.join("");

    (expr, reactive_clones, subscribes)
}

fn loop_codegen(
    iterator_variable: &RustExpression,
    iteratable: &RustExpression,
    reactive_list: bool,
    children: &Vec<Node>,
    ctx: CodegenContext,
) -> ElementCode {
    let create_fn_var_name = format!("__loop{}", uid());

    let container_elem_var_name = format!("__loop_container{}", uid());

    let (iteratable, clones, subscribes) = reactive_expression(
        iteratable,
        &container_elem_var_name,
        &"create_list".to_string(),
    );

    let (children_var_names, children_code) = codegen_children(children, ctx);

    let create_fn_call = if reactive_list {
        format!(
            "
            let (key, item) = i;

            let children = {create_fn_var_name}_cpy(item, gl, globals);
            let pe = PhantomElement::new(children, frame_ref.clone());
            
            node_id_map_cpy.as_ref().borrow_mut().insert(key, pe.clone());

            vec![pe]
        "
        )
    } else {
        format!("{create_fn_var_name}_cpy(i, gl, globals)")
    };

    let mut creation_code = format!(
        "
            let frame_ref_cpy = frame_ref.clone();
            let {create_fn_var_name} = move |{iterator_variable}, gl: &glow::Context, globals: &Globals| {{
                let frame_ref = frame_ref_cpy.clone();
                {children_code}
                vec![{children_var_names}]
            }};

            let {container_elem_var_name} = PhantomElement::new(vec![], frame_ref.clone());
            
            let {container_elem_var_name}_cpy = {container_elem_var_name}.clone();
            let {create_fn_var_name}_cpy = {create_fn_var_name}.clone();
            let iteratable = {iteratable}.clone();


            let frame_ref_cpy = frame_ref.clone();
            let create_list = move |gl: &glow::Context, globals: &Globals| {{
                let frame_ref = frame_ref_cpy.clone();

                {clones}

                let children = iteratable.into_iter().map(|i| {{
                    {create_fn_call}
                }}).flatten().collect::<Vec<Node>>();

                {container_elem_var_name}_cpy.mutate(move |pe: &mut PhantomElement| {{

                    // TODO: Cleanup old children
                    pe.children = children;
                }});
            }};

            create_list(gl, globals);

            {subscribes}
        "
    );

    if reactive_list {
        creation_code = format!(
            "
            let node_id_map: RcRefCell<HashMap<ReactiveListKey, Node>> =
                rc_ref_cell(HashMap::new());
            let node_id_map_cpy = node_id_map.clone();

            {creation_code}

            let {create_fn_var_name}_cpy = {create_fn_var_name}.clone();
            let {container_elem_var_name}_cpy = {container_elem_var_name}.clone();
            let node_id_map_cpy = node_id_map.clone();

            let frame_ref_cpy = frame_ref.clone();
            let push_sub = ({iteratable}).subscribe_to_push(Box::new(move |key, item| {{
                let frame_ref = frame_ref_cpy.clone();

                let key = *key;
                let item = item.clone();
                let {create_fn_var_name}_cpy = {create_fn_var_name}_cpy.clone();
                let node_id_map_cpy = node_id_map_cpy.clone();

                let frame_ref_cpy = frame_ref.clone();
                let create_element: CreateElementFn =
                    Box::new(move |gl: &glow::Context, globals: &mut Globals| {{
                        let frame_ref = frame_ref_cpy.clone();

                        let children = {create_fn_var_name}_cpy(item, gl, globals);
                        let pe = PhantomElement::new(children, frame_ref.clone());
                        node_id_map_cpy.as_ref().borrow_mut().insert(key, pe.clone());
                        pe
                    }});

                queue_element(create_element, {container_elem_var_name}_cpy.clone());
            }}));

            let {container_elem_var_name}_cpy = {container_elem_var_name}.clone();
            let node_id_map_cpy = node_id_map.clone();

            let rem_sub = ({iteratable}).subscribe_to_remove(Box::new(move |key, ()| {{
                if let Some(pe) = node_id_map_cpy.as_ref().borrow_mut().remove(&key) {{
                    let uid = pe.uid.clone();

                    {container_elem_var_name}_cpy.mutate(move |container: &mut PhantomElement| {{
                        container.children.retain(|c| c.uid != uid);
                    }});
                }}
            }})); 

            // TODO: Unsub on cleanup
        "
        );
    }

    ElementCode {
        creation_code,
        elem_var_name: container_elem_var_name,
    }
}

fn conditional_codegen(
    condition: &RustExpression,
    children: &Vec<Node>,
    ctx: CodegenContext,
) -> ElementCode {
    let create_fn_var_name = format!("__cond{}", uid());

    let container_elem_var_name = format!("__cond_container{}", uid());

    let (condition, clones, subscribes) = reactive_expression(
        condition,
        &container_elem_var_name,
        &"rerun_cond".to_string(),
    );

    let (children_var_names, children_code) = codegen_children(children, ctx);

    let creation_code = format!("

        let frame_ref_cpy = frame_ref.clone();
        let {create_fn_var_name} = move |gl: &glow::Context, globals: &mut Globals| {{
            let frame_ref = frame_ref_cpy.clone();

            {children_code}
            vec![{children_var_names}]
        }};

        {clones}

        let init_children = if {condition} {{
            {create_fn_var_name}(gl, globals)
        }} else {{
            vec![]
        }};

        let {container_elem_var_name} = PhantomElement::new(init_children, frame_ref.clone());

        let {container_elem_var_name}_cpy = {container_elem_var_name}.clone();
        let {create_fn_var_name}_cpy = {create_fn_var_name}.clone();
        let prev_cond_result: RcRefCell<Option<bool>> = rc_ref_cell(None);

        let frame_ref_cpy = frame_ref.clone();
        let rerun_cond = move || {{
            let prev_cond_result = prev_cond_result.clone();
            let frame_ref = frame_ref_cpy.clone();
        
            let cond_result = {condition};
            
            let prev = prev_cond_result.borrow().clone();
            prev_cond_result.borrow_mut().replace(cond_result);

            if let Some(prev) = prev {{
                if prev == cond_result {{
                    return;
                }}
            }}
    
            if cond_result {{
                queue_element(Box::new(move |gl, globals| PhantomElement::new({create_fn_var_name}_cpy(gl, globals), frame_ref.clone())), {container_elem_var_name}_cpy);
            }} else {{
                
                {container_elem_var_name}_cpy.mutate(move |pe: &mut PhantomElement| {{
                    // TODO: Cleanup old children
                    pe.children = vec![];
                }});
            }};
        }};

        {subscribes}
    ");

    ElementCode {
        creation_code,
        elem_var_name: container_elem_var_name,
    }
}

fn create_reactive_text(expr: &RustExpression, ctx: CodegenContext) -> ElementCode {
    let text_node = create_text_node("", ctx.clone());

    let text_node_elem = text_node.elem_var_name.clone();

    let (expr, clones, subscribes) =
        reactive_expression(expr, &text_node_elem, &"update_text".to_string());

    let creation_code = format!(
        "
        {text_node_creation_code}
       
        let {text_node_elem}_cpy = {text_node_elem}.clone();

        {clones}

        let update_text = move || {{
            {text_node_elem}_cpy.mutate(move |t: &mut Text| {{
                t.mutate_text(move |t: &mut String| {{
                    *t = format!(\"{{}}\", {expr});
                }})
            }});
        }};

        {subscribes}
        
        update_text();
    ",
        text_node_creation_code = text_node.creation_code
    );

    ElementCode {
        creation_code,
        elem_var_name: text_node.elem_var_name,
    }
}

struct ElementCode {
    /// The Rust code that creates the element(s) and assigns them to a variable(s).
    creation_code: String,

    /// Either the name of a single variable or multiple variables separated by commas.
    elem_var_name: String,
}

fn codegen_rules(styles: &Vec<css::Property>) -> String {
    let mut code = "let mut __s = Style::default();".to_string();

    for style in styles {
        match style.name.as_str() {
            "background-color" => {
                let col_hex = style.value[0].trim_start_matches("#");
                code.push_str(&format!("
                                    __s.set_property(\"background-color\", StyleProperty::BackgroundColour(c(\"{col_hex}\")));
                                "));
            }
            "border" => {
                let split = style.value[0].split_whitespace().collect::<Vec<_>>();
                let width = split[0];
                let col_hex = split[1].trim_start_matches("#");
                code.push_str(&format!("
                                    __s.set_property(\"border\", StyleProperty::Border {{ width: {width}, colour: c(\"{col_hex}\") }} );
                                "));
            }
            "color" => {
                let col_hex = style.value[0].trim_start_matches("#");
                code.push_str(&format!("
                                    __s.set_property(\"color\", StyleProperty::Color(c(\"{col_hex}\")));
                                "));
            }
            "font-size" => {
                code.push_str(&format!(
                    "
                                    __s.set_property(\"font-size\", StyleProperty::FontSize({}));
                                ",
                    style.value[0]
                ));
            }
            "font-family" => {
                code.push_str(&format!("
                                    __s.set_property(\"font-family\", StyleProperty::FontFamily(\"{}\".to_string()));
                                ", style.value[0]));
            }
            "alt" => {
                let width = style.value[0].clone();
                let offset = style.value[1].clone();
                let col_hex = style.value[2].trim_start_matches("#");

                code.push_str(&format!("
                                    __s.set_property(\"alt\", StyleProperty::Alt(Some(({}, {}, c(\"{}\")))));
                                ", width, offset, col_hex));
            }
            _ => todo!(),
        }
    }

    code
}

impl Element {
    fn rs_codegen(&self, ctx: CodegenContext) -> ElementCode {
        let elem_var_name = format!("__elem{}", uid());

        let (children_var_names, children_code) = codegen_children(&self.children, ctx.clone());

        let on_click = self
            .attributes
            .iter()
            .find(|a| a.name() == "onclick")
            .map(|a| match a {
                Attribute::Reactive(ReactiveAttribute { name: _, value }) => {
                    format!(
                        "
                        let bb = {elem_var_name}.borrow::<Element>().bounding_box.clone();
                        
                        let __onclick_callback = {value}.clone();
                        globals.subscriptions.subscribe_click_in_area(bb, rc_ref_cell(move |_, _| {{
                            __onclick_callback();
                        }}));
                    "
                    )
                }
                _ => "".to_string(),
            });

        // if let Some(class_list) = self.classes.as_ref() {
        //     match class_list {
        //         ClassList::Static(ref classes) => {
        //             let styles = ctx
        //                 .styles
        //                 .iter()
        //                 .filter(|r| r.selector.matches_classes(classes))
        //                 .map(|r| r.properties.clone())
        //                 .flatten()
        //                 .collect::<Vec<_>>();
        //         }
        //         ClassList::Reactive(_) => todo!(),
        //     }
        // }

        let mut classes_code = String::new();
        let mut update_class = String::new();

        if let Some(class_list) = self.classes.as_ref() {
            match class_list {
                ClassList::Static(classes) => {
                    classes_code = classes
                        .iter()
                        .map(|c| format!("\"{}\".to_string()", c))
                        .collect::<Vec<String>>()
                        .join(", ");
                }
                ClassList::Reactive(expr) => {
                    let (expr, clones, subs) =
                        reactive_expression(&expr, &elem_var_name, &"update_class".to_string());

                    update_class = format!("
                        {clones}
                        let {elem_var_name}_cpy = {elem_var_name}.clone();
                        let update_class = move || {{
                            {elem_var_name}_cpy.mutate(move |e: &mut Element| {{ 
                                e.class_list = ({expr}).split_whitespace().map(|s| s.to_string()).collect();
                            }});
                        }};
                        {subs}
                        update_class();
                    ");
                }
            }
        }

        let mut creation_code = format!(
            "
            {children_code}

            let {elem_var_name} = Element::new(
                gl,
                vec![{children_var_names}],
                p(20., 20.),
                d(20., 20.),
                vec![{classes_code}],
                frame_ref.clone(),
            );
            
            {update_class}
            "
        );

        if let Some(on_click) = on_click {
            creation_code.push_str(&on_click);
        }

        let bound_properties = vec![
            ("x", "properties.position.x"),
            ("y", "properties.position.y"),
            ("w", "properties.dimensions.width"),
            ("h", "properties.dimensions.height"),
        ];

        for (attr_name, property_name) in bound_properties {
            if let Some(attr) = self.attributes.iter().find(|a| a.name() == attr_name) {
                match attr {
                    Attribute::Static(StaticAttribute { name: _, value }) => {
                        let value = match value {
                            Some(v) => v.clone(),
                            None => "true".to_string(),
                        };

                        creation_code.push_str(&format!("\n{elem_var_name}.mutate(move |e: &mut Element| {{ e.{property_name} = {value};}});\n"));
                    }
                    Attribute::Reactive(ReactiveAttribute { name: _, value }) => {
                        let update_fn = format!("__attr_update_{}", uid());

                        let (expr, clones, subs) =
                            reactive_expression(value, &elem_var_name, &update_fn);

                        creation_code.push_str(&format!(
                            "
                            {clones}
                            let {elem_var_name}_cpy = {elem_var_name}.clone();
                            let {update_fn} = move || {{
                                {elem_var_name}_cpy.mutate(move |e: &mut Element| {{ e.{property_name} = {expr}; }});
                            }};
                            {subs}

                            {update_fn}();"));
                    }
                }
            }
        }

        ElementCode {
            creation_code,
            elem_var_name,
        }
    }
}

impl Component {
    fn rs_codegen(&self, ctx: CodegenContext) -> ElementCode {
        let (children_var_names, children_code) = codegen_children(&self.children, ctx);

        let mut props = String::new();

        for prop in self.props.iter() {
            let (name, value) = match prop {
                Attribute::Static(StaticAttribute { name, value }) => (
                    name,
                    value
                        .clone()
                        .map(|v| format!("\"{}\".to_string()", v))
                        .unwrap_or("true".to_string()),
                ),
                Attribute::Reactive(ReactiveAttribute { name, value }) => (name, value.clone()),
            };

            let name = format!("prop_{}", name);

            props.push_str(&format!("let {name} = ({value}).clone();\n"));
        }

        let creation_code = format!(
            " 
            {props}
            {children_code}
        "
        );

        ElementCode {
            creation_code,
            elem_var_name: children_var_names,
        }
    }
}

fn codegen_children(children: &Vec<Node>, ctx: CodegenContext) -> (String, String) {
    let children = children
        .iter()
        .map(|c| c.gl_codegen(ctx.clone()))
        .collect::<Vec<_>>();

    let children_code = children
        .iter()
        .map(|c| c.creation_code.clone())
        .collect::<Vec<_>>()
        .join("\n");

    let children_var_names = children
        .iter()
        .map(|c| c.elem_var_name.clone())
        .filter(|n| !n.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    (children_var_names, children_code)
}

// let bb = __elem1.borrow::<Element>().bounding_box.clone();
//
// globals.subscriptions.subscribe_click_in_area(bb, rc_ref_cell(move |_, _| {
//     add();
// }));
