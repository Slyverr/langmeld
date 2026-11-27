//! Parsing speed on JSON, from text to tree.

use std::fmt::Write;
use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use langmeld_syntax::{FileId, SourceFile};

/// About a mebibyte of varied JSON.
fn document() -> SourceFile {
    let mut text = String::from("[\n");
    for n in 0.. {
        if text.len() > 1 << 20 {
            break;
        }
        writeln!(
            text,
            "  {{\"id\": {n}, \"name\": \"item {n}\", \"price\": {n}.5e-1, \"tags\": [\"a\", \"b\"], \"stock\": {{\"open\": true, \"note\": null}}}},"
        )
        .expect("writes to a string");
    }
    text.push_str("  {}\n]\n");
    SourceFile::new(FileId::new(0), "bench.json", text).expect("small")
}

fn parsing(c: &mut Criterion) {
    let language = json::language();
    let file = document();
    assert!(
        langmeld_syntax::parse(language, &file)
            .diagnostics
            .is_empty()
    );

    let mut group = c.benchmark_group("json");
    group.throughput(Throughput::Bytes(file.text().len() as u64));
    group.bench_function("lex", |b| b.iter(|| language.lexer().lex(black_box(&file))));
    group.bench_function("parse", |b| {
        b.iter(|| langmeld_syntax::parse(language, black_box(&file)));
    });

    // The same document, with the grammar grown to 300 rules.
    let extra = 300 - language.schema().nodes().count();
    let large = json::builder(extra).build().expect("the grammar is valid");
    assert!(langmeld_syntax::parse(&large, &file).diagnostics.is_empty());
    group.bench_function("parse with 300 rules", |b| {
        b.iter(|| langmeld_syntax::parse(&large, black_box(&file)));
    });
    group.finish();
}

criterion_group!(benches, parsing);
criterion_main!(benches);
