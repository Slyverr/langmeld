// `radius` is a number token, not a node.
use typed::syntax::{Circle, Name};

fn radius(circle: &Circle) -> Option<Name> {
    circle.radius()
}

fn main() {
    let _ = radius;
}
