use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use langmeld_core::SourceFile;
use langmeld_language::{LanguageWorkspaceAnalyzer, complete_with, parse};
use schema_language::{SchemaAnalyzer, SchemaCompleter, SchemaWorkspaceAnalyzer, parse_schema};

const SIZES: [usize; 3] = [16, 128, 1_024];

fn language_workloads(criterion: &mut Criterion) {
    benchmark_declarative_parser(criterion);
    benchmark_host_language(criterion);
    benchmark_workspace_editor(criterion);
}

fn benchmark_declarative_parser(criterion: &mut Criterion) {
    let language = toy_language::load().expect("WidgetScript definition loads");
    let mut group = criterion.benchmark_group("declarative/widgetscript");

    for count in SIZES {
        let source = SourceFile::new("benchmark.widget", widgetscript_source(count));
        let parsed = parse(&source, &language);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        assert_eq!(parsed.constructs[0].children.len(), count);

        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::new("parse", count),
            &source,
            |bencher, source| {
                bencher.iter(|| black_box(parse(black_box(source), &language)));
            },
        );
    }

    group.finish();
}

fn benchmark_host_language(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("host/aster");

    for count in SIZES {
        let source = SourceFile::new("benchmark.aster", aster_source(count));
        let parsed = aster_language::parse_program(&source);
        let analysis = aster_language::analyze(&source, &parsed);

        assert!(parsed.is_valid(), "{:#?}", parsed.diagnostics);
        assert!(analysis.is_valid(), "{:#?}", analysis.diagnostics);
        assert_eq!(parsed.tree.functions.len(), count);
        assert_eq!(analysis.result.symbols.len(), count * 3);
        assert_eq!(analysis.result.references.len(), count * 2);

        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::new("parse", count),
            &source,
            |bencher, source| {
                bencher.iter(|| black_box(aster_language::parse_program(black_box(source))));
            },
        );
        group.bench_with_input(BenchmarkId::new("analysis", count), &count, |bencher, _| {
            bencher.iter(|| black_box(aster_language::analyze(&source, &parsed)));
        });
        group.bench_with_input(
            BenchmarkId::new("parse_and_analysis", count),
            &count,
            |bencher, _| {
                bencher.iter(|| {
                    let parsed = aster_language::parse_program(black_box(&source));

                    black_box(aster_language::analyze(&source, &parsed))
                });
            },
        );
    }

    group.finish();
}

fn benchmark_workspace_editor(criterion: &mut Criterion) {
    let analyzer = SchemaAnalyzer;
    let workspace_analyzer = SchemaWorkspaceAnalyzer;
    let completer = SchemaCompleter;
    let mut group = criterion.benchmark_group("workspace/schema");

    for count in SIZES {
        let sources = schema_sources(count);
        let parsed = sources.iter().map(parse_schema).collect::<Vec<_>>();
        let documents = sources.iter().zip(&parsed).collect::<Vec<_>>();
        let analysis = workspace_analyzer.analyze_workspace(&analyzer, &documents);

        assert!(
            parsed.iter().all(|document| document.is_valid()),
            "schema fixture failed to parse"
        );
        assert!(
            analysis.iter().all(|document| document.is_valid()),
            "schema fixture failed to analyze"
        );
        assert_eq!(analysis[0].result.symbols.len(), count);
        assert_eq!(analysis[0].result.references.len(), count);

        let type_reference_start = sources[0]
            .text()
            .find("next: Record")
            .expect("first schema type reference")
            + "next: ".len();
        let completion_offset = type_reference_start + 3;
        let completions = complete_with(
            &sources[0],
            &parsed[0],
            Some(&analysis[0]),
            completion_offset,
            &completer,
        );
        assert!(
            completions
                .iter()
                .any(|candidate| candidate.text == "Record1"),
            "schema completion omitted the referenced record"
        );

        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::new("parse", count),
            &sources,
            |bencher, sources| {
                bencher.iter(|| black_box(sources.iter().map(parse_schema).collect::<Vec<_>>()));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("workspace_analysis", count),
            &documents,
            |bencher, documents| {
                bencher
                    .iter(|| black_box(workspace_analyzer.analyze_workspace(&analyzer, documents)));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("completion", count),
            &completion_offset,
            |bencher, offset| {
                bencher.iter(|| {
                    black_box(complete_with(
                        &sources[0],
                        &parsed[0],
                        Some(&analysis[0]),
                        *offset,
                        &completer,
                    ))
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("parse_and_workspace_analysis", count),
            &sources,
            |bencher, sources| {
                bencher.iter(|| {
                    let parsed = sources.iter().map(parse_schema).collect::<Vec<_>>();
                    let documents = sources.iter().zip(&parsed).collect::<Vec<_>>();

                    black_box(workspace_analyzer.analyze_workspace(&analyzer, &documents))
                });
            },
        );
    }

    group.finish();
}

fn widgetscript_source(count: usize) -> String {
    let mut source = String::from("program {\n");
    for _ in 0..count {
        source.push_str("  draw circle;\n");
    }
    source.push_str("}\n");

    source
}

fn aster_source(count: usize) -> String {
    let mut source = String::new();
    for index in 0..count {
        source.push_str(&format!(
            "fn add_{index}(left: i32, right: i32) -> i32 {{ return left + right; }}\n"
        ));
    }

    source
}

fn schema_sources(count: usize) -> Vec<SourceFile> {
    (0..count)
        .map(|index| {
            let next = (index + 1) % count;
            let source = format!("record Record{index} {{ next: Record{next} }}");

            SourceFile::new(format!("record-{index}.schema"), source)
        })
        .collect()
}

criterion_group!(benches, language_workloads);
criterion_main!(benches);
