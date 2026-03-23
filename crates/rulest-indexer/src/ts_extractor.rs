//! TypeScript/TSX symbol extractor using swc_ecma_parser.

use std::fs;
use std::path::Path;

use swc_common::sync::Lrc;
use swc_common::{FileName, SourceMap};
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

use rulest_core::models::{SymbolKind, Visibility};

use crate::extractor::{ExtractedFile, ExtractedSymbol, Extractor};

/// Helper: get Ident sym as owned String.
fn ident_name(i: &Ident) -> String {
    i.sym.as_str().to_owned()
}

/// Helper: extract name from an expression (for implements/extends clauses).
fn expr_name(e: &Expr) -> String {
    match e {
        Expr::Ident(i) => ident_name(i),
        Expr::Member(m) => {
            let obj = expr_name(&m.obj);
            let prop = match &m.prop {
                MemberProp::Ident(i) => i.sym.as_str().to_owned(),
                _ => "?".to_string(),
            };
            format!("{}.{}", obj, prop)
        }
        _ => "?".to_string(),
    }
}

pub struct TsExtractor;

impl Extractor for TsExtractor {
    fn extract(&self, file_path: &Path) -> Result<ExtractedFile, String> {
        let source = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(Lrc::new(FileName::Real(file_path.to_path_buf())), source);

        let is_tsx = file_path.extension().is_some_and(|ext| ext == "tsx");

        let mut parser = Parser::new(
            Syntax::Typescript(TsSyntax {
                tsx: is_tsx,
                decorators: true,
                ..Default::default()
            }),
            StringInput::from(&*fm),
            None,
        );

        let module = parser
            .parse_module()
            .map_err(|e| format!("Failed to parse {}: {:?}", file_path.display(), e))?;

        let mut symbols = Vec::new();
        extract_module_items(&module.body, &mut symbols, &cm, None);

        Ok(ExtractedFile {
            symbols,
            trait_impls: Vec::new(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }
}

fn extract_module_items(
    items: &[ModuleItem],
    symbols: &mut Vec<ExtractedSymbol>,
    cm: &SourceMap,
    scope: Option<&str>,
) {
    for item in items {
        match item {
            ModuleItem::ModuleDecl(decl) => extract_module_decl(decl, symbols, cm, scope),
            ModuleItem::Stmt(stmt) => {
                if let Stmt::Decl(decl) = stmt {
                    extract_decl(decl, symbols, cm, scope, false);
                }
            }
        }
    }
}

fn extract_module_decl(
    decl: &ModuleDecl,
    symbols: &mut Vec<ExtractedSymbol>,
    cm: &SourceMap,
    scope: Option<&str>,
) {
    match decl {
        ModuleDecl::ExportDecl(export) => {
            extract_decl(&export.decl, symbols, cm, scope, true);
        }
        ModuleDecl::ExportDefaultDecl(export) => {
            let line = line_number(cm, export.span);
            match &export.decl {
                DefaultDecl::Fn(f) => {
                    let name = f.ident.as_ref().map(|i| ident_name(i)).unwrap_or_else(|| "default".to_string());
                    let sig = format_fn_sig(&name, &f.function);
                    symbols.push(ExtractedSymbol {
                        name, kind: SymbolKind::Function, visibility: Visibility::Public,
                        signature: Some(sig), line_number: line, scope: scope.map(|s| s.to_string()),
                    });
                }
                DefaultDecl::Class(c) => {
                    let name = c.ident.as_ref().map(|i| ident_name(i)).unwrap_or_else(|| "default".to_string());
                    let sig = format_class_sig(&name, &c.class);
                    symbols.push(ExtractedSymbol {
                        name: name.clone(), kind: SymbolKind::Class, visibility: Visibility::Public,
                        signature: Some(sig), line_number: line, scope: scope.map(|s| s.to_string()),
                    });
                    extract_class_members(&c.class, symbols, cm, &name);
                }
                DefaultDecl::TsInterfaceDecl(i) => {
                    symbols.push(ExtractedSymbol {
                        name: i.id.sym.as_str().to_owned(), kind: SymbolKind::Interface, visibility: Visibility::Public,
                        signature: Some(format!("interface {}", i.id.sym.as_str())),
                        line_number: line, scope: scope.map(|s| s.to_string()),
                    });
                }
            }
        }
        ModuleDecl::ExportNamed(named) => {
            for spec in &named.specifiers {
                if let ExportSpecifier::Named(n) = spec {
                    let exported = n.exported.as_ref().map(|e| match e {
                        ModuleExportName::Ident(i) => ident_name(i),
                        ModuleExportName::Str(s) => s.value.as_str().unwrap_or_default().to_owned(),
                    }).unwrap_or_else(|| match &n.orig {
                        ModuleExportName::Ident(i) => ident_name(i),
                        ModuleExportName::Str(s) => s.value.as_str().unwrap_or_default().to_owned(),
                    });
                    let orig = match &n.orig {
                        ModuleExportName::Ident(i) => ident_name(i),
                        ModuleExportName::Str(s) => s.value.as_str().unwrap_or_default().to_owned(),
                    };
                    let src = named.src.as_ref().map(|s| s.value.as_str().unwrap_or_default().to_owned());
                    let sig = if let Some(src) = src {
                        format!("export {{ {} }} from '{}'", orig, src)
                    } else if orig != exported {
                        format!("export {{ {} as {} }}", orig, exported)
                    } else {
                        format!("export {{ {} }}", orig)
                    };
                    symbols.push(ExtractedSymbol {
                        name: exported, kind: SymbolKind::ReExport, visibility: Visibility::Public,
                        signature: Some(sig), line_number: line_number(cm, n.span),
                        scope: scope.map(|s| s.to_string()),
                    });
                }
            }
        }
        ModuleDecl::ExportAll(all) => {
            symbols.push(ExtractedSymbol {
                name: "*".to_string(), kind: SymbolKind::ReExport, visibility: Visibility::Public,
                signature: Some(format!("export * from '{}'", all.src.value.as_str().unwrap_or_default())),
                line_number: line_number(cm, all.span), scope: scope.map(|s| s.to_string()),
            });
        }
        _ => {}
    }
}

fn extract_decl(
    decl: &Decl,
    symbols: &mut Vec<ExtractedSymbol>,
    cm: &SourceMap,
    scope: Option<&str>,
    is_exported: bool,
) {
    let vis = if is_exported { Visibility::Public } else { Visibility::Private };

    match decl {
        Decl::Fn(f) => {
            let name = f.ident.sym.as_str().to_owned();
            let sig = format_fn_sig(&name, &f.function);
            symbols.push(ExtractedSymbol {
                name, kind: SymbolKind::Function, visibility: vis,
                signature: Some(sig), line_number: line_number(cm, f.function.span),
                scope: scope.map(|s| s.to_string()),
            });
        }
        Decl::Class(c) => {
            let name = c.ident.sym.as_str().to_owned();
            let sig = format_class_sig(&name, &c.class);
            symbols.push(ExtractedSymbol {
                name: name.clone(), kind: SymbolKind::Class, visibility: vis,
                signature: Some(sig), line_number: line_number(cm, c.class.span),
                scope: scope.map(|s| s.to_string()),
            });
            extract_class_members(&c.class, symbols, cm, &name);
        }
        Decl::Var(var) => {
            for d in &var.decls {
                if let Pat::Ident(ident) = &d.name {
                    let name = ident.id.sym.as_str().to_owned();
                    let kind = if var.kind == VarDeclKind::Const { SymbolKind::Const } else { SymbolKind::Static };
                    let sig = format!("{} {}", var.kind.as_str(), name);
                    symbols.push(ExtractedSymbol {
                        name, kind, visibility: vis,
                        signature: Some(sig), line_number: line_number(cm, d.span),
                        scope: scope.map(|s| s.to_string()),
                    });
                }
            }
        }
        Decl::TsInterface(i) => {
            let extends = if i.extends.is_empty() {
                String::new()
            } else {
                let bases: Vec<String> = i.extends.iter().map(|e| expr_name(&e.expr)).collect();
                format!(" extends {}", bases.join(", "))
            };
            symbols.push(ExtractedSymbol {
                name: i.id.sym.as_str().to_owned(), kind: SymbolKind::Interface, visibility: vis,
                signature: Some(format!("interface {}{}", i.id.sym.as_str(), extends)),
                line_number: line_number(cm, i.span), scope: scope.map(|s| s.to_string()),
            });
        }
        Decl::TsTypeAlias(t) => {
            symbols.push(ExtractedSymbol {
                name: t.id.sym.as_str().to_owned(), kind: SymbolKind::TypeAlias, visibility: vis,
                signature: Some(format!("type {}", t.id.sym.as_str())),
                line_number: line_number(cm, t.span), scope: scope.map(|s| s.to_string()),
            });
        }
        Decl::TsEnum(e) => {
            let members: Vec<String> = e.members.iter().map(|m| match &m.id {
                TsEnumMemberId::Ident(i) => ident_name(i),
                TsEnumMemberId::Str(s) => format!("\"{}\"", s.value.as_str().unwrap_or_default()),
            }).collect();
            symbols.push(ExtractedSymbol {
                name: e.id.sym.as_str().to_owned(), kind: SymbolKind::Enum, visibility: vis,
                signature: Some(format!("enum {} {{ {} }}", e.id.sym.as_str(), members.join(", "))),
                line_number: line_number(cm, e.span), scope: scope.map(|s| s.to_string()),
            });
        }
        Decl::TsModule(m) => {
            let ns_name = match &m.id {
                TsModuleName::Ident(i) => ident_name(i),
                TsModuleName::Str(s) => s.value.as_str().unwrap_or_default().to_owned(),
            };
            if let Some(TsNamespaceBody::TsModuleBlock(block)) = &m.body {
                let ns_scope = match scope {
                    Some(s) => format!("{} > namespace {}", s, ns_name),
                    None => format!("namespace {}", ns_name),
                };
                extract_module_items(&block.body, symbols, cm, Some(&ns_scope));
            }
        }
        Decl::Using(_) => {}
    }
}

fn extract_class_members(class: &Class, symbols: &mut Vec<ExtractedSymbol>, cm: &SourceMap, class_name: &str) {
    let class_scope = format!("class {}", class_name);
    for member in &class.body {
        match member {
            ClassMember::Method(m) => {
                let name = prop_name_str(&m.key);
                let vis = match m.accessibility {
                    Some(Accessibility::Private) => Visibility::Private,
                    Some(Accessibility::Protected) => Visibility::CrateLocal,
                    _ => Visibility::Public,
                };
                let prefix = if m.is_static { "static " } else { "" };
                let async_prefix = if m.function.is_async { "async " } else { "" };
                symbols.push(ExtractedSymbol {
                    name, kind: SymbolKind::Function, visibility: vis,
                    signature: Some(format!("{}{}method", prefix, async_prefix)),
                    line_number: line_number(cm, m.span), scope: Some(class_scope.clone()),
                });
            }
            ClassMember::Constructor(c) => {
                symbols.push(ExtractedSymbol {
                    name: "constructor".to_string(), kind: SymbolKind::Function,
                    visibility: Visibility::Public, signature: Some("constructor".to_string()),
                    line_number: line_number(cm, c.span), scope: Some(class_scope.clone()),
                });
            }
            _ => {}
        }
    }
}

fn line_number(cm: &SourceMap, span: swc_common::Span) -> Option<u32> {
    if span.lo.is_dummy() { return None; }
    let loc = cm.lookup_char_pos(span.lo);
    Some(loc.line as u32)
}

fn format_fn_sig(name: &str, f: &Function) -> String {
    let async_prefix = if f.is_async { "async " } else { "" };
    let params: Vec<String> = f.params.iter().enumerate().map(|(i, p)| match &p.pat {
        Pat::Ident(ident) => ident.id.sym.as_str().to_owned(),
        Pat::Rest(r) => match &*r.arg {
            Pat::Ident(i) => format!("...{}", i.id.sym.as_str()),
            _ => format!("...arg{}", i),
        },
        _ => format!("arg{}", i),
    }).collect();
    format!("{}function {}({})", async_prefix, name, params.join(", "))
}

fn format_class_sig(name: &str, class: &Class) -> String {
    let mut sig = format!("class {}", name);
    if let Some(super_class) = &class.super_class {
        if let Expr::Ident(ident) = &**super_class {
            sig.push_str(&format!(" extends {}", ident.sym.as_str()));
        }
    }
    if !class.implements.is_empty() {
        let impls: Vec<String> = class.implements.iter().map(|i| expr_name(&i.expr)).collect();
        sig.push_str(&format!(" implements {}", impls.join(", ")));
    }
    sig
}

fn prop_name_str(key: &PropName) -> String {
    match key {
        PropName::Ident(i) => i.sym.as_str().to_owned(),
        PropName::Str(s) => s.value.as_str().unwrap_or_default().to_owned(),
        PropName::Num(n) => n.value.to_string(),
        PropName::Computed(_) => "[computed]".to_string(),
        PropName::BigInt(b) => b.value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn extract_from_source(source: &str, ext: &str) -> ExtractedFile {
        let mut file = NamedTempFile::with_suffix(ext).unwrap();
        file.write_all(source.as_bytes()).unwrap();
        TsExtractor.extract(file.path()).unwrap()
    }

    #[test]
    fn test_extract_ts_function() {
        let e = extract_from_source("export function greet(name: string): string { return name; }", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "greet");
        assert_eq!(e.symbols[0].kind, SymbolKind::Function);
        assert_eq!(e.symbols[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_ts_class() {
        let e = extract_from_source(
            "export class Greeter implements IGreeter { constructor(private prefix: string) {} greet(name: string): string { return this.prefix + name; } }",
            ".ts",
        );
        let class = e.symbols.iter().find(|s| s.name == "Greeter").expect("Should find Greeter");
        assert_eq!(class.kind, SymbolKind::Class);
        assert!(class.signature.as_ref().unwrap().contains("implements IGreeter"));
        let method = e.symbols.iter().find(|s| s.name == "greet" && s.scope.as_deref() == Some("class Greeter"));
        assert!(method.is_some(), "Should find greet method with class scope");
    }

    #[test]
    fn test_extract_ts_interface() {
        let e = extract_from_source("export interface Greeter { greet(name: string): string; }", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "Greeter");
        assert_eq!(e.symbols[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_ts_enum() {
        let e = extract_from_source("export enum Mood { Happy, Sad, Neutral }", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "Mood");
        assert_eq!(e.symbols[0].kind, SymbolKind::Enum);
        assert!(e.symbols[0].signature.as_ref().unwrap().contains("Happy"));
    }

    #[test]
    fn test_extract_ts_type_alias() {
        let e = extract_from_source("export type MoodMap = Record<string, number>;", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "MoodMap");
        assert_eq!(e.symbols[0].kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_ts_const() {
        let e = extract_from_source("export const MAX_SIZE: number = 100;", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "MAX_SIZE");
        assert_eq!(e.symbols[0].kind, SymbolKind::Const);
    }

    #[test]
    fn test_extract_ts_reexport() {
        let e = extract_from_source("export { Foo as Bar } from './foo';", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "Bar");
        assert_eq!(e.symbols[0].kind, SymbolKind::ReExport);
    }

    #[test]
    fn test_extract_ts_private() {
        let e = extract_from_source("function internalHelper(): void {}", ".ts");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_tsx() {
        let e = extract_from_source("export function App(): JSX.Element { return <div>Hello</div>; }", ".tsx");
        assert_eq!(e.symbols.len(), 1);
        assert_eq!(e.symbols[0].name, "App");
        assert_eq!(e.symbols[0].kind, SymbolKind::Function);
    }
}
