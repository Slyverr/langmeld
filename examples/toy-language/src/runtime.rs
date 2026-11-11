use std::collections::HashMap;

#[derive(Default)]
pub(super) struct RuntimeState {
    pub(super) variables: HashMap<String, String>,
    pub(super) output: String,
}
