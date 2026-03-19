use std::fs;
use std::path::Path;

use rulest_core::models::{SymbolKind, Visibility};
use syn::{
    visit::Visit, Fields, FnArg, ForeignItem, ImplItem, ItemConst, ItemEnum, ItemFn,
    ItemForeignMod, ItemImpl, ItemMacro, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemType, ItemUse, ReturnType, TraitItem, UseTree,
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
    pub line_number: Option<u32>,
    pub scope: Option<String>,
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
        scope_stack: Vec::new(),
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
    /// Scope stack for nested items (e.g., ["mod trading", "impl FeeCalculator"]).
    scope_stack: Vec<String>,
}

impl SymbolVisitor {
    /// Join the scope stack with ` > ` separator, returning `None` if empty.
    fn current_scope(&self) -> Option<String> {
        if self.scope_stack.is_empty() {
            None
        } else {
            Some(self.scope_stack.join(" > "))
        }
    }
}

impl<'ast> Visit<'ast> for SymbolVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let vis = convert_visibility(&node.vis);
        let line = node.sig.ident.span().start().line as u32;

        // Detect FFI functions: extern "C"/"system" ABI, #[no_mangle], or #[export_name]
        let has_ffi_abi = node.sig.abi.as_ref().is_some_and(|abi| {
            abi.name.as_ref().is_some_and(|lit| {
                let val = lit.value();
                val == "C" || val == "system"
            })
        });
        let is_ffi = has_ffi_abi
            || has_attribute(&node.attrs, "no_mangle")
            || has_attribute(&node.attrs, "export_name");

        let kind = if is_ffi {
            SymbolKind::FfiFunction
        } else {
            SymbolKind::Function
        };

        let sig = format_fn_signature(&node.sig);

        self.symbols.push(ExtractedSymbol {
            name: node.sig.ident.to_string(),
            kind,
            visibility: vis,
            signature: Some(sig),
            line_number: Some(line),
            scope: self.current_scope(),
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

        let line = node.ident.span().start().line as u32;
        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Struct,
            visibility: vis,
            signature: Some(format!("struct {}{}{}", node.ident, format_generics(&node.generics), fields_str)),
            line_number: Some(line),
            scope: self.current_scope(),
        });

        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let vis = convert_visibility(&node.vis);
        let variants: Vec<String> = node.variants.iter().map(|v| v.ident.to_string()).collect();

        let line = node.ident.span().start().line as u32;
        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Enum,
            visibility: vis,
            signature: Some(format!("enum {}{} {{ {} }}", node.ident, format_generics(&node.generics), variants.join(", "))),
            line_number: Some(line),
            scope: self.current_scope(),
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

        let line = node.ident.span().start().line as u32;
        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Trait,
            visibility: vis,
            signature: Some(format!(
                "trait {}{} {{ {} }}",
                node.ident,
                format_generics(&node.generics),
                methods.join("; ")
            )),
            line_number: Some(line),
            scope: self.current_scope(),
        });

        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        let vis = convert_visibility(&node.vis);
        let ty = quote_type(&node.ty);

        let line = node.ident.span().start().line as u32;
        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::TypeAlias,
            visibility: vis,
            signature: Some(format!("type {} = {}", node.ident, ty)),
            line_number: Some(line),
            scope: self.current_scope(),
        });

        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        let vis = convert_visibility(&node.vis);
        let ty = quote_type(&node.ty);

        let line = node.ident.span().start().line as u32;
        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Const,
            visibility: vis,
            signature: Some(format!("const {}: {}", node.ident, ty)),
            line_number: Some(line),
            scope: self.current_scope(),
        });

        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        let vis = convert_visibility(&node.vis);
        let ty = quote_type(&node.ty);

        let line = node.ident.span().start().line as u32;
        self.symbols.push(ExtractedSymbol {
            name: node.ident.to_string(),
            kind: SymbolKind::Static,
            visibility: vis,
            signature: Some(format!("static {}: {}", node.ident, ty)),
            line_number: Some(line),
            scope: self.current_scope(),
        });

        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        // Only extract named macros (macro_rules! foo { ... })
        if let Some(ref ident) = node.ident {
            let line = ident.span().start().line as u32;
            // macro_rules! are always pub-accessible if exported
            self.symbols.push(ExtractedSymbol {
                name: ident.to_string(),
                kind: SymbolKind::Macro,
                visibility: Visibility::Public, // macro visibility is determined by #[macro_export]
                signature: Some(format!("macro_rules! {}", ident)),
                line_number: Some(line),
                scope: self.current_scope(),
            });
        }
        syn::visit::visit_item_macro(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.scope_stack.push(format!("mod {}", node.ident));
        syn::visit::visit_item_mod(self, node);
        self.scope_stack.pop();
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

        // Compute scope for methods in this impl block
        let scope = if let Some((_, trait_path, _)) = &node.trait_ {
            let trait_name = trait_path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            format!("impl {} for {}", trait_name, self_ty)
        } else {
            format!("impl {}", self_ty)
        };

        // Push scope for methods in this impl block
        self.scope_stack.push(scope);

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

                let line = method.sig.ident.span().start().line as u32;
                self.symbols.push(ExtractedSymbol {
                    name: method.sig.ident.to_string(),
                    kind: SymbolKind::Function,
                    visibility: vis,
                    signature: Some(sig),
                    line_number: Some(line),
                    scope: self.current_scope(),
                });
            }
        }

        syn::visit::visit_item_impl(self, node);

        // Pop the impl scope
        self.scope_stack.pop();
    }

    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let vis = convert_visibility(&node.vis);

        // Only extract `pub use` and `pub(crate) use` re-exports
        if matches!(vis, Visibility::Public | Visibility::CrateLocal) {
            let paths = flatten_use_tree(&node.tree, "");
            for (name, full_path) in paths {
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind: SymbolKind::ReExport,
                    visibility: vis,
                    signature: Some(format!("pub use {}", full_path)),
                    line_number: None,
                    scope: self.current_scope(),
                });
            }
        }

        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_foreign_mod(&mut self, node: &'ast ItemForeignMod) {
        let abi_str = node.abi.name.as_ref().map(|lit| lit.value());

        for item in &node.items {
            if let ForeignItem::Fn(foreign_fn) = item {
                let vis = convert_visibility(&foreign_fn.vis);
                let line = foreign_fn.sig.ident.span().start().line as u32;

                let sig = if let Some(ref abi) = abi_str {
                    format!(
                        "extern \"{}\" {}",
                        abi,
                        format_fn_signature(&foreign_fn.sig)
                    )
                } else {
                    format!("extern {}", format_fn_signature(&foreign_fn.sig))
                };

                self.symbols.push(ExtractedSymbol {
                    name: foreign_fn.sig.ident.to_string(),
                    kind: SymbolKind::FfiFunction,
                    visibility: vis,
                    signature: Some(sig),
                    line_number: Some(line),
                    scope: self.current_scope(),
                });
            }
        }

        syn::visit::visit_item_foreign_mod(self, node);
    }
}

/// Check if a list of attributes contains an attribute with the given name.
fn has_attribute(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
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

fn format_generics(generics: &syn::Generics) -> String {
    if generics.params.is_empty() {
        return String::new();
    }
    let params = &generics.params;
    let generic_params = quote::quote!(#params).to_string();
    let where_part = if let Some(where_clause) = &generics.where_clause {
        format!(" {}", quote::quote!(#where_clause).to_string())
    } else {
        String::new()
    };
    format!("<{}>{}",  generic_params, where_part)
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

    let abi_prefix = match &sig.abi {
        Some(abi) => match &abi.name {
            Some(lit) => format!("extern \"{}\" ", lit.value()),
            None => "extern ".to_string(),
        },
        None => String::new(),
    };

    let generics = format_generics(&sig.generics);

    format!(
        "{}{}fn {}{}({}){}", abi_prefix, async_prefix, sig.ident, generics, params.join(", "), ret
    )
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
            // `pub use module::*` -- record as a glob re-export
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
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cnt = COUNTER.fetch_add(1, Ordering::SeqCst);
        path.push(format!("rulest_test_{}_{}.rs", id, cnt));

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

    #[test]
    fn test_extract_macro() {
        let source = r#"
            macro_rules! my_macro {
                ($x:expr) => { $x + 1 };
            }
        "#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "my_macro");
        assert_eq!(sym.kind, SymbolKind::Macro);
        assert_eq!(sym.visibility, Visibility::Public);

        let sig = sym.signature.as_ref().expect("signature should be present");
        assert!(sig.contains("macro_rules!"), "signature should contain 'macro_rules!', got: {}", sig);
    }

    #[test]
    fn test_extract_generic_function() {
        let source = r#"pub fn process<T: Clone + Send>(items: Vec<T>) -> Vec<T> { items }"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sym = &extracted.symbols[0];
        assert_eq!(sym.name, "process");
        let sig = sym.signature.as_ref().unwrap();
        assert!(sig.contains("<"), "signature should contain generics: {}", sig);
        assert!(sig.contains("Clone"), "signature should contain bound Clone: {}", sig);
        assert!(sig.contains("Send"), "signature should contain bound Send: {}", sig);
    }

    #[test]
    fn test_extract_generic_struct() {
        let source = r#"pub struct Wrapper<T: Clone> { pub inner: T }"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sig = extracted.symbols[0].signature.as_ref().unwrap();
        assert!(sig.contains("<"), "signature should contain generics: {}", sig);
        assert!(sig.contains("Clone"), "signature should contain bound Clone: {}", sig);
    }

    #[test]
    fn test_extract_generic_enum() {
        let source = r#"pub enum Option<T> { Some(T), None }"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sig = extracted.symbols[0].signature.as_ref().unwrap();
        assert!(sig.contains("<T>"), "signature should contain generics: {}", sig);
    }

    #[test]
    fn test_extract_generic_trait() {
        let source = r#"pub trait Convert<T> { fn convert(&self) -> T; }"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sig = extracted.symbols[0].signature.as_ref().unwrap();
        assert!(sig.contains("<T>"), "signature should contain generics: {}", sig);
    }

    #[test]
    fn test_extract_lifetime_function() {
        let source = r#"pub fn first<'a>(items: &'a [u32]) -> &'a u32 { &items[0] }"#;
        let extracted = extract_from_source(source);

        assert_eq!(extracted.symbols.len(), 1);
        let sig = extracted.symbols[0].signature.as_ref().unwrap();
        assert!(sig.contains("<"), "signature should contain lifetime: {}", sig);
        assert!(sig.contains("'a"), "signature should contain 'a: {}", sig);
    }

    #[test]
    fn test_extract_ffi_function() {
        let source = r#"
            #[no_mangle]
            pub extern "C" fn my_ffi_func(x: i32) -> i32 { x }
        "#;
        let extracted = extract_from_source(source);
        assert_eq!(extracted.symbols.len(), 1);
        assert_eq!(extracted.symbols[0].kind, SymbolKind::FfiFunction);
        let sig = extracted.symbols[0].signature.as_ref().unwrap();
        assert!(
            sig.contains("extern"),
            "signature should contain extern: {}",
            sig
        );
    }

    #[test]
    fn test_extract_ffi_no_mangle_only() {
        let source = r#"
            #[no_mangle]
            pub fn my_func(x: i32) -> i32 { x }
        "#;
        let extracted = extract_from_source(source);
        assert_eq!(extracted.symbols.len(), 1);
        assert_eq!(extracted.symbols[0].kind, SymbolKind::FfiFunction);
    }

    #[test]
    fn test_extract_ffi_export_name() {
        let source = r#"
            #[export_name = "custom_name"]
            pub fn my_func(x: i32) -> i32 { x }
        "#;
        let extracted = extract_from_source(source);
        assert_eq!(extracted.symbols.len(), 1);
        assert_eq!(extracted.symbols[0].kind, SymbolKind::FfiFunction);
    }

    #[test]
    fn test_extract_foreign_mod() {
        let source = r#"
            extern "C" {
                fn external_func(x: i32) -> i32;
            }
        "#;
        let extracted = extract_from_source(source);
        assert_eq!(extracted.symbols.len(), 1);
        assert_eq!(extracted.symbols[0].kind, SymbolKind::FfiFunction);
        let sig = extracted.symbols[0].signature.as_ref().unwrap();
        assert!(
            sig.contains("extern"),
            "signature should contain extern: {}",
            sig
        );
    }

    #[test]
    fn test_regular_function_not_ffi() {
        let source = r#"pub fn regular(x: i32) -> i32 { x }"#;
        let extracted = extract_from_source(source);
        assert_eq!(extracted.symbols.len(), 1);
        assert_eq!(extracted.symbols[0].kind, SymbolKind::Function);
    }
}
