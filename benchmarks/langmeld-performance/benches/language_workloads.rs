use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use langmeld::core::SourceFile;
use std::hint::black_box;

const SIZES: [usize; 3] = [16, 128, 1024];

fn workloads(c: &mut Criterion) {
    benchmark_ledger(c);
    benchmark_c_signatures(c);
    benchmark_functions(c);
}

fn benchmark_ledger(c: &mut Criterion) {
    let language = ledger_language::load_language().expect("Ledger language loads");
    let parser = ledger_language::parser().expect("Ledger parser prepares");
    let mut group = c.benchmark_group("ledger_interpreter");
    group.bench_function("load_compiled_language", |b| {
        b.iter(|| black_box(ledger_language::load_language().expect("Ledger language loads")))
    });
    group.bench_function("parser_setup", |b| {
        b.iter(|| black_box(ledger_language::parser().expect("Ledger parser prepares")))
    });

    for count in SIZES {
        let source = SourceFile::new("bench.ledger.lmprog", ledger_source(count));
        let sources = [source.clone()];
        let parsed = parser.parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let programs = [parsed.program];
        let program_refs = [&programs[0]];
        let analysis = ledger_language::analyze_sources(&language, &sources, &program_refs)
            .expect("Ledger analysis runs");
        assert!(
            analysis.diagnostics.is_empty(),
            "{:#?}",
            analysis.diagnostics
        );
        let expected = ledger_language::evaluate(&ledger_language::RuntimeContext {
            language: &language,
            sources: &sources,
            programs: &programs,
            analysis: &analysis,
        })
        .expect("Ledger program executes");
        assert_eq!(expected.len(), count);

        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("parse", count), &source, |b, source| {
            b.iter(|| black_box(parser.parse(black_box(source))))
        });
        group.bench_with_input(BenchmarkId::new("analysis", count), &count, |b, _| {
            b.iter(|| {
                black_box(
                    ledger_language::analyze_sources(&language, &sources, &program_refs)
                        .expect("Ledger analysis runs"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("interpret", count), &count, |b, _| {
            b.iter(|| {
                black_box(
                    ledger_language::evaluate(&ledger_language::RuntimeContext {
                        language: &language,
                        sources: &sources,
                        programs: &programs,
                        analysis: &analysis,
                    })
                    .expect("Ledger program executes"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("full_pipeline", count), &count, |b, _| {
            b.iter(|| {
                let parsed = parser.parse(black_box(&source));
                let programs = [parsed.program];
                let analysis =
                    ledger_language::analyze_sources(&language, &sources, &[&programs[0]])
                        .expect("Ledger analysis runs");
                black_box(
                    ledger_language::evaluate(&ledger_language::RuntimeContext {
                        language: &language,
                        sources: &sources,
                        programs: &programs,
                        analysis: &analysis,
                    })
                    .expect("Ledger program executes"),
                )
            })
        });
    }
    group.finish();
}

fn benchmark_c_signatures(c: &mut Criterion) {
    let language = csignatures_language::load_language().expect("CSignatures language loads");
    let parser = csignatures_language::parser().expect("CSignatures parser prepares");
    let mut group = c.benchmark_group("c_signatures_to_rust_ffi");
    group.bench_function("load_compiled_language", |b| {
        b.iter(|| {
            black_box(csignatures_language::load_language().expect("CSignatures language loads"))
        })
    });
    group.bench_function("parser_setup", |b| {
        b.iter(|| black_box(csignatures_language::parser().expect("CSignatures parser prepares")))
    });

    for count in SIZES {
        let source = SourceFile::new("bench.c", c_source(count));
        let sources = [source.clone()];
        let parsed = parser.parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let programs = [parsed.program];
        let program_refs = [&programs[0]];
        let analysis = csignatures_language::analyze_sources(&language, &sources, &program_refs)
            .expect("CSignatures analysis runs");
        assert!(
            analysis.diagnostics.is_empty(),
            "{:#?}",
            analysis.diagnostics
        );
        let expected = csignatures_language::generate_rust(&csignatures_language::RuntimeContext {
            language: &language,
            sources: &sources,
            programs: &programs,
            analysis: &analysis,
        })
        .expect("C declarations lower to Rust");
        assert_eq!(expected.lines().count(), count + 2);

        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("parse", count), &source, |b, source| {
            b.iter(|| black_box(parser.parse(black_box(source))))
        });
        group.bench_with_input(BenchmarkId::new("analysis", count), &count, |b, _| {
            b.iter(|| {
                black_box(
                    csignatures_language::analyze_sources(&language, &sources, &program_refs)
                        .expect("CSignatures analysis runs"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("generate_rust", count), &count, |b, _| {
            b.iter(|| {
                black_box(
                    csignatures_language::generate_rust(&csignatures_language::RuntimeContext {
                        language: &language,
                        sources: &sources,
                        programs: &programs,
                        analysis: &analysis,
                    })
                    .expect("C declarations lower to Rust"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("full_pipeline", count), &count, |b, _| {
            b.iter(|| {
                let parsed = parser.parse(black_box(&source));
                let programs = [parsed.program];
                let analysis =
                    csignatures_language::analyze_sources(&language, &sources, &[&programs[0]])
                        .expect("CSignatures analysis runs");
                black_box(
                    csignatures_language::generate_rust(&csignatures_language::RuntimeContext {
                        language: &language,
                        sources: &sources,
                        programs: &programs,
                        analysis: &analysis,
                    })
                    .expect("C declarations lower to Rust"),
                )
            })
        });
    }
    group.finish();
}

fn benchmark_functions(c: &mut Criterion) {
    let language = functions_language::load_language().expect("Functions language loads");
    let parser = functions_language::parser().expect("Functions parser prepares");
    let mut group = c.benchmark_group("rust_function_source_generation");
    group.bench_function("load_compiled_language", |b| {
        b.iter(|| black_box(functions_language::load_language().expect("Functions language loads")))
    });
    group.bench_function("parser_setup", |b| {
        b.iter(|| black_box(functions_language::parser().expect("Functions parser prepares")))
    });

    for count in SIZES {
        let source = SourceFile::new("bench.functions.lmprog", functions_source(count));
        let sources = [source.clone()];
        let parsed = parser.parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let programs = [parsed.program];
        let program_refs = [&programs[0]];
        let analysis = functions_language::analyze_sources(&language, &sources, &program_refs)
            .expect("Functions analysis runs");
        assert!(
            analysis.diagnostics.is_empty(),
            "{:#?}",
            analysis.diagnostics
        );
        let expected = functions_language::generate_rust(&functions_language::RuntimeContext {
            language: &language,
            sources: &sources,
            programs: &programs,
            analysis: &analysis,
        })
        .expect("function declarations lower to Rust");
        assert_eq!(expected.matches("pub fn ").count(), count);

        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("parse", count), &source, |b, source| {
            b.iter(|| black_box(parser.parse(black_box(source))))
        });
        group.bench_with_input(BenchmarkId::new("analysis", count), &count, |b, _| {
            b.iter(|| {
                black_box(
                    functions_language::analyze_sources(&language, &sources, &program_refs)
                        .expect("Functions analysis runs"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("generate_rust", count), &count, |b, _| {
            b.iter(|| {
                black_box(
                    functions_language::generate_rust(&functions_language::RuntimeContext {
                        language: &language,
                        sources: &sources,
                        programs: &programs,
                        analysis: &analysis,
                    })
                    .expect("function declarations lower to Rust"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("full_pipeline", count), &count, |b, _| {
            b.iter(|| {
                let parsed = parser.parse(black_box(&source));
                let programs = [parsed.program];
                let analysis =
                    functions_language::analyze_sources(&language, &sources, &[&programs[0]])
                        .expect("Functions analysis runs");
                black_box(
                    functions_language::generate_rust(&functions_language::RuntimeContext {
                        language: &language,
                        sources: &sources,
                        programs: &programs,
                        analysis: &analysis,
                    })
                    .expect("function declarations lower to Rust"),
                )
            })
        });
    }
    group.finish();
}

fn ledger_source(count: usize) -> String {
    let mut source = String::from("program {\n");
    for index in 0..count {
        source.push_str(&format!(
            "  set value_{index} to {index}\n  print value_{index}\n"
        ));
    }
    source.push_str("}\n");
    source
}

fn c_source(count: usize) -> String {
    let mut source = String::new();
    for index in 0..count {
        source.push_str(&format!(
            "int function_{index}(int left_{index}, int right_{index});\n"
        ));
    }
    source
}

fn functions_source(count: usize) -> String {
    let mut source = String::from("module {\n");
    for index in 0..count {
        source.push_str(&format!(
            "  fn function_{index}(left_{index}: i32, right_{index}: i32) -> i32\n"
        ));
    }
    source.push_str("}\n");
    source
}

criterion_group!(language_workloads, workloads);
criterion_main!(language_workloads);
