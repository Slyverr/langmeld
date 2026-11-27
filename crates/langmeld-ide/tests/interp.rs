//! Environments, values and budgets for interpreters.

use langmeld_ide::kit::interp::{Budget, Env, Exhausted, Value};

#[test]
fn inner_scopes_hide_outer_names() {
    let mut env: Env<i32> = Env::new();
    assert_eq!(env.define("x", 1), None);
    env.push();
    assert_eq!(env.get("x"), Some(&1));
    env.define("x", 2);
    env.define("y", 3);
    assert_eq!(env.get("x"), Some(&2));
    assert!(env.set("y", 4));
    env.pop();
    assert_eq!(env.get("x"), Some(&1));
    assert_eq!(env.get("y"), None);
    assert!(!env.set("y", 5), "nothing named `y`");
    assert_eq!(env.define("x", 6), Some(1), "replaced in the same scope");
    // The outermost scope stays.
    env.pop();
    env.pop();
    assert_eq!(env.get("x"), Some(&6));
}

#[test]
fn a_function_sees_its_own_names_and_the_outermost_ones() {
    let mut env = Env::new();
    env.define("global", 1);
    env.push();
    env.define("local", 2);
    env.push_frame();
    env.define("param", 3);
    env.push();
    assert_eq!(env.get("param"), Some(&3));
    assert_eq!(env.get("global"), Some(&1));
    assert_eq!(env.get("local"), None, "of the caller");
    assert!(env.set("global", 10));
    assert!(!env.set("local", 20));
    env.pop();
    env.pop();
    assert_eq!(env.get("local"), Some(&2));
    assert_eq!(env.get("global"), Some(&10));
    assert_eq!(env.get("param"), None);
}

#[test]
fn budgets_end_runaway_programs() {
    let mut budget = Budget::new(3, 2);
    for _ in 0..3 {
        assert_eq!(budget.step(), Ok(()));
    }
    assert_eq!(budget.step(), Err(Exhausted::Steps));
    assert_eq!(budget.step(), Err(Exhausted::Steps));
    assert_eq!(budget.enter(), Ok(()));
    assert_eq!(budget.enter(), Ok(()));
    assert_eq!(budget.enter(), Err(Exhausted::Depth));
    budget.leave();
    assert_eq!(budget.enter(), Ok(()));
    assert!(Exhausted::Steps.to_string().contains("steps"));
    assert!(Exhausted::Depth.to_string().contains("deep"));
}

#[test]
fn values_print_and_name_their_types() {
    let values: [(Value, &str, &str); 5] = [
        (Value::Nothing, "nothing", "nothing"),
        (Value::Bool(true), "true", "boolean"),
        (Value::Int(-4), "-4", "whole number"),
        (Value::Float(2.0), "2.0", "number"),
        (Value::text("hi"), "hi", "text"),
    ];
    for (value, shown, kind) in values {
        assert_eq!(value.to_string(), shown);
        assert_eq!(value.type_name(), kind);
    }
}

#[test]
fn a_limit_reached_is_a_problem_to_report() {
    use langmeld_syntax::{FileId, Severity, TextRange, TextSize};

    let range = TextRange::empty(TextSize::new(3));
    let steps = Exhausted::Steps.diagnostic("X-001", FileId::new(1), range);
    assert_eq!(steps.severity, Severity::Error);
    assert_eq!(steps.message, "the program ran for too many steps");
    assert_eq!(
        steps.notes,
        ["To fix this, make the program do less, or raise the budget of steps."]
    );
    let depth = Exhausted::Depth.diagnostic("X-002", FileId::new(1), range);
    assert_eq!(depth.message, "calls go too deep");
    assert_eq!(
        depth.notes,
        ["To fix this, make the calls go less deep, or raise the budget of depth."]
    );
}
