// The grammar has no `Square`, as if `Rect` had been renamed.
use typed::syntax::Square;

fn main() {
    let _ = std::any::type_name::<Square>();
}
