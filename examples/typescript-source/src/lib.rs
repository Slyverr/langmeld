//! The TypeScript compiler parser adapted to Langmeld's generic analysis and tools.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

use langmeld::{
    core::{Diagnostic, Severity, SourceFile, Span, ValidatedLanguageSpec},
    language::{
        AstNode, CapturedValue, ConfiguredLanguageParser, CustomConstructMatch,
        CustomConstructParser, CustomLexer, LanguageToken, LanguageTokenKind, LexedSource,
        ParseResult, PreparedLanguage, Program, SyntaxNode, SyntaxNodeRole, Value,
        WorkspaceAnalysis, WorkspaceDiagnostic, WorkspaceTypeAnalysisHook, analyze,
        analyze_workspace_with_resolutions, parse_definition,
    },
};
use serde_json::Value as JsonValue;

const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_FRONTEND_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
struct TypeScriptModuleMetadata {
    imports: Vec<TypeScriptImport>,
    functions: Vec<TypeScriptExport>,
}

#[derive(Clone, Debug)]
struct TypeScriptImport {
    module_specifier: String,
    module_span: Option<Span>,
    imported_name: String,
    imported_span: Option<Span>,
}

#[derive(Clone, Debug)]
struct TypeScriptExport {
    name: String,
    exported: bool,
    default_exported: bool,
}

struct TypeScriptFrontend {
    bun: std::ffi::OsString,
    child: Option<Child>,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: Option<BufReader<ChildStdout>>,
}

impl TypeScriptFrontend {
    fn new() -> Self {
        Self {
            bun: std::env::var_os("LANGMELD_BUN").unwrap_or_else(|| "bun".into()),
            child: None,
            stdin: None,
            stdout: None,
        }
    }

    fn parse(&mut self, source: &SourceFile) -> Result<JsonValue, Diagnostic> {
        if source.text.len() > MAX_SOURCE_BYTES {
            return Err(Diagnostic::error(
                "TS1003",
                format!("TypeScript source exceeds the {MAX_SOURCE_BYTES} byte adapter limit"),
                Span::new(0, source.text.len()),
            ));
        }
        self.ensure_started()?;
        let request = serde_json::json!({ "source": source.text });
        let send_result = {
            let stdin = self.stdin.as_mut().expect("started frontend has stdin");
            serde_json::to_writer(&mut *stdin, &request)
                .and_then(|()| stdin.write_all(b"\n").map_err(serde_json::Error::io))
                .and_then(|()| stdin.flush().map_err(serde_json::Error::io))
        };
        if let Err(error) = send_result {
            return Err(self.worker_failure(&format!(
                "cannot send request to TypeScript frontend: {error}"
            )));
        }
        let mut line = Vec::new();
        let read_result = self
            .stdout
            .as_mut()
            .expect("started frontend has stdout")
            .take((MAX_FRONTEND_RESPONSE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line);
        let read = match read_result {
            Ok(read) => read,
            Err(error) => {
                return Err(self.worker_failure(&format!(
                    "cannot read TypeScript frontend response: {error}"
                )));
            }
        };
        if read == 0 {
            return Err(
                self.worker_failure("TypeScript frontend exited before returning a response")
            );
        }
        if line.len() > MAX_FRONTEND_RESPONSE_BYTES || line.last() != Some(&b'\n') {
            self.stop_worker();
            return Err(Diagnostic::spec_error(
                "TS1000",
                format!(
                    "TypeScript frontend response exceeds the {MAX_FRONTEND_RESPONSE_BYTES} byte limit or is incomplete"
                ),
            ));
        }
        let response: JsonValue =
            serde_json::from_slice(&line[..line.len() - 1]).map_err(|error| {
                self.stop_worker();
                Diagnostic::spec_error(
                    "TS1002",
                    format!("TypeScript frontend returned invalid JSON: {error}"),
                )
            })?;
        if let Some(error) = response.get("frontend_error").and_then(JsonValue::as_str) {
            return Err(Diagnostic::spec_error(
                "TS1002",
                format!("TypeScript frontend rejected request: {error}"),
            ));
        }
        Ok(response)
    }

    fn ensure_started(&mut self) -> Result<(), Diagnostic> {
        if self.child.is_some() {
            return Ok(());
        }
        let package_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new(&self.bun)
            .args(["run", "indexer.ts"])
            .current_dir(package_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                Diagnostic::spec_error(
                    "TS1000",
                    format!("cannot start Bun TypeScript frontend: {error}"),
                )
            })?;
        let stdin = child.stdin.take().expect("piped child stdin");
        let stdout = child.stdout.take().expect("piped child stdout");
        self.stdin = Some(BufWriter::new(stdin));
        self.stdout = Some(BufReader::new(stdout));
        self.child = Some(child);
        let mut ready = Vec::new();
        let stdout = self.stdout.as_mut().expect("frontend stdout was set");
        let read = match stdout.take(1024).read_until(b'\n', &mut ready) {
            Ok(read) => read,
            Err(error) => {
                self.stop_worker();
                return Err(Diagnostic::spec_error(
                    "TS1000",
                    format!("cannot initialize TypeScript frontend: {error}"),
                ));
            }
        };
        let response: JsonValue =
            match serde_json::from_slice(ready.strip_suffix(b"\n").unwrap_or(&ready)) {
                Ok(response) => response,
                Err(error) => {
                    self.stop_worker();
                    return Err(Diagnostic::spec_error(
                        "TS1000",
                        format!("TypeScript frontend startup handshake is invalid: {error}"),
                    ));
                }
            };
        if read == 0 || response.get("ready").and_then(JsonValue::as_bool) != Some(true) {
            self.stop_worker();
            return Err(Diagnostic::spec_error(
                "TS1000",
                "TypeScript frontend did not become ready",
            ));
        }
        Ok(())
    }

    fn worker_failure(&mut self, message: &str) -> Diagnostic {
        let status = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten());
        self.stop_worker();
        Diagnostic::spec_error(
            "TS1000",
            status.map_or_else(
                || message.to_owned(),
                |status| format!("{message} (worker exited with {status})"),
            ),
        )
    }

    fn stop_worker(&mut self) {
        self.stdin.take();
        self.stdout.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

impl Drop for TypeScriptFrontend {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

/// Parsed TypeScript after adaptation to Langmeld's generic tree.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceAnalysis {
    /// Parse result containing source-spanned generic nodes.
    pub parsed: ParseResult,
    /// TypeScript parser and generic symbol/reference diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Registers the parser callback that adapts one TypeScript source file.
#[must_use]
pub fn parser_callbacks() -> [CustomConstructParser; 3] {
    parser_callbacks_with_frontend(Arc::new(Mutex::new(TypeScriptFrontend::new())))
}

fn parser_callbacks_with_frontend(
    frontend: Arc<Mutex<TypeScriptFrontend>>,
) -> [CustomConstructParser; 3] {
    let module_frontend = Arc::clone(&frontend);
    [
        CustomConstructParser::new("typescript_module", move |source, start| {
            let mut frontend = module_frontend.lock().map_err(|_| {
                Diagnostic::spec_error("TS1000", "TypeScript frontend lock was poisoned")
            })?;
            parse_module(&mut frontend, source, start)
        }),
        CustomConstructParser::new("typescript_function", parse_host_only_construct),
        CustomConstructParser::new("typescript_default_export_alias", parse_host_only_construct),
    ]
}

/// Loads and validates the checked-in TypeScript source-index definition.
pub fn validated_language_spec() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let definition = SourceFile::new(
        "typescript-source.langmeld",
        include_str!("../typescript-source.langmeld"),
    );
    parse_definition(&definition)?.validate().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| Diagnostic::spec_error(error.code, error.message))
            .collect()
    })
}

/// Parses actual TypeScript syntax, then resolves same-file function references.
#[must_use]
pub fn analyze_source(source: &SourceFile) -> SourceAnalysis {
    let (spec, parser) = match configured_language_and_parser() {
        Ok(value) => value,
        Err(diagnostics) => return empty_analysis(source, diagnostics),
    };
    let mut parsed = parser.parse(source);
    let mut diagnostics = parsed.diagnostics.clone();
    diagnostics.extend(analyze(&spec, &parsed.program, source));
    parsed.diagnostics = diagnostics.clone();
    SourceAnalysis {
        parsed,
        diagnostics,
    }
}

/// Parses a collection of TypeScript files and resolves named function imports
/// across the shared workspace namespace.
#[must_use]
pub fn analyze_sources(sources: &[SourceFile]) -> WorkspaceAnalysis {
    if sources.is_empty() {
        return WorkspaceAnalysis::default();
    }
    let (spec, parser) = match configured_language_and_parser() {
        Ok(value) => value,
        Err(diagnostics) => {
            return WorkspaceAnalysis {
                diagnostics: diagnostics
                    .into_iter()
                    .map(|diagnostic| WorkspaceDiagnostic {
                        source_index: 0,
                        source_id: None,
                        source_revision: None,
                        diagnostic,
                    })
                    .collect(),
                ..WorkspaceAnalysis::default()
            };
        }
    };
    let parsed = sources
        .iter()
        .map(|source| parser.parse(source))
        .collect::<Vec<_>>();
    let workspace_files = sources
        .iter()
        .zip(&parsed)
        .map(|(source, result)| (source, &result.program))
        .collect::<Vec<_>>();
    let mut analysis = analyze_workspace_with_resolutions(&spec, &workspace_files);
    for (source_index, result) in parsed.iter().enumerate() {
        analysis
            .diagnostics
            .extend(
                result
                    .diagnostics
                    .iter()
                    .cloned()
                    .map(|diagnostic| WorkspaceDiagnostic {
                        source_index,
                        source_id: None,
                        source_revision: None,
                        diagnostic,
                    }),
            );
    }
    let metadata = parsed
        .iter()
        .map(|result| module_metadata(&result.program))
        .collect::<Vec<_>>();
    append_project_import_diagnostics(sources, &metadata, &mut analysis.diagnostics);
    analysis
}

/// Creates a workspace hook that checks imported function paths and exports in
/// a live TypeScript project LSP session.
pub fn project_import_analysis_hook() -> WorkspaceTypeAnalysisHook {
    Box::new(|_, files, _, _, _, diagnostics| {
        let sources = files
            .iter()
            .map(|(source, _)| (*source).clone())
            .collect::<Vec<_>>();
        let metadata = files
            .iter()
            .map(|(_, program)| module_metadata(program))
            .collect::<Vec<_>>();
        append_project_import_diagnostics(&sources, &metadata, diagnostics);
    })
}

fn module_metadata(
    program: &langmeld::language::Program,
) -> Result<TypeScriptModuleMetadata, String> {
    program
        .nodes
        .first()
        .and_then(|root| root.host_data::<TypeScriptModuleMetadata>())
        .cloned()
        .ok_or_else(|| "TypeScript module metadata is unavailable".to_owned())
}

fn append_project_import_diagnostics(
    sources: &[SourceFile],
    project_metadata: &[Result<TypeScriptModuleMetadata, String>],
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    for (source_index, source_metadata) in project_metadata.iter().enumerate() {
        let current_metadata = match source_metadata {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(workspace_diagnostic(
                    source_index,
                    Diagnostic::spec_error("TS1000", error.clone()),
                ));
                continue;
            }
        };
        for import in &current_metadata.imports {
            let module = &import.module_specifier;
            let module_span = import.module_span;
            let Some(target_index) =
                resolve_project_module(&sources[source_index], module, sources)
            else {
                diagnostics.push(workspace_diagnostic(
                    source_index,
                    Diagnostic::error(
                        "TS1004",
                        format!("cannot resolve TypeScript import `{module}` in this project"),
                        module_span.unwrap_or(Span::new(0, 0)),
                    ),
                ));
                continue;
            };
            let imported = &import.imported_name;
            let is_default_import = imported == "default";
            let is_exported_function =
                project_metadata[target_index]
                    .as_ref()
                    .ok()
                    .is_some_and(|functions| {
                        functions.functions.iter().any(|function| {
                            if is_default_import {
                                function.default_exported
                            } else {
                                function.name == *imported && function.exported
                            }
                        })
                    });
            if !is_exported_function {
                let name_span = import
                    .imported_span
                    .or(module_span)
                    .unwrap_or(Span::new(0, 0));
                diagnostics.push(workspace_diagnostic(
                    source_index,
                    Diagnostic::error(
                        "TS1005",
                        format!("`{imported}` is not an exported function in `{module}`"),
                        name_span,
                    ),
                ));
            }
        }
    }
}

fn workspace_diagnostic(source_index: usize, diagnostic: Diagnostic) -> WorkspaceDiagnostic {
    WorkspaceDiagnostic {
        source_index,
        source_id: None,
        source_revision: None,
        diagnostic,
    }
}

fn resolve_project_module(
    source: &SourceFile,
    module: &str,
    sources: &[SourceFile],
) -> Option<usize> {
    let importer_path = source_path(&source.name)?;
    let base = importer_path.parent()?;
    let requested_path = normalize_path(&base.join(module));
    let candidate = if requested_path
        .extension()
        .is_some_and(|extension| extension == "ts")
    {
        requested_path
    } else {
        requested_path.with_extension("ts")
    };
    let candidate = normalize_path(&candidate);
    let index_candidate = normalize_path(&base.join(module).join("index.ts"));
    sources.iter().position(|candidate_source| {
        source_path(&candidate_source.name).is_some_and(|path| {
            let path = normalize_path(&path);
            path == candidate || path == index_candidate
        })
    })
}

fn module_namespace_name(source_name: &str) -> String {
    let path = source_path(source_name)
        .map(|path| normalize_path(&path))
        .unwrap_or_else(|| normalize_path(Path::new(source_name)));
    encode_module_path(&logical_module_path(path))
}

fn logical_module_path(mut path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|extension| extension == "ts") {
        path.set_extension("");
        if path.file_name().is_some_and(|name| name == "index") {
            path.pop();
        }
    }
    path
}

fn imported_module_namespace(source: &SourceFile, module: &str) -> Option<String> {
    let importer_path = source_path(&source.name)?;
    let base = importer_path.parent()?;
    Some(encode_module_path(&logical_module_path(normalize_path(
        &base.join(module),
    ))))
}

fn encode_module_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let mut namespace = String::with_capacity(1 + value.len() * 2);
    namespace.push('m');
    for byte in value.bytes() {
        use std::fmt::Write as _;
        write!(namespace, "{byte:02x}").expect("writing to String cannot fail");
    }
    namespace
}

fn source_path(name: &str) -> Option<PathBuf> {
    let path = name.strip_prefix("file://").unwrap_or(name);
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(decoded).ok()?))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn configured_language_and_parser()
-> Result<(ValidatedLanguageSpec, ConfiguredLanguageParser), Vec<Diagnostic>> {
    let spec = validated_language_spec()?;
    let frontend = Arc::new(Mutex::new(TypeScriptFrontend::new()));
    let callbacks = parser_callbacks_with_frontend(frontend);
    let parser = PreparedLanguage::new(spec.clone())
        .map_err(|error| vec![Diagnostic::spec_error("TS1099", error.to_string())])?
        .with_custom_parsers(callbacks)
        .map_err(|error| vec![Diagnostic::spec_error("TS1099", error.to_string())])?;
    Ok((spec, parser.with_custom_lexer(typescript_lexer())))
}

fn typescript_lexer() -> CustomLexer {
    CustomLexer::new(|source| {
        if source.text.is_empty() {
            return LexedSource::default();
        }
        LexedSource::new(
            [LanguageToken::new(
                LanguageTokenKind::Opaque,
                Span::new(0, source.text.len()),
            )],
            [],
        )
    })
}

/// Prepares the parser callbacks used by the TypeScript source adapter.
pub fn configured_parser() -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    configured_language_and_parser().map(|(_, parser)| parser)
}

fn empty_analysis(source: &SourceFile, diagnostics: Vec<Diagnostic>) -> SourceAnalysis {
    SourceAnalysis {
        parsed: ParseResult {
            syntax_tree: SyntaxNode {
                role: SyntaxNodeRole::SourceFile,
                construct: None,
                syntax_variant: None,
                field: None,
                index: None,
                span: Span::new(0, source.text.len()),
                children: Vec::new(),
            },
            program: Program { nodes: Vec::new() },
            diagnostics: diagnostics.clone(),
            syntax_tokens: Vec::new(),
            completion_blocks: Vec::new(),
            completion_frontiers: Vec::new(),
            custom_parser_spans: Vec::new(),
            block_delimiters: Vec::new(),
            resource_limited: false,
        },
        diagnostics,
    }
}

fn parse_module(
    frontend: &mut TypeScriptFrontend,
    source: &SourceFile,
    start: usize,
) -> Result<Option<CustomConstructMatch>, Diagnostic> {
    let ast = frontend.parse(source)?;
    let diagnostics = ast
        .get("errors")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .map(|error| parser_diagnostic(source, error))
        .collect();
    let functions = ast
        .get("functions")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| Diagnostic::spec_error("TS1002", "TypeScript AST has no function list"))?;
    let metadata = project_module_metadata(source, &ast, functions)?;
    let mut children = Vec::new();
    for function in functions {
        if let Some(node) = function_node(source, function) {
            if function
                .get("default_exported")
                .and_then(JsonValue::as_bool)
                == Some(true)
            {
                children.push(default_export_alias(&node));
            } else {
                children.push(node);
            }
        }
    }
    let module_name = module_namespace_name(&source.name);
    Ok(Some(
        CustomConstructMatch {
            span: Span::new(start, source.text.len()),
            fields: BTreeMap::from([(
                "name".to_owned(),
                captured(Value::Identifier(module_name), Span::new(start, start)),
            )]),
            children: Some(children),
            syntax_variant: "typescript_module".to_owned(),
            diagnostics,
            host_data: None,
        }
        .with_host_data(metadata),
    ))
}

fn project_module_metadata(
    source: &SourceFile,
    ast: &JsonValue,
    functions: &[JsonValue],
) -> Result<TypeScriptModuleMetadata, Diagnostic> {
    let imports = ast
        .get("imports")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| Diagnostic::spec_error("TS1002", "TypeScript AST has no import list"))?
        .iter()
        .map(|import| {
            let module_specifier = import
                .get("module_specifier")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    Diagnostic::spec_error("TS1002", "TypeScript import has no module specifier")
                })?;
            let imported_name = import
                .get("imported_name")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    Diagnostic::spec_error("TS1002", "TypeScript import has no imported name")
                })?;
            Ok(TypeScriptImport {
                module_specifier: module_specifier.to_owned(),
                module_span: import
                    .get("module_span")
                    .and_then(|span| read_span(source, span)),
                imported_name: imported_name.to_owned(),
                imported_span: import
                    .get("imported_span")
                    .and_then(|span| read_span(source, span)),
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    let functions = functions
        .iter()
        .map(|function| {
            let name = function
                .get("name")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    Diagnostic::spec_error("TS1002", "TypeScript function has no name")
                })?;
            Ok(TypeScriptExport {
                name: name.to_owned(),
                exported: function.get("exported").and_then(JsonValue::as_bool) == Some(true),
                default_exported: function
                    .get("default_exported")
                    .and_then(JsonValue::as_bool)
                    == Some(true),
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    Ok(TypeScriptModuleMetadata { imports, functions })
}

fn parse_host_only_construct(
    _source: &SourceFile,
    _start: usize,
) -> Result<Option<CustomConstructMatch>, Diagnostic> {
    Ok(None)
}

fn function_node(source: &SourceFile, function: &JsonValue) -> Option<AstNode> {
    let name = function.get("name")?.as_str()?.to_owned();
    let span = read_span(source, function.get("span")?)?;
    let name_span = read_span(source, function.get("name_span")?)?;
    let kind = function.get("kind")?.as_str()?;
    let mut fields = BTreeMap::from([
        (
            "name".to_owned(),
            captured(Value::Identifier(name), name_span),
        ),
        (
            "kind".to_owned(),
            captured(Value::Enum(kind.to_owned()), span),
        ),
    ]);

    if let Some(return_type) = function.get("return_type").and_then(JsonValue::as_str) {
        let return_span = read_span(source, function.get("return_type_span")?)?;
        fields.insert(
            "return_type".to_owned(),
            captured(Value::Text(return_type.to_owned()), return_span),
        );
    }

    let parameters = function
        .get("parameters")?
        .as_array()?
        .iter()
        .map(|parameter| {
            let name = parameter.get("name")?.as_str()?.to_owned();
            let name_span = read_span(source, parameter.get("name_span")?)?;
            let item_span = read_span(source, parameter.get("span")?)?;
            let mut values =
                BTreeMap::from([("name".to_owned(), captured(Value::Text(name), name_span))]);
            if let Some(annotation) = parameter.get("annotation").and_then(JsonValue::as_str) {
                let annotation_span = read_span(source, parameter.get("annotation_span")?)?;
                values.insert(
                    "annotation".to_owned(),
                    captured(Value::Text(annotation.to_owned()), annotation_span),
                );
            }
            Some(CapturedValue {
                value: Value::Properties(values),
                span: item_span,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if !parameters.is_empty() {
        let parameters_span =
            Span::new(parameters.first()?.span.start, parameters.last()?.span.end);
        fields.insert(
            "parameters".to_owned(),
            captured(Value::List(parameters), parameters_span),
        );
    }

    let calls = function
        .get("calls")?
        .as_array()?
        .iter()
        .filter_map(|call| {
            let target = call.get("target")?.as_str()?;
            let target_span = read_span(source, call.get("target_span")?)?;
            let call_span = read_span(source, call.get("span")?)?;
            let mut fields = BTreeMap::from([(
                "target".to_owned(),
                captured(Value::Reference(target.to_owned()), target_span),
            )]);
            if let Some(module) = call.get("module_specifier").and_then(JsonValue::as_str) {
                fields.insert(
                    "module_specifier".to_owned(),
                    captured(Value::Text(module.to_owned()), target_span),
                );
            }
            if let Some(local_name) = call.get("local_name").and_then(JsonValue::as_str) {
                fields.insert(
                    "local_name".to_owned(),
                    captured(Value::Text(local_name.to_owned()), target_span),
                );
            }
            let call = AstNode {
                construct: "call".to_owned(),
                syntax_variant: "typescript_call".to_owned(),
                fields,
                children: Vec::new(),
                span: call_span,
                host_data: None,
            };
            match call.fields.get("module_specifier").and_then(|field| {
                if let Value::Text(module) = &field.value {
                    Some(module.as_str())
                } else {
                    None
                }
            }) {
                Some(module) => Some(AstNode {
                    construct: "module".to_owned(),
                    syntax_variant: "typescript_import_scope".to_owned(),
                    fields: BTreeMap::from([(
                        "name".to_owned(),
                        captured(
                            Value::Identifier(imported_module_namespace(source, module)?),
                            target_span,
                        ),
                    )]),
                    children: vec![call.clone()],
                    span: call.span,
                    host_data: None,
                }),
                None => Some(call),
            }
        })
        .collect();
    Some(AstNode {
        construct: "function".to_owned(),
        syntax_variant: if kind == "async" {
            "typescript_async_function"
        } else {
            "typescript_function"
        }
        .to_owned(),
        fields,
        children: calls,
        span,
        host_data: None,
    })
}

fn default_export_alias(function: &AstNode) -> AstNode {
    let name_span = function
        .fields
        .get("name")
        .map_or(function.span, |name| name.span);
    AstNode {
        construct: "default_export_alias".to_owned(),
        syntax_variant: "typescript_default_export_alias".to_owned(),
        fields: BTreeMap::from([(
            "name".to_owned(),
            captured(Value::Identifier("default".to_owned()), name_span),
        )]),
        children: vec![function.clone()],
        span: function.span,
        host_data: None,
    }
}

fn parser_diagnostic(source: &SourceFile, error: &JsonValue) -> Diagnostic {
    let message = error
        .get("message")
        .and_then(JsonValue::as_str)
        .unwrap_or("invalid TypeScript syntax");
    match error.get("span").and_then(|span| read_span(source, span)) {
        Some(span) => Diagnostic::error("TS1001", message, span),
        None => Diagnostic::spec_error("TS1001", message),
    }
}

fn read_span(source: &SourceFile, span: &JsonValue) -> Option<Span> {
    let coordinates = span.as_array()?;
    let start = usize::try_from(coordinates.first()?.as_u64()?).ok()?;
    let end = usize::try_from(coordinates.get(1)?.as_u64()?).ok()?;
    (start <= end
        && end <= source.text.len()
        && source.text.is_char_boundary(start)
        && source.text.is_char_boundary(end))
    .then_some(Span::new(start, end))
}

fn captured(value: Value, span: Span) -> CapturedValue {
    CapturedValue { value, span }
}

/// Generated typed views for this TypeScript indexing definition.
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/langmeld_ast.rs"));
}

/// Reports whether any parser or generic-analysis diagnostic is an error.
#[must_use]
pub fn has_errors(analysis: &SourceAnalysis) -> bool {
    analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}
