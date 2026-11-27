//! Parsing speed as a grammar grows: the same input, with 5, 50 and 300 statement
//! rules to choose from.

use std::fmt::Write;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use langmeld_syntax::pattern::{choice, lit, many, r};
use langmeld_syntax::{
    Assoc, ExprSpec, FileId, IdentStyle, IntPreset, Language, Preset, SourceFile, Token, parse, seq,
};

/// A language with `rules` kinds of statement, each starting with its own word.
fn language(rules: usize) -> Language {
    let mut builder = Language::builder("Scale")
        .token(Token::skip("space", r"\s+"))
        .token(Token::preset("name", Preset::Ident(IdentStyle::Ascii)))
        .token(Token::preset("int", Preset::Int(IntPreset::default())))
        .rule("File", many(r("Statement")))
        .rule("Statement", choice((0..rules).map(|n| r(format!("S{n}")))));
    for n in 0..rules {
        builder = builder.rule(
            format!("S{n}"),
            seq![
                lit(format!("word{n}")),
                r("name"),
                lit("="),
                r("Expr"),
                lit(";")
            ],
        );
    }
    builder
        .expr(
            ExprSpec::new("Expr")
                .atom("Number", r("int"))
                .atom("Name", r("name"))
                .infix("Product", ["*"], 2, Assoc::Left)
                .infix("Sum", ["+"], 1, Assoc::Left),
        )
        .build()
        .expect("the grammar is valid")
}

/// Statements that use the first `used` rules in turn.
fn input(used: usize) -> SourceFile {
    let mut text = String::new();
    for n in 0.. {
        if text.len() > 1 << 20 {
            break;
        }
        writeln!(text, "word{} total = a + {n} * b + 2;", n % used).expect("writes to a string");
    }
    SourceFile::new(FileId::new(0), "bench", text).expect("small")
}

fn scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar");
    let few = input(5);
    for rules in [5, 50, 300] {
        let language = language(rules);
        let spread = input(rules);
        for (name, file) in [("same input", &few), ("every rule used", &spread)] {
            assert!(parse(&language, file).diagnostics.is_empty());
            group.throughput(Throughput::Bytes(file.text().len() as u64));
            group.bench_with_input(BenchmarkId::new(name, rules), file, |b, file| {
                b.iter(|| parse(&language, black_box(file)));
            });
        }
    }
    group.finish();
}

criterion_group!(benches, scaling);
criterion_main!(benches);
