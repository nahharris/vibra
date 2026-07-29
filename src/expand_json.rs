//! JSON view of a macro-expanded typed S-expression module.
//!
//! `vibra expand --format json` historically emitted one object entry per
//! top-level declaration. Keep that externally useful shape without routing
//! expanded syntax through the legacy YAML `Value` bridge.

use crate::ast::{Module, TopLevel};
use serde_json::{Map, Value};

/// Render expanded top-level forms as a deterministic JSON object.
///
/// Keys are the visible module names. Macro expansion has already happened in
/// `frontend::load_surface_program`, so generated declarations and hygienic
/// names appear here exactly as the typed frontend produced them.
pub fn module_json(module: &Module) -> Value {
    let mut output = Map::new();
    for form in &module.forms {
        let (name, kind) = match form {
            TopLevel::Import(import) => (&import.alias.value, "import"),
            TopLevel::Definition(definition) => (&definition.name.value, "definition"),
            TopLevel::Constant(constant) => (&constant.name.value, "constant"),
            TopLevel::Function(function) => (&function.name.value, "function"),
            TopLevel::Macro(mac) => (&mac.name.value, "macro"),
            TopLevel::Test(test) => (&test.name.value, "test"),
        };
        output.insert(name.clone(), Value::String(kind.to_string()));
    }
    Value::Object(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{self, DocumentId};
    use crate::syntax;

    #[test]
    fn preserves_visible_top_level_names_as_object_keys() {
        let document = syntax::parse("(const answer int64 42)\n").unwrap();
        let module = ast::lower_document_with_id(&document, DocumentId::ANONYMOUS).unwrap();
        assert_eq!(module_json(&module), serde_json::json!({"answer": "constant"}));
    }
}
