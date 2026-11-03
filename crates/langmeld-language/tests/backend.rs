use std::{cell::Cell, collections::BTreeMap};

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{
    ArtifactBackend, BackendDescriptor, BackendDescriptorIssue, BackendError, BackendFormat,
    BackendInputs, BackendOptions, BackendSetting, CompilationArtifact, Program, SourceMapEntry,
    WorkspaceArtifactBackend, WorkspaceBackendInputs, compile_with_backend,
    compile_workspace_with_backend, resolve_backend_settings, validate_backend_descriptor,
};

struct TestBackend {
    descriptor: BackendDescriptor,
    settings: Vec<BackendSetting>,
    result: Result<CompilationArtifact, Vec<Diagnostic>>,
    called: Cell<bool>,
    received_options: Cell<Option<BackendOptions>>,
    received_source: Cell<bool>,
}

impl ArtifactBackend for TestBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn settings(&self) -> Vec<BackendSetting> {
        self.settings.clone()
    }

    fn compile(
        &self,
        inputs: &BackendInputs<'_>,
        options: &BackendOptions,
    ) -> Result<CompilationArtifact, Vec<Diagnostic>> {
        self.called.set(true);
        self.received_options.set(Some(options.clone()));
        self.received_source.set(inputs.source.is_some());
        self.result.clone()
    }
}

fn format() -> BackendFormat {
    BackendFormat {
        id: "text".to_owned(),
        media_type: "text/plain".to_owned(),
        extension: "txt".to_owned(),
    }
}

fn descriptor() -> BackendDescriptor {
    BackendDescriptor {
        id: "test-text".to_owned(),
        formats: vec![format()],
        required_ir: vec!["program.ast".to_owned()],
        supports_source_maps: true,
    }
}

fn artifact() -> CompilationArtifact {
    CompilationArtifact {
        format: "text".to_owned(),
        media_type: "text/plain".to_owned(),
        bytes: b"compiled".to_vec(),
        source_map: Vec::new(),
    }
}

fn backend() -> TestBackend {
    TestBackend {
        descriptor: descriptor(),
        settings: Vec::new(),
        result: Ok(artifact()),
        called: Cell::new(false),
        received_options: Cell::new(None),
        received_source: Cell::new(false),
    }
}

fn inputs<'a>(program: &'a Program) -> BackendInputs<'a> {
    BackendInputs::new(program).with_artifact("program.ast", b"ast".to_vec())
}

#[test]
fn descriptor_validation_rejects_ambiguous_or_incomplete_metadata() {
    let mut invalid = descriptor();
    invalid.formats.push(format());
    assert!(matches!(
        validate_backend_descriptor(&invalid),
        Err(BackendError::InvalidDescriptor {
            issue: BackendDescriptorIssue::DuplicateFormatId(id),
            ..
        }) if id == "text"
    ));

    let mut invalid = descriptor();
    invalid.formats[0].extension = ".txt".to_owned();
    assert!(matches!(
        validate_backend_descriptor(&invalid),
        Err(BackendError::InvalidDescriptor {
            issue: BackendDescriptorIssue::InvalidFormatMetadata { field, .. },
            ..
        }) if field == "extension"
    ));

    let mut invalid = descriptor();
    invalid.required_ir.push("program.ast".to_owned());
    assert!(matches!(
        validate_backend_descriptor(&invalid),
        Err(BackendError::InvalidDescriptor {
            issue: BackendDescriptorIssue::DuplicateRequiredIr(id),
            ..
        }) if id == "program.ast"
    ));
}

#[test]
fn invalid_backend_descriptor_fails_before_compiler_invocation() {
    let mut backend = backend();
    backend.descriptor.formats.clear();
    let program = Program { nodes: Vec::new() };

    let result = compile_with_backend(&backend, &inputs(&program), &BackendOptions::new("text"));

    assert!(matches!(
        result,
        Err(BackendError::InvalidDescriptor {
            issue: BackendDescriptorIssue::NoFormats,
            ..
        })
    ));
    assert!(!backend.called.get());
}

#[test]
fn setting_resolution_applies_defaults_and_preserves_explicit_values() {
    let schema = [
        BackendSetting::choice("opt-level", "Optimization", ["0", "2", "3"]).default_value("2"),
        BackendSetting::text("runtime", "Runtime"),
    ];
    let values = BTreeMap::from([("opt-level".to_owned(), "3".to_owned())]);

    let resolved = resolve_backend_settings("test", &schema, &values)
        .expect("valid explicit values and defaults resolve");

    assert_eq!(resolved.get("opt-level").map(String::as_str), Some("3"));
    assert!(!resolved.contains_key("runtime"));
}

#[test]
fn setting_resolution_rejects_invalid_defaults_and_unknown_values() {
    let invalid_default =
        [BackendSetting::choice("mode", "Mode", ["fast", "safe"]).default_value("unknown")];
    assert!(matches!(
        resolve_backend_settings("test", &invalid_default, &BTreeMap::new()),
        Err(BackendError::InvalidSettingSchema { key, .. }) if key == "mode"
    ));

    let schema = [BackendSetting::choice("mode", "Mode", ["fast", "safe"])];
    let values = BTreeMap::from([("mode".to_owned(), "unknown".to_owned())]);
    assert!(matches!(
        resolve_backend_settings("test", &schema, &values),
        Err(BackendError::InvalidSettingValue { key, .. }) if key == "mode"
    ));
}

#[test]
fn compiler_receives_effective_settings_without_mutating_caller_options() {
    let mut backend = backend();
    backend.settings = vec![
        BackendSetting::choice("opt-level", "Optimization", ["0", "2", "3"]).default_value("2"),
    ];
    let program = Program { nodes: Vec::new() };
    let options = BackendOptions::new("text").with_setting("opt-level", "3");

    compile_with_backend(&backend, &inputs(&program), &options)
        .expect("valid backend output is returned");

    assert_eq!(
        options.settings.get("opt-level").map(String::as_str),
        Some("3")
    );
    assert_eq!(
        backend
            .received_options
            .take()
            .expect("compiler receives options")
            .settings
            .get("opt-level")
            .map(String::as_str),
        Some("3")
    );

    let defaulted = BackendOptions::new("text");
    compile_with_backend(&backend, &inputs(&program), &defaulted)
        .expect("declared setting defaults are valid");
    assert_eq!(
        backend
            .received_options
            .take()
            .expect("compiler receives resolved defaults")
            .settings
            .get("opt-level")
            .map(String::as_str),
        Some("2")
    );
}

#[test]
fn unsupported_format_and_missing_ir_fail_before_compilation() {
    let backend = backend();
    let program = Program { nodes: Vec::new() };
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program), &BackendOptions::new("binary")),
        Err(BackendError::UnsupportedFormat { .. })
    ));
    assert!(!backend.called.get());

    assert!(matches!(
        compile_with_backend(
            &backend,
            &BackendInputs::new(&program),
            &BackendOptions::new("text")
        ),
        Err(BackendError::MissingRequiredIr { artifact, .. }) if artifact == "program.ast"
    ));
    assert!(!backend.called.get());
}

#[test]
fn requested_source_maps_require_backend_support_and_original_source() {
    let mut backend = backend();
    let program = Program { nodes: Vec::new() };
    let options = BackendOptions {
        source_maps: true,
        ..BackendOptions::new("text")
    };
    backend.descriptor.supports_source_maps = false;
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program), &options),
        Err(BackendError::UnsupportedSourceMaps { .. })
    ));
    assert!(!backend.called.get());

    backend.descriptor.supports_source_maps = true;
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program), &options),
        Err(BackendError::MissingSourceSnapshotForMaps { .. })
    ));
    assert!(!backend.called.get());
}

#[test]
fn valid_artifact_receives_original_source_and_is_returned() {
    let backend = backend();
    let program = Program { nodes: Vec::new() };
    let source = SourceFile::new("main.lang", "original source");
    let result = compile_with_backend(
        &backend,
        &inputs(&program).with_source(&source),
        &BackendOptions::new("text"),
    )
    .expect("valid backend artifact is returned");

    assert_eq!(result, artifact());
    assert!(backend.called.get());
    assert!(backend.received_source.get());
}

#[test]
fn mismatched_format_metadata_is_rejected() {
    let mut backend = backend();
    backend.result = Ok(CompilationArtifact {
        format: "binary".to_owned(),
        ..artifact()
    });
    let program = Program { nodes: Vec::new() };

    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program), &BackendOptions::new("text")),
        Err(BackendError::InvalidArtifact { .. })
    ));
}

#[test]
fn generated_and_source_map_ranges_are_validated() {
    let mut backend = backend();
    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::new(Span::new(0, 4), Span::new(0, 1))],
        ..artifact()
    });
    let program = Program { nodes: Vec::new() };
    let source = SourceFile::new("main.lang", "hello");
    let options = BackendOptions {
        source_maps: true,
        ..BackendOptions::new("text")
    };
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program).with_source(&source), &options),
        Err(BackendError::InvalidSourceMapRange { .. })
    ));

    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::new(Span::new(0, 1), Span::new(0, 6))],
        ..artifact()
    });
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program).with_source(&source), &options),
        Err(BackendError::InvalidSourceMapSourceRange { .. })
    ));

    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::in_source(
            Span::new(0, 1),
            Span::new(0, 1),
            1,
        )],
        ..artifact()
    });
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program).with_source(&source), &options),
        Err(BackendError::InvalidSourceMapSourceIndex { .. })
    ));

    let unicode = SourceFile::new("unicode.lang", "é");
    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::new(Span::new(0, 1), Span::new(0, 1))],
        ..artifact()
    });
    assert!(matches!(
        compile_with_backend(&backend, &inputs(&program).with_source(&unicode), &options),
        Err(BackendError::InvalidSourceMapSourceRange { .. })
    ));
}

#[test]
fn backend_must_not_return_unrequested_source_maps() {
    let mut backend = backend();
    backend.result = Ok(CompilationArtifact {
        source_map: vec![SourceMapEntry::new(Span::new(0, 1), Span::new(0, 1))],
        ..artifact()
    });
    let program = Program { nodes: Vec::new() };
    let source = SourceFile::new("main.lang", "source");

    assert!(matches!(
        compile_with_backend(
            &backend,
            &inputs(&program).with_source(&source),
            &BackendOptions::new("text")
        ),
        Err(BackendError::UnexpectedSourceMaps { .. })
    ));
}

#[test]
fn backend_diagnostics_are_preserved() {
    let mut backend = backend();
    let diagnostic = Diagnostic::error("LM3001", "lowering failed", Span::new(2, 5));
    backend.result = Err(vec![diagnostic.clone()]);
    let program = Program { nodes: Vec::new() };

    assert_eq!(
        compile_with_backend(&backend, &inputs(&program), &BackendOptions::new("text")),
        Err(BackendError::Diagnostics(vec![diagnostic]))
    );
}

struct WorkspaceTestBackend {
    descriptor: BackendDescriptor,
    result: Result<CompilationArtifact, Vec<Diagnostic>>,
    called: Cell<bool>,
    received_file_count: Cell<usize>,
    received_host_ir: Cell<bool>,
}

impl WorkspaceArtifactBackend for WorkspaceTestBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn compile_workspace(
        &self,
        inputs: &WorkspaceBackendInputs<'_>,
        _options: &BackendOptions,
    ) -> Result<CompilationArtifact, Vec<Diagnostic>> {
        self.called.set(true);
        self.received_file_count.set(inputs.files.len());
        self.received_host_ir
            .set(inputs.artifacts.contains_key("host.ir"));
        self.result.clone()
    }
}

fn workspace_backend() -> WorkspaceTestBackend {
    WorkspaceTestBackend {
        descriptor: BackendDescriptor {
            required_ir: vec!["langmeld.workspace".to_owned(), "host.ir".to_owned()],
            ..descriptor()
        },
        result: Ok(artifact()),
        called: Cell::new(false),
        received_file_count: Cell::new(0),
        received_host_ir: Cell::new(false),
    }
}

#[test]
fn workspace_backend_checks_ir_and_receives_all_sources() {
    let backend = workspace_backend();
    let first_source = SourceFile::new("first.lang", "first");
    let second_source = SourceFile::new("second.lang", "second");
    let first_program = Program { nodes: Vec::new() };
    let second_program = Program { nodes: Vec::new() };
    let files = [
        (&first_source, &first_program),
        (&second_source, &second_program),
    ];
    let options = BackendOptions::new("text");

    assert!(matches!(
        compile_workspace_with_backend(
            &backend,
            &WorkspaceBackendInputs::new(&files),
            &options
        ),
        Err(BackendError::MissingRequiredIr { artifact, .. }) if artifact == "host.ir"
    ));
    assert!(!backend.called.get());

    let result = compile_workspace_with_backend(
        &backend,
        &WorkspaceBackendInputs::new(&files).with_artifact("host.ir", b"ir".to_vec()),
        &options,
    )
    .expect("workspace backend receives built-in and host artifacts");

    assert_eq!(result, artifact());
    assert!(backend.called.get());
    assert_eq!(backend.received_file_count.get(), 2);
    assert!(backend.received_host_ir.get());
}

#[test]
fn workspace_program_ir_requires_exactly_one_source() {
    let mut backend = workspace_backend();
    backend.descriptor.required_ir = vec!["langmeld.program".to_owned()];
    let first_source = SourceFile::new("first.lang", "first");
    let second_source = SourceFile::new("second.lang", "second");
    let program = Program { nodes: Vec::new() };
    let two_files = [(&first_source, &program), (&second_source, &program)];

    assert!(matches!(
        compile_workspace_with_backend(
            &backend,
            &WorkspaceBackendInputs::new(&two_files),
            &BackendOptions::new("text")
        ),
        Err(BackendError::MissingRequiredIr { artifact, .. })
            if artifact == "langmeld.program (exactly one source)"
    ));
    assert!(!backend.called.get());

    let one_file = [(&first_source, &program)];
    compile_workspace_with_backend(
        &backend,
        &WorkspaceBackendInputs::new(&one_file),
        &BackendOptions::new("text"),
    )
    .expect("the generic program is available for one source");
    assert!(backend.called.get());
}

#[test]
fn workspace_backend_diagnostics_and_artifact_metadata_are_preserved() {
    let mut backend = workspace_backend();
    backend.descriptor.required_ir.clear();
    let source = SourceFile::new("main.lang", "source");
    let program = Program { nodes: Vec::new() };
    let files = [(&source, &program)];
    let inputs = WorkspaceBackendInputs::new(&files);
    let options = BackendOptions::new("text");
    let diagnostic = Diagnostic::error("LM3002", "workspace lowering failed", Span::new(0, 4));

    backend.result = Err(vec![diagnostic.clone()]);
    assert_eq!(
        compile_workspace_with_backend(&backend, &inputs, &options),
        Err(BackendError::Diagnostics(vec![diagnostic]))
    );

    backend.result = Ok(CompilationArtifact {
        media_type: "application/octet-stream".to_owned(),
        ..artifact()
    });
    assert!(matches!(
        compile_workspace_with_backend(&backend, &inputs, &options),
        Err(BackendError::InvalidArtifact { .. })
    ));
}

#[test]
fn workspace_source_maps_require_unambiguous_valid_source_ranges() {
    let mut backend = workspace_backend();
    backend.descriptor.required_ir.clear();
    backend.descriptor.supports_source_maps = true;
    let first_source = SourceFile::new("first.lang", "alpha");
    let second_source = SourceFile::new("second.lang", "βeta");
    let program = Program { nodes: Vec::new() };
    let files = [(&first_source, &program), (&second_source, &program)];
    let inputs = WorkspaceBackendInputs::new(&files);
    let options = BackendOptions {
        source_maps: true,
        ..BackendOptions::new("text")
    };

    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::in_source(
            Span::new(0, 1),
            Span::new(0, 2),
            1,
        )],
        ..artifact()
    });
    compile_workspace_with_backend(&backend, &inputs, &options)
        .expect("valid multi-source map identifies its input");

    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::new(Span::new(0, 1), Span::new(0, 1))],
        ..artifact()
    });
    assert!(matches!(
        compile_workspace_with_backend(&backend, &inputs, &options),
        Err(BackendError::MissingSourceMapSourceIndex { .. })
    ));

    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::in_source(
            Span::new(0, 1),
            Span::new(0, 1),
            8,
        )],
        ..artifact()
    });
    assert!(matches!(
        compile_workspace_with_backend(&backend, &inputs, &options),
        Err(BackendError::InvalidSourceMapSourceIndex { .. })
    ));

    backend.result = Ok(CompilationArtifact {
        bytes: b"out".to_vec(),
        source_map: vec![SourceMapEntry::in_source(
            Span::new(0, 1),
            Span::new(0, 1),
            1,
        )],
        ..artifact()
    });
    assert!(matches!(
        compile_workspace_with_backend(&backend, &inputs, &options),
        Err(BackendError::InvalidSourceMapSourceRange { .. })
    ));
}
