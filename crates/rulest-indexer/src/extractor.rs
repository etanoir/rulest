use std::fs;
use std::path::Path;

use rulest_core::models::{SymbolKind, Visibility};
use syn::{
    visit::Visit, Fields, FnArg, ImplItem, ItemConst, ItemEnum, ItemFn, ItemImpl,
    ItemStatic, ItemStruct, ItemTrait, ItemType, ReturnType, TraitItem,
};

/// Extracted symbols from a single Rust source file.
pub struct ExtractedFile {
    pub symbols: Vec<ExtractedSymbol>,
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
    };
    visitor.visit_file(&syntax);

    Ok(ExtractedFile {
        symbols: visitor.symbols,
    })
}

struct SymbolVisitor {
    symbols: Vec<ExtractedSymbol>,
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
        // Extract methods from impl blocks
        for item in &node.items {
            if let ImplItem::Fn(method) = item {
                let vis = convert_visibility(&method.vis);
                let sig = format_fn_signature(&method.sig);

                let self_ty = quote_type(&node.self_ty);
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
