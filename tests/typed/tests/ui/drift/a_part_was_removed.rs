// The grammar has no `colour` label, as if it had been removed.
use typed::syntax::Circle;

fn colour(circle: &Circle) {
    let _ = circle.colour();
}

fn main() {
    let _ = colour;
}
