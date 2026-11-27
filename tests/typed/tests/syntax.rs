//! The Rust types written for the Shapes language: how trees read through them.

use langmeld::ast::AstNode;
use langmeld::{FileId, SourceFile, parse};
use typed::syntax::{self, Expr, File, Item, Visitor, language, visit};

fn tree(text: &str) -> (langmeld::Parse, File) {
    let file = SourceFile::new(FileId::new(0), "a.shp", text).expect("small");
    let parsed = parse(language(), &file);
    let root = File::cast(parsed.syntax()).expect("a file");
    (parsed, root)
}

const SAMPLE: &str = "// shapes
circle disc radius 5;
rect size 2 by (3 + 4) * 5;
rect wide size width by 1;
group frame {
    note \"hello\";
    circle dot radius 1;
}
";

#[test]
fn nodes_give_their_labelled_parts() {
    let (parsed, root) = tree(SAMPLE);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let items: Vec<Item> = root.items().collect();
    assert_eq!(items.len(), 4);

    let Item::Circle(disc) = &items[0] else {
        panic!("a circle");
    };
    assert_eq!(disc.name().expect("named").syntax().text(), "disc");
    assert_eq!(disc.radius().expect("a radius").text(), "5");

    let Item::Rect(unnamed) = &items[1] else {
        panic!("a rect");
    };
    // The name is optional, and what follows it is not mistaken for it.
    assert!(unnamed.name().is_none());
    assert_eq!(unnamed.width().expect("a width").syntax().text(), "2");
    let Some(Expr::Mul(height)) = unnamed.height() else {
        panic!("a product");
    };
    assert_eq!(height.operator().expect("an operator").text(), "*");
    let Some(Expr::Paren(sum)) = height.lhs() else {
        panic!("parentheses");
    };
    let Some(Expr::Add(sum)) = sum.inner() else {
        panic!("a sum");
    };
    assert_eq!(sum.lhs().expect("a left side").syntax().text(), "3");
    assert_eq!(sum.rhs().expect("a right side").syntax().text(), "4");
    assert_eq!(height.rhs().expect("a right side").syntax().text(), "5");

    let Item::Rect(named) = &items[2] else {
        panic!("a rect");
    };
    assert_eq!(named.name().expect("named").syntax().text(), "wide");
    assert!(matches!(named.width(), Some(Expr::Ref(_))));

    let Item::Group(frame) = &items[3] else {
        panic!("a group");
    };
    assert_eq!(frame.name().expect("named").syntax().text(), "frame");
    let inner: Vec<_> = frame.items().collect();
    assert_eq!(inner.len(), 2);
    let Item::Note(note) = &inner[0] else {
        panic!("a note");
    };
    assert_eq!(note.body().expect("a body").text(), "\"hello\"");
}

#[test]
fn views_know_what_they_can_be_cast_from() {
    let (_, root) = tree("circle a radius 1;");
    let item = root.items().next().expect("an item");
    assert!(syntax::Circle::can_cast(item.syntax().kind()));
    assert!(!syntax::Rect::can_cast(item.syntax().kind()));
    assert!(syntax::Rect::cast(item.syntax().clone()).is_none());
    assert_eq!(item.range(), item.syntax().text_range());
    let circle = syntax::Circle::cast(item.syntax().clone()).expect("a circle");
    assert_eq!(circle.syntax(), item.syntax());
}

#[derive(Default)]
struct Counter {
    nodes: usize,
}

macro_rules! count {
    ($($method:ident: $node:ident),* $(,)?) => {
        impl Visitor for Counter {
            type Output = usize;
            $(fn $method(&mut self, _: &syntax::$node) -> usize {
                self.nodes += 1;
                1
            })*
        }
    };
}

count!(
    visit_add: Add,
    visit_circle: Circle,
    visit_file: File,
    visit_group: Group,
    visit_mul: Mul,
    visit_name: Name,
    visit_neg: Neg,
    visit_note: Note,
    visit_num: Num,
    visit_paren: Paren,
    visit_rect: Rect,
    visit_ref: Ref,
);

#[test]
fn a_visitor_is_given_each_kind_of_node() {
    let (parsed, _) = tree(SAMPLE);
    let mut counter = Counter::default();
    let visited = parsed
        .syntax()
        .descendants()
        .filter_map(|node| visit(&mut counter, node))
        .sum::<usize>();
    assert_eq!(visited, counter.nodes);
    assert!(visited > 20, "{visited}");
}

#[test]
fn trees_with_mistakes_give_nothing_instead_of_failing() {
    for text in [
        "",
        "circle",
        "circle a radius ;",
        "rect size by ;",
        "group { note }",
        "note 5 ; ( ) * + -",
        "circle a radius 99999999999999999999999 ;",
        "\u{1F600} circle \u{1F600} a radius 1 ;",
    ] {
        let (_, root) = tree(text);
        for item in root.items() {
            match item {
                Item::Circle(it) => drop((it.name(), it.radius())),
                Item::Rect(it) => drop((it.name(), it.width(), it.height())),
                Item::Group(it) => drop((it.name(), it.items().count())),
                Item::Note(it) => drop(it.body()),
            }
        }
        for node in root.syntax().descendants() {
            if let Some(Expr::Add(sum)) = Expr::cast(node) {
                drop((sum.lhs(), sum.operator(), sum.rhs()));
            }
        }
    }
}

#[test]
fn the_language_comes_with_the_types() {
    assert_eq!(language().name(), "Shapes");
    let loaded = langmeld::load_language(syntax::source()).expect("compiled in");
    assert_eq!(loaded.extensions, ["shp"]);
}
