use std::{ops::DerefMut, sync::atomic::Ordering};

use crate::{
    js_component_scoping::ComponentVariableRenamer, utils::find_and_replace_js_identifiers,
    Attribute, Component, Element, JSExpression, Node, ReactiveAttribute, ScriptTag,
    StaticAttribute, ID_COUNTER,
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

impl Node {
    pub fn full_codegen(&self) -> String {
        let mut root_cvr = CVR::new(&"root".to_string());

        let _type = CodegenType::HTML;

        self.codegen(&_type, &root_cvr)
    }

    fn codegen(&self, _type: &CodegenType, cvr: &CVR) -> String {
        match self {
            Node::Component(c) => c.codegen(_type, cvr),
            Node::Element(e) => e.codegen(_type, cvr),
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
                iteratable,
                children,
            } => loop_codegen(iterator_variable, iteratable, children, _type, cvr),
            Node::ConditionalElements {
                condition,
                children,
            } => conditional_elements_codegen(condition, children, _type, cvr),
            Node::ComponentHole { .. } => panic!("Component was not replaced."),
            Node::ScriptTag(st) => st.codegen(_type, cvr),
        }
    }
}

fn script_tag_to_element(tag: &ScriptTag, var_renamer: &CVR) -> Element {
    println!("Script tag to element");
    let code = var_renamer.process(&tag.code.clone());
    let attributes = tag.attributes.clone();

    let children = vec![Node::Text(code)];

    Element {
        name: "script".to_string(),
        attributes,
        children,
    }
}

impl ScriptTag {
    fn codegen(&self, _type: &CodegenType, cvr: &CVR) -> String {
        match _type {
            CodegenType::HTML => script_tag_to_element(self, cvr).codegen(_type, cvr),
            CodegenType::JSDom { .. } => {
                format!("{};\n", self.code.clone())
            }
        }
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
    fn codegen(&self, _type: &CodegenType, cvr: &CVR) -> String {
        match _type {
            CodegenType::HTML => self.html_codegen(_type, cvr),
            CodegenType::JSDom {
                parent_elem_var_name,
            } => self.jsdom_codegen(parent_elem_var_name, _type, cvr),
        }
    }

    fn html_codegen(&self, _type: &CodegenType, cvr: &CVR) -> String {
        assert!(_type.is_html());

        if has_reactive_attributes(&self.attributes) {
            let elem_var_name = format!(
                "__elem_reactive_html_{}",
                ID_COUNTER.fetch_add(1, Ordering::SeqCst)
            );
            let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
            let js = self.jsdom_codegen(&elem_var_name, _type, cvr);

            return format!(
                "<span id=\"{id}\"></span>
                <script>
                    const {elem_var_name} = document.getElementById(\"{id}\");
                    {js}
                </script>",
            );
        }

        let mut code = format!("<{}", self.name);
        for attr in &self.attributes {
            match attr {
                Attribute::Static(StaticAttribute { name, value }) => {
                    code.push_str(&format!(" {}=\"{}\"", name, value));
                }
                Attribute::Reactive(ReactiveAttribute { name, value }) => {
                    panic!();
                }
            }
        }
        code.push('>');
        for child in &self.children {
            code.push_str(&child.codegen(_type, cvr));
        }
        code.push_str(&format!("</{}>", self.name));

        code
    }

    fn jsdom_codegen(
        &self,
        parent_elem_var_name: &String,
        _type: &CodegenType,
        cvr: &CVR,
    ) -> String {
        let elem_var_name = format!("__elem{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

        let mut code = format!(
            "const {} = document.createElement(\"{}\");\n",
            elem_var_name, self.name
        );

        for attr in &self.attributes {
            handle_attr(attr, &elem_var_name, &mut code, cvr, &self.name, self.attributes.clone());
        }

        let _type = CodegenType::JSDom {
            parent_elem_var_name: elem_var_name.clone(),
        };

        for child in &self.children {
            code.push_str(&child.codegen(&_type, cvr));
        }

        code.push_str(&format!(
            "{}.appendChild({});\n",
            parent_elem_var_name, elem_var_name
        ));

        code
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
            code.push_str(&format!(
                "{}.setAttribute(\"{}\", \"{}\");\n",
                elem_var_name, name, value
            ));
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
    println!("Handling special attr: {}", name);
    if name == "onclick" {
        let value = cvr.process_no_declared(&value);
        let c = format!("{elem_var_name}.onclick = ({value});");
        code.push_str(&c);
        return true;
    } else if name == "bind" {
        let value = cvr.process_no_declared(&value);

        // TODO: error handling
        assert!(elem_name == "input");

        let input_type = attributes_all
            .iter()
            .find(|a| match a {
                Attribute::Static(StaticAttribute { name, value: _ }) => name == "type",
                _ => false,
            });

        let input_type = match input_type { 
            Some(Attribute::Static(StaticAttribute { name: _, value })) => value,
            _ => return false,
        };

        match input_type.as_str() {
            "text" => code.push_str(&format!(
                "{{
                const value = {value};
                if (value.__STATE === true) {{
                    const update_fn = () => ({elem_var_name}).value = value.value;
                    value.subscribe(update_fn);
                    update_fn();
                    {elem_var_name}.oninput = (e) => {{
                        value.set(() => e.target.value);
                    }};
                }}
            }}"
            )),
            "checkbox" => code.push_str(&format!(
                "{{
                const value = {value};
                if (value.__STATE === true) {{
                    const update_fn = () => ({elem_var_name}).checked = value.value;
                    value.subscribe(update_fn);
                    update_fn();
                    {elem_var_name}.onchange = (e) => {{
                        console.log(e.target.checked);
                        value.set(() => e.target.checked);
                    }};
                }}
            }}"
            )),
            _ => {}
        }

        return true;
    }

    false
}

impl Component {
    fn codegen(&self, _type: &CodegenType, cvr: &CVR) -> String {
        let child_cvr = CVR::new(&self.name);

        let props_set = self
            .props
            .iter()
            .map(|p| match p {
                Attribute::Static(StaticAttribute { name, value }) => {
                    format!("props.{} = \"{}\";", name, value)
                }
                Attribute::Reactive(ReactiveAttribute { name, value }) => {
                    let mut expr = value.to_string();

                    expr = cvr.process_no_declared(&expr);

                    format!("props.{} = {};", name, expr)
                }
            })
            .collect::<Vec<String>>()
            .join("\n");

        let mut props_set = if props_set.is_empty() {
            "".to_string()
        } else {
            format!("const props = {{}};\n{}", props_set)
        };

        if _type.is_html() {
            props_set = child_cvr.process(&props_set);
        }

        let inner: String = self
            .children
            .iter()
            .map(|c| c.codegen(_type, &child_cvr))
            .collect();

        match _type {
            CodegenType::HTML { .. } => {
                if props_set.is_empty() {
                    inner
                } else {
                    format!("<script>\n{props_set}\n</script>\n{inner}")
                }
            }
            CodegenType::JSDom { .. } => {
                format!(
                    "{{\n
                    {}
                    {}}}\n",
                    props_set, inner
                )
            }
        }
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

    for item in ress::Scanner::new(&expr) {
        if let ress::tokens::Token::Ident(ref name) = item.unwrap().token {
            let name = name.to_string();
            if !name.starts_with("$") {
                continue;
            }

            let name = name.trim_start_matches("$");
            reactive_deps.push(name.to_string());
        }
    }

    for dep in reactive_deps.iter() {
        expr = find_and_replace_js_identifiers(
            &expr,
            &format!("${}", dep),
            &format!("(({}).value)", dep),
        );
    }

    let subscriptions = reactive_deps
        .iter()
        .map(|dep| format!("({dep}).subscribe(() => ({update_fn})({expr}));",))
        .collect::<Vec<String>>()
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
) -> String {
    let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
    let elem_var_name = format!("__elem{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

    let child_type = CodegenType::JSDom {
        parent_elem_var_name: elem_var_name.clone(),
    };

    let children = children
        .iter()
        .map(|c| c.codegen(&child_type, cvr))
        .collect::<String>();

    let func = format!(
        "const {id}cond = (c) => {{
        if (c) {{
            {children}
        }} else {{
            {elem_var_name}.innerHTML = \"\";
        }}
    }};"
    );

    match _type {
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
    }
}

fn loop_codegen(
    iterator_variable: &String,
    iteratable: &JSExpression,
    children: &[Node],
    _type: &CodegenType,
    cvr: &CVR,
) -> String {
    let id = format!("r{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));
    let elem_var_name = format!("__elem{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst));

    let child_type = CodegenType::JSDom {
        parent_elem_var_name: elem_var_name.clone(),
    };

    let children = children
        .iter()
        .map(|c| c.codegen(&child_type, cvr))
        .collect::<String>();

    let func = format!(
        "const {id}loop = (arr) => {{
        {elem_var_name}.innerHTML = \"\";
        for (let __i = 0; __i < arr.length; __i++) {{
            const {iterator_variable} = arr[__i];
            {children}
        }}
    }};"
    );

    match _type {
        CodegenType::HTML => {
            let r = reactive_expression(iteratable, &format!("{}loop", id), cvr);

            format!(
                "<span id=\"{id}\"></span>
                <script>
                    const {elem_var_name} = document.getElementById(\"{id}\");
                    
                    {func}
                    {r} 
                </script>"
            )
        }
        CodegenType::JSDom {
            parent_elem_var_name,
        } => {
            let r = reactive_expression(iteratable, &format!("{}loop", id), cvr);

            format!(
                "
                const {elem_var_name} = document.createElement(\"span\");
                {func}
                {r}
                {parent_elem_var_name}.appendChild({elem_var_name});
                "
            )
        }
    }
}
