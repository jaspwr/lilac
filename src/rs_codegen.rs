use crate::{codegen::CodegenResult, Node};


impl Node {
    pub fn full_gl_codegen(&self) -> CodegenResult {
        println!("{:#?}", self);

        let ctx = CodegenContext {
            parent_element_var_name: "window".to_string(),
        };

        let rs = self.gl_codegen(ctx)?;

        Ok(rs)
    }

    fn gl_codegen(&self, ctx: CodegenContext) -> CodegenResult {
        Ok(match self {
            Node::Component(c) => c.codegen(ctx),
            Node::Element(e) => e.codegen(ctx),
            Node::Text(t) => create_text_node(t, ctx),
            _ => "".to_string(),
        })
    }
}

#[derive(Debug, Clone)]
struct CodegenContext {
    parent_element_var_name: String,
}

fn create_text_node(text: &str, ctx: CodegenContext) -> String {
    format!("\"{}\"", text)
}


