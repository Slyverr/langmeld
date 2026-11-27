//! Calc's trees read through the types written from `calc.langmeld`.

use calc::syntax::{Expr, File, Item, language};
use langmeld::ast::AstNode;
use langmeld::{FileId, SourceFile, parse};

fn file(text: &str) -> File {
    let source = SourceFile::new(FileId::new(0), "a.calc", text).expect("small");
    let parsed = parse(language(), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    File::cast(parsed.syntax()).expect("a file")
}

#[test]
fn items_expose_their_parts() {
    let root = file("let x = 1 + 2;\nfn f(a, b) { a }\nf(x, 3);");
    let items: Vec<_> = root.items().collect();
    let Item::LetStmt(binding) = &items[0] else {
        panic!("a let");
    };
    assert_eq!(binding.name().expect("a name").syntax().text(), "x");
    let Some(Expr::Add(sum)) = binding.value() else {
        panic!("a sum");
    };
    assert_eq!(sum.operator().expect("an operator").text(), "+");
    assert_eq!(sum.lhs().expect("lhs").syntax().text(), "1");

    let Item::FnDecl(function) = &items[1] else {
        panic!("a function");
    };
    let names: Vec<_> = function
        .params()
        .filter_map(|param| param.name())
        .map(|name| name.syntax().text().to_string())
        .collect();
    assert_eq!(names, ["a", "b"]);
    assert_eq!(function.body().expect("a body").stmts().count(), 1);

    let Item::ExprStmt(call) = &items[2] else {
        panic!("an expression");
    };
    let Some(Expr::Call(call)) = call.expr() else {
        panic!("a call");
    };
    assert_eq!(call.args().count(), 2);
}

#[test]
fn if_has_a_condition_and_two_blocks() {
    let root = file("if 1 < 2 { 3 } else { 4 };");
    let Some(Item::ExprStmt(statement)) = root.items().next() else {
        panic!("an expression");
    };
    let Some(Expr::If(branch)) = statement.expr() else {
        panic!("an if");
    };
    assert!(matches!(branch.cond(), Some(Expr::Cmp(_))));
    assert_eq!(branch.then().expect("then").syntax().text(), "{ 3 }");
    assert_eq!(branch.r#else().expect("else").syntax().text(), "{ 4 }");
    let without = file("if true { 1 };");
    let Some(Item::ExprStmt(statement)) = without.items().next() else {
        panic!("an expression");
    };
    let Some(Expr::If(branch)) = statement.expr() else {
        panic!("an if");
    };
    assert!(branch.r#else().is_none());
}
