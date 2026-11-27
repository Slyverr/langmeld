// The visitor has a method for each node and none has a default, so a node that the
// code never handles is an error.
use typed::syntax::{Circle, Visitor};

struct OnlyCircles;

impl Visitor for OnlyCircles {
    type Output = ();
    fn visit_circle(&mut self, _: &Circle) {}
}

fn main() {
    let _ = OnlyCircles;
}
