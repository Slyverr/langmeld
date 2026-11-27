use langmeld::ast::AstNode;
use typed::syntax::{Circle, Expr, File, Item, Name, NumberToken, Rect, Visitor, visit};

struct Names;

impl Visitor for Names {
    type Output = ();
    fn visit_add(&mut self, _: &typed::syntax::Add) {}
    fn visit_circle(&mut self, _: &Circle) {}
    fn visit_file(&mut self, _: &File) {}
    fn visit_group(&mut self, _: &typed::syntax::Group) {}
    fn visit_mul(&mut self, _: &typed::syntax::Mul) {}
    fn visit_name(&mut self, _: &Name) {}
    fn visit_neg(&mut self, _: &typed::syntax::Neg) {}
    fn visit_note(&mut self, _: &typed::syntax::Note) {}
    fn visit_num(&mut self, _: &typed::syntax::Num) {}
    fn visit_paren(&mut self, _: &typed::syntax::Paren) {}
    fn visit_rect(&mut self, _: &Rect) {}
    fn visit_ref(&mut self, _: &typed::syntax::Ref) {}
}

fn value(expr: &Expr) -> usize {
    match expr {
        Expr::Num(_) | Expr::Ref(_) => 1,
        Expr::Paren(paren) => paren.inner().map_or(0, |inner| value(&inner)),
        Expr::Neg(neg) => neg.operand().map_or(0, |operand| value(&operand)),
        Expr::Mul(product) => product.lhs().map_or(0, |lhs| value(&lhs)),
        Expr::Add(sum) => sum.rhs().map_or(0, |rhs| value(&rhs)),
    }
}

fn radius(circle: &Circle) -> Option<NumberToken> {
    circle.radius()
}

fn kind(item: &Item) -> &'static str {
    match item {
        Item::Circle(_) => "circle",
        Item::Rect(_) => "rect",
        Item::Group(_) => "group",
        Item::Note(_) => "note",
    }
}

fn main() {
    let _ = (value, radius, kind, visit::<Names>, Names, <File as AstNode>::can_cast);
}
