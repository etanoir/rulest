use std::fs;
use std::path::Path;

use rulest_core::models::{SymbolKind, Visibility};
use syn::{
    visit::Visit, Fields, FnArg, ImplItem, ItemConst, ItemEnum, ItemFn, ItemImpl,
    ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUse, ReturnType, TraitItem,
    UseTree,
};

/// Extracted symbols from a single Rust source file.
pub struct ExtractedFile {
    pub symbols: Vec<ExtractedSymbol>,
    /// Trait implementation relationships: `(trait_name, type_name)` pairs.
    pub trait_impls: Vec<(String, String)>,
}

/// A symbol extracted from source code (not yet assigned a module_id).
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub signature: Option<String>,
}

/// Parse a Rust source file and extract all symbols.
pub fn extract_symbols(file_path: &Path) -> Result<ExtractedFile, String> {
    let source = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

    let syntax = syn::parse_file(&source)
        .map_err(|e| format!("Failed to parse {}: {}", file_path.display(), e))?;

    let mut visitor = SymbolVisitor {
        symbols: Vec::new(),
        trait_impls: Vec::new(),
    };
    visitor.visit_file(&syntax);

    Ok(ExtractedFile {
        symbols: visitor.symbols,
        trait_impls: visitor.trait_impls,
    })
}

struct SymbolVisitor {
    symbols: Vec<ExtractedSymbol>,
    trait_impls: Vec<(String, String)>,
}

impl<'ast> Visit<'ast> for SymbolVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let vis = convert_visibility(&node.vis);
        let sig = format_fn_signature(&node.sig);

        self.symbols.push(ExtractedSymbol {
            name: node.sig.ident.to_string(),
            kind: SymbolKind::Function,
            visibility: vis,
            signature: Some(sig),
        });

        // Don't recurse into function bodies for nested items
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let vis = convert_visibility(&node.vis);
        let fields_str = match &node.fields {
            Fields::Named(f) => {
                let field_strs: Vec<String> = f
                    .named
                    .iter()
                    .map(|field| {
                        let name = field
                            .ident
                            .as_ref()
                            .map(|i| i.to_string())
                            .unwrap_or_default();
                        let ty = quote_type(&field.ty);
                        format!("{}: {}", name, ty)
                    })
                    .collect();
                format!(" {{ {} }}", field_strs.join(", "))
            }
            Fields::Unnamed(f) => {
                let field_strs: Vec<String> =
                    f.unnamed.iter().map(|field| quote_type(&field.ty)).collect();
                format!("({})", field_strs.join(", "))
            }
            Fields::Unit => String::new(),
        };

        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Struct,
            visibility: vis,
            signature: Some(format!("struct {}{}", node.ident, fields_str)),
        });

        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let vis = convert_visibility(&node.vis);
        let variants: Vec<String> = node.variants.iter().map(|v| v.ident.to_string()).collect();

        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Enum,
            visibility: vis,
            signature: Some(format!("enum {} {{ {} }}", node.ident, variants.join(", "))),
        });

        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        let vis = convert_visibility(&node.vis);
        let methods: Vec<String> = node
            .items
            .iter()
            .filter_map(|item| {
                if let TraitItem::Fn(method) = item {
                    Some(format_fn_signature(&method.sig))
                } else {
                    None
                }
            })
            .collect();

        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Trait,
            visibility: vis,
            signature: Some(format!(
                "trait {} {{ {} }}",
                node.ident,
                methods.join("; ")
            )),
        });

        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        let vis = convert_visibility(&node.vis);
        let ty = quote_type(&node.ty);

        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::TypeAlias,
            visibility: vis,
            signature: Some(format!("type {} = {}", node.ident, ty)),
        });

        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        let vis = convert_visibility(&node.vis);
        let ty = quote_type(&node.ty);

        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Const,
            visibility: vis,
            signature: Some(format!("const {}: {}", node.ident, ty)),
        });

        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        let vis = convert_visibility(&node.vis);
        let ty = quote_type(&node.ty);

        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Static,
            visibility: vis,
            signature: Some(format!("static {}: {}", node.ident, ty)),
        });

        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let self_ty = quote_type(&node.self_ty);

        // Record trait implementation relationship
        if let Some((_, trait_path, _)) = &node.trait_ {
            let trait_name = trait_path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            self.trait_impls.push((trait_name, self_ty.clone()));
        }

        // Extract methods from impl blocks
        for item in &node.items {
            if let ImplItem::Fn(method) = item {
                let vis = convert_visibility(&method.vis);
                let sig = format_fn_signature(&method.sig);

                let _qualified_name = if let Some((_, trait_path, _)) = &node.trait_ {
                    let trait_name = trait_path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    format!("<{} as {}>::{}", self_ty, trait_name, method.sig.ident)
                } else {
                    format!("{}::{}", self_ty, method.sig.ident)
                };

                self.symbols.push(ExtractedSymbol {
                    name: method.sig.ident.to_string(),
                    kind: SymbolKind::Function,
                    visibility: vis,
                    signature: Some(sig),
                });
            }
        }

        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let vis = convert_visibility(&node.vis);

        // Only extract `pub use` and `pub(crate) use` re-exports
        if matches!(vis, Visibility::Public | Visibility::CrateLocal) {
            let paths = flatten_use_tree(&node.tree, "");
            for (name, full_path) in paths {
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind: SymbolKind::TypeAlias,
                    visibility: vis,
                    signature: Some(format!("pub use {}", full_path)),
                });
            }
        }

        syn::visit::visit_item_use(self, node);
    }
}

fn convert_visibility(vis: &syn::Visibility) -> Visibility {
    match vis {
        syn::Visibility::Public(_) => Visibility::Public,
        syn::Visibility::Restricted(r) => {
            let path = r
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if path == "crate" {
                Visibility::CrateLocal
            } else {
                Visibility::Private
            }
        }
        syn::Visibility::Inherited => Visibility::Private,
    }
}

fn format_fn_signature(sig: &syn::Signature) -> String {
    let params: Vec<String> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(r) => {
                let mut s = String::new();
                if r.reference.is_some() {
                    s.push('&');
                    if r.mutability.is_some() {
                        s.push_str("mut ");
                    }
                }
                s.push_str("self");
                s
            }
            FnArg::Typed(t) => {
                let name = quote_pat(&t.pat);
                let ty = quote_type(&t.ty);
                format!("{}: {}", name, ty)
            }
        })
        .collect();

    let ret = match &sig.output {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => format!(" -> {}", quote_type(ty)),
    };

    let async_prefix = if sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };

    format!("{}fn {}({}){}", async_prefix, sig.ident, params.join(", "), ret)
}

fn quote_type(ty: &syn::Type) -> String {
    // Use token stream to get a reasonable string representation
    quote::quote!(#ty).to_string().replace(" ", "")
}

fn quote_pat(pat: &syn::Pat) -> String {
    quote::quote!(#pat).to_string()
}

/// Flatten a `UseTree` into a list of `(symbol_name, full_path)` pairs.
fn flatten_use_tree(tree: &UseTree, prefix: &str) -> Vec<(String, String)> {
    match tree {
        UseTree::Path(p) => {
            let segment = p.ident.to_string();
            let new_prefix = if prefix.is_empty() {
                segment
            } else {
                format!("{}::{}", prefix, segment)
            };
            flatten_use_tree(&p.tree, &new_prefix)
        }
        UseTree::Name(n) => {
            let name = n.ident.to_string();
            let full_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}::{}", prefix, name)
            };
            vec![(name, full_path)]
        }
        UseTree::Rename(r) => {
            let original = r.ident.to_string();
            let alias = r.rename.to_string();
            let full_path = if prefix.is_empty() {
                original
            } else {
                format!("{}::{}", prefix, original)
            };
            vec![(alias, full_path)]
        }
        UseTree::Glob(_) => {
            // `pub use module::*` — record as a glob re-export
            let full_path = if prefix.is_empty() {
                "*".to_string()
            } else {
                format!("{}::*", prefix)
            };
            vec![("*".to_string(), full_path)]
        }
        UseTree::Group(g) => {
            g.items
                .iter()
                .flat_map(|sub| flatten_use_tree(sub, prefix))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: write source to a temp file, extract symbols, return the result.
    fn extract_from_source(source: &str) -> ExtractedFile {
        let mut path = std::env::temp_dir();
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("rulest_test_{}.rs", id));

        let mut file = std::fs::File::create(&path).expect("Failed to create temp file");
        file.write_all(source.as_bytes())
            .expect("Failed to write temp file");
        drop(file);

        let result = extract_symbols(&path).expect("Failed to extract symbols");

        // Clean up
        let _ = std::fs::remove_file(&path);

        result
    }

    #[test]
    fn test_extract_function() {
        let source = r#"pub fn foo(x: i32) -> bool { true }"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.visibility, Visibility::Public);

        let sig = sym.signature.as_ref().expect("signature should be present");
        assert!(sig.contains("fn foo"), "signature should contain 'fn foo', got: {}", sig);
        assert!(sig.contains("-> bool"), "signature should contain '-> bool', got: {}", sig);
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }
        "#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "Point");
        assert_eq!(sym.kind, SymbolKind::Struct);
        assert_eq!(sym.visibility, Visibility::Public);

        let sig = sym.signature.as_ref().expect("signature should be present");
        assert!(sig.contains("struct Point"), "signature should contain 'struct Point', got: {}", sig);
        assert!(sig.contains("x:"), "signature should contain field 'x:', got: {}", sig);
        assert!(sig.contains("y:"), "signature should contain field 'y:', got: {}", sig);
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
            pub enum Color {
                Red,
                Green,
                Blue,
            }
        "#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "Color");
        assert_eq!(sym.kind, SymbolKind::Enum);
        assert_eq!(sym.visibility, Visibility::Public);

        let sig = sym.signature.as_ref().expect("signature should be present");
        assert!(sig.contains("enum Color"), "signature should contain 'enum Color', got: {}", sig);
        assert!(sig.contains("Red"), "signature should contain variant 'Red', got: {}", sig);
        assert!(sig.contains("Green"), "signature should contain variant 'Green', got: {}", sig);
        assert!(sig.contains("Blue"), "signature should contain variant 'Blue', got: {}", sig);
    }

    #[test]
    fn test_extract_trait() {
        let source = r#"
            pub trait Greetable {
                fn greet(&self) -> String;
            }
        "#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "Greetable");
        assert_eq!(sym.kind, SymbolKind::Trait);
        assert_eq!(sym.visibility, Visibility::Public);

        let sig = sym.signature.as_ref().expect("signature should be present");
        assert!(sig.contains("trait Greetable"), "signature should contain 'trait Greetable', got: {}", sig);
        assert!(sig.contains("fn greet"), "signature should contain 'fn greet', got: {}", sig);
    }

    #[test]
    fn test_extract_impl_trait() {
        let source = r#"
            pub struct Foo;

            impl std::fmt::Display for Foo {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "Foo")
                }
            }
        "#;
        let extracted = extract_from_source(source);

        // Should have the struct Foo and the method fmt
        assert!(
            extracted.symbols.iter().any(|s| s.name == "Foo" && s.kind == SymbolKind::Struct),
            "Should extract struct Foo"
        );
        assert!(
            extracted.symbols.iter().any(|s| s.name == "fmt" && s.kind == SymbolKind::Function),
            "Should extract method fmt"
        );

        // Check trait_impls captures (Display, Foo)
        assert_eq!(extracted.trait_impls.len(), 1);
        let (trait_name, type_name) = &extracted.trait_impls[0];
        assert!(
            trait_name.contains("Display"),
            "trait_impls should capture Display, got: {}", trait_name
        );
        assert_eq!(type_name, "Foo", "trait_impls should capture type Foo, got: {}", type_name);
    }

    #[test]
    fn test_extract_pub_use() {
        let source = r#"pub use crate::module::Symbol;"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "Symbol");
        assert_eq!(sym.visibility, Visibility::Public);

        let sig = sym.signature.as_ref().expect("signature should be present");
        assert!(sig.contains("pub use"), "signature should contain 'pub use', got: {}", sig);
        assert!(sig.contains("crate::module::Symbol"), "signature should contain the full path, got: {}", sig);
    }
}
