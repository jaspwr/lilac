use crate::{css::StyleSheet, job::Target, Node};

pub type CodegenResult = Result<String, String>;

impl Node {
    pub fn codegen(&self, target: Target, stylesheet: StyleSheet) -> CodegenResult {
        Ok(match target {
            Target::Unknown => return Err("Unknown target".to_string()),
            Target::Web => self.full_js_codegen(stylesheet)?,
            Target::GL => self.full_gl_codegen(stylesheet)?,
        })
    }
}

