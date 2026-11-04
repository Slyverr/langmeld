pub(crate) fn execute(name: &str, actual: f64, expected: f64) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "assertion failed: `{name}` is {actual}, expected {expected}"
        ))
    }
}
