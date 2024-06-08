use std::{collections::HashMap, ops::DerefMut, sync::atomic::Ordering};

use owo_colors::colors::xterm::PompadourMagenta;

use crate::{
    codegen::CodegenResult,
    css::{Rule, Selector, StyleSheet},
    js_component_scoping::ComponentVariableRenamer,
    parse::VOID_ELEMENTS,
    utils::find_and_replace_js_identifiers,
    Attribute, ClassList, Component, Dialect, Element, Id, JSExpression, Node, ReactiveAttribute,
    ScriptTag, StaticAttribute, ID_COUNTER,
};

type CVR = ComponentVariableRenamer;

enum CodegenType {
    HTML,
    JSDom { parent_elem_var_name: String },
}

impl CodegenType {
    fn is_html(&self) -> bool {
        match self {
            CodegenType::HTML { .. } => true,
            _ => false,
        }
    }
}

/// Recursive rename map
type RRM = HashMap<String, JSExpression>;

impl Node {
    pub fn full_js_codegen(&self, stylesheet: StyleSheet) -> Result<String, String> {
        let mut root_cvr = CVR::new(&"root".to_string());
        let rrm = RRM::new();

        let _type = CodegenType::HTML;

        let html = self.codegen_js(&_type, &root_cvr, rrm)?;

        Ok(format!(
            "<!DOCTYPE html>
<!-- This file was generated by Lilac v{}. -->
<head>
    <meta charset=\"UTF-8\">
</head>
<script>{}</script>
<style>{}</style>
{}",
            env!("CARGO_PKG_VERSION"),
            include_str!("../prelude.js"),
            codegen_stylesheet(&stylesheet),
            html
        ))
    }

    fn codegen_js(&self, _type: &CodegenType, cvr: &CVR, rrm: RRM) -> CodegenResult {
        Ok(match self {
            Node::Component(c) => c.codegen(_type, cvr, rrm)?,
            Node::Element(e) => e.codegen(_type, cvr, rrm)?,
            Node::Text(t) => match _type {
                CodegenType::HTML { .. } => t.clone(),
                CodegenType::JSDom {
                    parent_elem_var_name,
                } => {
                    let text_node_var_name =
                        format!("__textNode{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
                    format!(
                        "const {} = document.createTextNode(`{}`);\n",
                        text_node_var_name, t
                    ) + &format!(
                        "{}.appendChild({});\n",
                        parent_elem_var_name, text_node_var_name
                    )
                }
            },
            Node::ReactiveText(t) => reactive_text_codegen(t, _type, cvr),
            Node::Loop {
                iterator_variable,
                reactive_list,
                iteratable,
                children,
            } => loop_codegen(
                *reactive_list,
                iterator_variable,
                iteratable,
                children,
                _type,
                cvr,
                rrm,
            )?,
            Node::ConditionalElements {
                condition,
                children,
            } => conditional_elements_codegen(condition, children, _type, cvr, rrm)?,
            Node::ComponentHole { name, props, .. } => {
                // HACK: These should generally be removed but when a component instance has no
                // children supplied these get left behind.
                if name == "Children" {
                    return Ok("".to_string());
                }

                if let Some(create_fn_name) = rrm.get(name) {
                    let props_set = codegen_props_set(&props, cvr);

                    let parent_elem_var_name = match _type {
                        CodegenType::JSDom {
                            parent_elem_var_name,
                        } => parent_elem_var_name,
                        _ => panic!(),
                    };

                    format!("{{ {props_set};\n {create_fn_name}(props, {parent_elem_var_name}); }}")
                } else {
                    panic!("Component {} not replaced.", name);
                }
            }
            Node::ScriptTag(st) => st.codegen(_type, cvr, rrm)?,
            Node::StyleTag(_) => unreachable!(),
        })
    }
}

fn script_tag_to_element(tag: &ScriptTag, var_renamer: &CVR) -> Element {
    let code = var_renamer.process(&tag.code.clone());
    let attributes = tag.attributes.clone();

    let children = vec![Node::Text(code)];

    Element {
        name: "script".to_string(),
        id: None,
        classes: None,
        attributes,
        children,
    }
}

impl ScriptTag {
    fn codegen(&self, _type: &CodegenType, cvr: &CVR, rrm: RRM) -> CodegenResult {
        Ok(match _type {
            CodegenType::HTML => script_tag_to_element(self, cvr).codegen(_type, cvr, rrm)?,
            CodegenType::JSDom { .. } => {
                format!("{};\n", self.code.clone())
            }
        })
    }
}

fn has_reactive_attributes(attrs: &Vec<Attribute>) -> bool {
    for attr in attrs {
        if attr.is_reactive() {
            return true;
        }
    }

    false
}

impl Element {
    fn codegen(&self, _type: &CodegenType, cvr: &CVR, rrm: RRM) -> Result<String, String> {
        if self.name == "" {
            return Ok(self
                .children
                .iter()
                .map(|c| c.codegen_js(_type, cvr, rrm.clone()))
                .collect::<CodegenResult>()?);
        }

        Ok(match _type {
            CodegenType::HTML => self.html_codegen(_type, cvr, rrm)?,
            CodegenType::JSDom {
                parent_elem_var_name,
            } => self.jsdom_codegen(parent_elem_var_name, _type, cvr, rrm)?,
        })
    }

    fn html_codegen(&self, _type: &CodegenType, cvr: &CVR, rrm: RRM) -> CodegenResult {
        assert!(_type.is_html());

        if has_reactive_attributes(&self.attributes)
            || self.id.as_ref().map(|id| id.is_reactive()).unwrap_or(false)
            || self
                .classes
                .as_ref()
                .map(|classes| classes.is_reactive())
                .unwrap_or(false)
        {
            let elem_var_name = format!(
                "__elem_reactive_html_{}",
                ID_COUNTER.fetch_add(1, Ordering::SeqCst)
            );
            let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
            let js = self.jsdom_codegen(&elem_var_name, _type, cvr, rrm)?;

            return Ok(format!(
                "<span id=\"{id}\"></span>
                <script>
                    const {elem_var_name} = document.getElementById(\"{id}\");
                    {js}
                </script>",
            ));
        }

        let mut code = format!("<{}", self.name);

        if let Some(id) = &self.id {
            match id {
                Id::Static(id) => {
                    code.push_str(&format!(" id=\"{}\"", id));
                }
                Id::Reactive(_) => {
                    panic!("Should have fallen back to JS Dom codegen.");
                }
            }
        }

        if let Some(classes) = &self.classes {
            match classes {
                ClassList::Static(classes) => {
                    code.push_str(&format!(" class=\"{}\"", classes.join(" ")));
                }
                ClassList::Reactive(_) => {
                    panic!("Should have fallen back to JS Dom codegen.");
                }
            }
        }

        for attr in &self.attributes {
            match attr {
                Attribute::Static(StaticAttribute { name, value }) => {
                    if let Some(value) = value {
                        code.push_str(&format!(" {}=\"{}\"", name, value));
                    } else {
                        code.push_str(&format!(" {}", name));
                    }
                }
                Attribute::Reactive(ReactiveAttribute { name, value }) => {
                    panic!();
                }
            }
        }
        code.push('>');

        if VOID_ELEMENTS.contains(&self.name.as_str()) {
            return Ok(code);
        }

        for child in &self.children {
            code.push_str(&child.codegen_js(_type, cvr, rrm.clone())?);
        }
        code.push_str(&format!("</{}>", self.name));

        Ok(code)
    }

    fn jsdom_codegen(
        &self,
        parent_elem_var_name: &String,
        _type: &CodegenType,
        cvr: &CVR,
        rrm: RRM,
    ) -> CodegenResult {
        let elem_var_name = format!("__elem{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

        let mut code = format!(
            "const {} = document.createElement(\"{}\");\n",
            elem_var_name, self.name
        );

        if let Some(id) = &self.id {
            match id {
                Id::Static(id) => {
                    code.push_str(&format!("{elem_var_name}.id = \"{id}\";\n"));
                }
                Id::Reactive(expr) => {
                    let update_fn_var_name =
                        format!("__{}update_id", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
                    let r = reactive_expression(&expr, &update_fn_var_name, cvr);
                    code.push_str(&format!(
                        "const {update_fn_var_name} = (val) => {elem_var_name}.id = val;\n{r}\n"
                    ));
                }
            }
        }

        if let Some(classes) = &self.classes {
            match classes {
                ClassList::Static(classes) => {
                    let classes = classes.join(" ");
                    code.push_str(&format!("{elem_var_name}.className = \"{classes}\";\n"));
                }
                ClassList::Reactive(expr) => {
                    let update_fn_var_name = format!(
                        "__{}update_classes",
                        ID_COUNTER.fetch_add(1, Ordering::SeqCst)
                    );
                    let r = reactive_expression(&expr, &update_fn_var_name, cvr);
                    code.push_str(&format!(
                        "const {update_fn_var_name} = (val) => {elem_var_name}.className = val;\n{r}\n"
                    ));
                }
            }
        }

        for attr in &self.attributes {
            handle_attr(
                attr,
                &elem_var_name,
                &mut code,
                cvr,
                &self.name,
                self.attributes.clone(),
            );
        }

        let _type = CodegenType::JSDom {
            parent_elem_var_name: elem_var_name.clone(),
        };

        for child in &self.children {
            code.push_str(&child.codegen_js(&_type, cvr, rrm.clone())?);
        }

        code.push_str(&format!(
            "{}.appendChild({});\n",
            parent_elem_var_name, elem_var_name
        ));

        Ok(code)
    }
}

fn handle_attr(
    attr: &Attribute,
    elem_var_name: &String,
    code: &mut String,
    cvr: &CVR,
    elem_name: &String,
    attributes_all: Vec<Attribute>,
) {
    match attr {
        Attribute::Static(StaticAttribute { name, value }) => {
            if let Some(value) = value {
                code.push_str(&format!(
                    "{elem_var_name}.setAttribute(\"{name}\", \"{value}\");\n",
                ));
            } else {
                code.push_str(&format!(
                    "{elem_var_name}.setAttribute(\"{name}\", \"\");\n",
                ));
            }
        }
        Attribute::Reactive(ReactiveAttribute { name, value }) => {
            if handle_special_attr(
                attr,
                elem_var_name,
                code,
                cvr,
                elem_name,
                attributes_all,
                name,
                value,
            ) {
                return;
            }

            let update_fn_var_name =
                format!("__{}update_attr", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

            let r = reactive_expression(value, &update_fn_var_name, cvr);

            code.push_str(&format!(
                        "const {update_fn_var_name} = (val) => {elem_var_name}.setAttribute(\"{name}\", val);\n{r}\n",
                    ));
        }
    }
}

fn handle_special_attr(
    attr: &Attribute,
    elem_var_name: &String,
    code: &mut String,
    cvr: &CVR,
    elem_name: &String,
    attributes_all: Vec<Attribute>,
    name: &String,
    value: &JSExpression,
) -> bool {
    if name == "onclick" {
        let value = cvr.process_no_declared(&value);
        let c = format!("{elem_var_name}.onclick = ({value});");
        code.push_str(&c);
        return true;
    } else if name == "bind" {
        let value = cvr.process_no_declared(&value);

        // TODO: error handling
        assert!(elem_name == "input");

        let input_type = attributes_all.iter().find(|a| match a {
            Attribute::Static(StaticAttribute { name, value: _ }) => name == "type",
            _ => false,
        });

        let input_type = match input_type {
            Some(Attribute::Static(StaticAttribute {
                name: _,
                value: Some(v),
            })) => v,
            _ => return false,
        };

        let not_state_err = throw_rt_error("bind attribute can only be used with a state type.");

        match input_type.as_str() {
            "text" => code.push_str(&format!(
                "{{
                const value = {value};
                if (value.__STATE === true) {{
                    const update_fn = () => ({elem_var_name}).value = value.get();
                    value.subscribe(update_fn);
                    update_fn();
                    {elem_var_name}.oninput = (e) => {{
                        value.set(() => e.target.value);
                    }};
                }} else {{
                    {not_state_err}
                }}
            }}"
            )),
            "checkbox" => code.push_str(&format!(
                "{{
                const value = {value};
                if (value.__STATE === true) {{
                    const update_fn = () => ({elem_var_name}).checked = value.get();
                    value.subscribe(update_fn);
                    update_fn();
                    {elem_var_name}.onchange = (e) => {{
                        value.set(() => e.target.checked);
                    }};
                }} else {{
                    {not_state_err}
                }}
            }}"
            )),
            _ => {}
        }

        return true;
    }

    false
}

fn throw_rt_error(message: &str) -> String {
    format!("throw new Error(\"[Lilac runtime error]: {}\")", message)
}

fn codegen_props_set(props: &Vec<Attribute>, cvr: &CVR) -> JSExpression {
    if props.is_empty() {
        return "".to_string();
    }

    let sets = props
        .iter()
        .map(|p| match p {
            Attribute::Static(StaticAttribute { name, value }) => {
                if let Some(value) = value {
                    format!("props.{} = \"{}\";", name, value)
                } else {
                    format!("props.{} = true;", name)
                }
            }
            Attribute::Reactive(ReactiveAttribute { name, value }) => {
                let mut expr = value.to_string();

                expr = cvr.process_no_declared(&expr);

                format!("props.{} = {};", name, expr)
            }
        })
        .collect::<Vec<String>>()
        .join("\n");

    format!("const props = {{}};\n{sets}\n")
}

impl Component {
    fn codegen(&self, _type: &CodegenType, cvr: &CVR, mut rrm: RRM) -> CodegenResult {
        if self.dialect != Dialect::JsLilac && self.dialect != Dialect::TsLilac {
            return Err(format!(
                "Unsupported dialect for component {}. Valid dialects for web are JsLilac and TsLilac. The target is determined by the dialect of the Root componenet",
                self.name
            ));
        }

        if self.recursive && _type.is_html() {
            return Err(format!(
                "Recursive component {} in non-conditional context.",
                self.name
            ));
        }

        let create_fn_name = format!(
            "__create{}{}",
            self.name,
            ID_COUNTER.fetch_add(1, Ordering::SeqCst)
        );

        if self.recursive {
            rrm.insert(self.name.clone(), create_fn_name.clone());
        }

        let mut props_set = codegen_props_set(&self.props, cvr);

        let child_cvr = CVR::new(&self.name);

        if _type.is_html() {
            props_set = child_cvr.process(&props_set);
        }

        let inner: String = self
            .children
            .iter()
            .map(|c| c.codegen_js(_type, &child_cvr, rrm.clone()))
            .collect::<CodegenResult>()?;

        Ok(match _type {
            CodegenType::HTML { .. } => {
                if props_set.is_empty() {
                    inner
                } else {
                    format!("<script>\n{props_set}\n</script>\n{inner}")
                }
            }
            CodegenType::JSDom {
                parent_elem_var_name,
            } => {
                if self.recursive {
                    format!(
                        "
                        const {create_fn_name} = (props, parent_elem) => {{
                            const {parent_elem_var_name} = parent_elem;
                            {inner}
                        }};
                        {{
                            {props_set};
                            {create_fn_name}(props, {parent_elem_var_name});
                        }}
                        "
                    )
                } else {
                    format!("{{\n{props_set};{inner}}}\n")
                }
            }
        })
    }
}

fn reactive_text_codegen(exp: &JSExpression, _type: &CodegenType, cvr: &CVR) -> String {
    let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

    let update_fn_var_name = format!("__{id}setTextNode");
    let text_node_var_name = format!("__{id}textNode");

    let r = reactive_expression(exp, &update_fn_var_name, cvr);

    let js = format!(
        "
    let {text_node_var_name} = document.createTextNode(``);
    const {update_fn_var_name} = (text) => {text_node_var_name}.nodeValue = text;
    {r}
    "
    );

    match _type {
        CodegenType::HTML { .. } => {
            format!(
                "<span id=\"{id}\"></span>
<script>
{js}
document.getElementById(\"{id}\").appendChild({text_node_var_name});
</script>"
            )
        }
        CodegenType::JSDom {
            parent_elem_var_name,
        } => {
            format!(
                "{js}
{parent_elem_var_name}.appendChild({text_node_var_name});"
            )
        }
    }
}

fn reactive_expression(expr: &JSExpression, update_fn: &JSExpression, cvr: &CVR) -> JSExpression {
    let mut expr = expr.to_string();

    expr = cvr.process_no_declared(&expr);

    let mut reactive_deps = vec![];

    let mut last_name = "".to_string();
    let mut namespace = "".to_string();

    for item in ress::Scanner::new(&expr) {
        if let ress::tokens::Token::Ident(ref name) = item.clone().unwrap().token {
            let name = name.to_string();
            last_name = name.clone();

            if last_name.starts_with("$") {
                last_name = format!("{}.get()", last_name.trim_start_matches("$").to_string());
            }

            if !name.starts_with("$") {
                continue;
            }

            let name = name.trim_start_matches("$");
            reactive_deps.push((name.to_string(), namespace));
            namespace = "".to_string();
        } else if let ress::tokens::Token::Punct(ref p) = item.unwrap().token {
            if p == &ress::tokens::Punct::Period {
                namespace = format!("{}{}.", namespace, last_name);
            } else {
                namespace = "".to_string();
            }
        } else {
            namespace = "".to_string();
        }
    }

    for (dep, _) in reactive_deps.iter() {
        expr =
            find_and_replace_js_identifiers(&expr, &format!("${}", dep), &format!("{}.get()", dep));
    }

    let subscriptions = reactive_deps
        .iter()
        .map(|(dep, namespace)| {
            let not_state_err = throw_rt_error(format!("{namespace}${dep} can only be used with a state type.").as_str());
            format!("if (({namespace}{dep}).__STATE !== true) {not_state_err};\n ({namespace}{dep}).subscribe(() => ({update_fn})({expr}));",)
        }).collect::<Vec<String>>()
        .join("\n");

    format!(
        "
        {subscriptions}
        ({update_fn})({expr}); 
        ",
    )
}

fn conditional_elements_codegen(
    condition: &JSExpression,
    children: &[Node],
    _type: &CodegenType,
    cvr: &CVR,
    rrm: RRM,
) -> CodegenResult {
    let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
    let elem_var_name = format!("__elem{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

    let child_type = CodegenType::JSDom {
        parent_elem_var_name: elem_var_name.clone(),
    };

    let children = children
        .iter()
        .map(|c| c.codegen_js(&child_type, cvr, rrm.clone()))
        .collect::<CodegenResult>()?;

    let func = format!(
        "const {id}cond = (c) => {{
        {elem_var_name}.innerHTML = \"\";
        if (c) {{
            {children}
        }}    
    }};"
    );

    Ok(match _type {
        CodegenType::HTML => {
            let r = reactive_expression(condition, &format!("{}cond", id), cvr);

            format!(
                "<span id=\"{id}\"></span>
<script>
    const {elem_var_name} = document.getElementById(\"{id}\");
    {func}
    {r} 
</script>
"
            )
        }
        CodegenType::JSDom {
            parent_elem_var_name,
        } => {
            let r = reactive_expression(condition, &format!("{}cond", id), cvr);

            format!(
                "
const {elem_var_name} = document.createElement(\"span\");
{func}
{r}
{parent_elem_var_name}.appendChild({elem_var_name});
"
            )
        }
    })
}

fn loop_codegen(
    reactive_list: bool,
    iterator_variable: &String,
    iteratable: &JSExpression,
    children: &[Node],
    _type: &CodegenType,
    cvr: &CVR,
    rrm: RRM,
) -> CodegenResult {
    let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
    let elem_var_name = format!("__elem{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

    let child_type = CodegenType::JSDom {
        parent_elem_var_name: elem_var_name.clone(),
    };

    let children = if reactive_list {
        Node::Element(Element {
            name: "".to_string(),
            id: None,
            classes: None,
            attributes: vec![],
            children: children.to_vec(),
        })
        .codegen_js(&child_type, cvr, rrm)?
    } else {
        children
            .into_iter()
            .map(|c| c.codegen_js(&child_type, cvr, rrm.clone()))
            .collect::<CodegenResult>()?
    };

    let func = format!(
        "const {id}loop = (arr) => {{
        {elem_var_name}.innerHTML = \"\";
        for (let __i = 0; __i < arr.length; __i++) {{
            const {iterator_variable} = arr[__i];
            {children}
        }}
    }};"
    );

    let not_lstate_err = throw_rt_error("$lstate can only be used with an lstate type.");

    let instantiate_and_subscribe = if reactive_list {
        let list = cvr.process_no_declared(&iteratable);
        format!(
            "
            if ({list}.__LSTATE !== true) {not_lstate_err}

            {list}.subscribeAdd((value, position) => {{
                const {iterator_variable} = value;
                let wrapper_span = undefined;
                {{
                    const {elem_var_name} = document.createElement(\"span\");
                    {children}
                    wrapper_span = {elem_var_name};
                }}
                if ({elem_var_name}.childNodes.length === 0) {{
                    {elem_var_name}.appendChild(wrapper_span);
                }} else {{
                    {elem_var_name}.insertBefore(wrapper_span, {elem_var_name}.childNodes[position]);
                }}
            }});

            {list}.subscribeRemove((position) => {{
                {elem_var_name}.removeChild({elem_var_name}.childNodes[position]);
            }});

            {id}loop({list}.get());
        ",
        )
    } else {
        reactive_expression(iteratable, &format!("{}loop", id), cvr)
    };

    Ok(match _type {
        CodegenType::HTML => {
            format!(
                "<span id=\"{id}\"></span>
                <script>
                    const {elem_var_name} = document.getElementById(\"{id}\");
                    
                    {func}
                    {instantiate_and_subscribe} 
                </script>"
            )
        }
        CodegenType::JSDom {
            parent_elem_var_name,
        } => {
            format!(
                "
                const {elem_var_name} = document.createElement(\"span\");
                {func}
                {instantiate_and_subscribe}
                {parent_elem_var_name}.appendChild({elem_var_name});
                "
            )
        }
    })
}

pub fn codegen_stylesheet(ss: &Vec<Rule>) -> String {
    ss.iter().map(|r| r.codegen()).collect()
}

impl Rule {
    fn codegen(&self) -> String {
        let mut code = format!("{} {{", self.selector.codegen());

        for prop in &self.properties {
            code.push_str(&format!("{}:{};", prop.name, prop.value.join(" ")));
        }

        code.push_str("} ");

        code
    }
}

impl Selector {
    fn codegen(&self) -> String {
        match self {
            Selector::None => unreachable!(),
            Selector::All => "*".to_string(),
            Selector::ID(id) => format!("#{}", id),
            Selector::Class(class) => format!(".{}", class),
            Selector::Tag(tag) => tag.clone(),
            Selector::Pseduo(pseudo) => format!(":{}", pseudo),
            Selector::Descendant(a, b) => format!("{} {}", a.codegen(), b.codegen()),
            Selector::Child(a, b) => format!("{} > {}", a.codegen(), b.codegen()),
            Selector::NextSibling(a, b) => format!("{} + {}", a.codegen(), b.codegen()),
        }
    }
}
