use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct MarkerBlock {
    start: usize,
    content_start: usize,
    content_end: usize,
    end: usize,
    parent: Option<String>,
}

fn marker_blocks(source: &str) -> Result<BTreeMap<String, MarkerBlock>, String> {
    let mut blocks = BTreeMap::new();
    let mut open = Vec::<(String, usize, usize, Option<String>)>::new();
    let mut offset = 0;

    for line in source.split_inclusive('\n') {
        let marker = line.trim();
        if let Some(key) = marker
            .strip_prefix("// <langmeld:")
            .and_then(|value| value.strip_suffix('>'))
        {
            if key.is_empty() || key.contains('<') || key.contains('>') {
                return Err("host scaffold has a malformed opening marker".to_owned());
            }
            let parent = open.last().map(|(key, _, _, _)| key.clone());
            open.push((key.to_owned(), offset, offset + line.len(), parent));
        } else if let Some(key) = marker
            .strip_prefix("// </langmeld:")
            .and_then(|value| value.strip_suffix('>'))
        {
            let Some((open_key, start, content_start, parent)) = open.pop() else {
                return Err(format!(
                    "host scaffold has an unmatched `{key}` closing marker"
                ));
            };
            if open_key != key {
                return Err(format!(
                    "host scaffold marker `{open_key}` closes as `{key}`"
                ));
            }
            let mut block_end = offset + line.len();
            if open.is_empty() {
                while source[block_end..].starts_with('\n') {
                    block_end += 1;
                }
            }
            if blocks
                .insert(
                    key.to_owned(),
                    MarkerBlock {
                        start,
                        content_start,
                        content_end: offset,
                        end: block_end,
                        parent,
                    },
                )
                .is_some()
            {
                return Err(format!("host scaffold marker `{key}` is duplicated"));
            }
        }
        offset += line.len();
    }
    if let Some((key, _, _, _)) = open.last() {
        return Err(format!("host scaffold marker `{key}` is not closed"));
    }
    if source[offset..].contains("// </langmeld:") {
        return Err("host scaffold has an unmatched closing marker".to_owned());
    }
    for (key, block) in &blocks {
        match (&block.parent, key.starts_with("body:")) {
            (None, false) => {}
            (Some(parent), true) if parent.starts_with("callback:") => {}
            _ => {
                return Err(format!(
                    "host scaffold marker `{key}` is nested incorrectly"
                ));
            }
        }
    }
    Ok(blocks)
}

fn without_top_level_blocks(source: &str, blocks: &BTreeMap<String, MarkerBlock>) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut top_level = blocks
        .values()
        .filter(|block| block.parent.is_none())
        .collect::<Vec<_>>();
    top_level.sort_by_key(|block| block.start);
    for block in top_level {
        output.push_str(&source[cursor..block.start]);
        cursor = block.end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn normalized_callback(
    source: &str,
    blocks: &BTreeMap<String, MarkerBlock>,
    key: &str,
) -> Result<String, String> {
    let callback = blocks
        .get(key)
        .ok_or_else(|| format!("host callback `{key}` is missing"))?;
    let mut body_blocks = blocks
        .iter()
        .filter(|(_, block)| block.parent.as_deref() == Some(key))
        .collect::<Vec<_>>();
    if body_blocks.len() != 1 {
        return Err(format!(
            "host callback `{key}` must have exactly one body marker"
        ));
    }
    let (body_key, body) = body_blocks.pop().expect("length checked above");
    if !body_key.starts_with("body:") {
        return Err(format!("host callback `{key}` has an invalid body marker"));
    }
    let mut normalized = String::with_capacity(callback.end - callback.start);
    normalized.push_str(&source[callback.start..body.content_start]);
    normalized.push_str("<callback-body>");
    normalized.push_str(&source[body.content_end..callback.end]);
    Ok(normalized)
}

fn registry_constructor(key: &str) -> Option<&'static str> {
    match key {
        "registry:parser" => Some("CustomConstructParser::new("),
        "registry:expression_atom_parser" => Some("ExpressionAtomParserRegistration::new("),
        "registry:semantic" => Some("ConfiguredSemanticHookRegistration::new("),
        "registry:resolver" => Some("WorkspaceSymbolResolverRegistration::new("),
        "registry:workspace" => Some("WorkspaceTypeAnalysisRegistration::new("),
        _ => None,
    }
}

fn registry_parts(
    source: &str,
    constructor: &str,
) -> Result<(String, BTreeMap<String, String>), String> {
    let mut shell = String::new();
    let mut entries = BTreeMap::new();
    for line in source.lines() {
        if let Some(start) = line.find(constructor) {
            let args = line[start + constructor.len()..].trim_start();
            let args = args
                .strip_prefix('"')
                .ok_or_else(|| "host registry has an invalid callback entry".to_owned())?;
            let end = args
                .find('"')
                .ok_or_else(|| "host registry callback entry has no closing quote".to_owned())?;
            let id = args[..end].to_owned();
            let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if entries.insert(id.clone(), normalized).is_some() {
                return Err(format!("host registry callback `{id}` is duplicated"));
            }
        } else {
            shell.push_str(line);
            shell.push('\n');
        }
    }
    Ok((shell, entries))
}

fn callback_ids(blocks: &BTreeMap<String, MarkerBlock>, prefix: &str) -> BTreeMap<String, String> {
    blocks
        .iter()
        .filter_map(|(key, block)| {
            (block.parent.is_none())
                .then(|| {
                    key.strip_prefix(prefix)
                        .map(|id| (id.to_owned(), key.clone()))
                })
                .flatten()
        })
        .collect()
}

fn validate_registries(
    existing_source: &str,
    existing: &BTreeMap<String, MarkerBlock>,
    generated_source: &str,
    generated: &BTreeMap<String, MarkerBlock>,
) -> Result<(), String> {
    let old_registry_keys = existing
        .iter()
        .filter(|(key, block)| key.starts_with("registry:") && block.parent.is_none())
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    let new_registry_keys = generated
        .iter()
        .filter(|(key, block)| key.starts_with("registry:") && block.parent.is_none())
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    if old_registry_keys
        .iter()
        .any(|registry| !new_registry_keys.contains(registry))
    {
        return Err("host registry was removed; migrate it manually".to_owned());
    }

    for key in new_registry_keys {
        let constructor =
            registry_constructor(key).ok_or_else(|| format!("unknown host registry `{key}`"))?;
        let new_block = &generated[key];
        let (new_shell, new_entries) = registry_parts(
            &generated_source[new_block.content_start..new_block.content_end],
            constructor,
        )?;
        let old_entries = if let Some(old_block) = existing.get(key) {
            let (old_shell, old_entries) = registry_parts(
                &existing_source[old_block.content_start..old_block.content_end],
                constructor,
            )?;
            if old_shell != new_shell {
                return Err(format!(
                    "host registry `{key}` was edited; restore its generated structure before updating"
                ));
            }
            old_entries
        } else {
            BTreeMap::new()
        };

        let prefix = match key.as_str() {
            "registry:parser" => "callback:parser:",
            "registry:expression_atom_parser" => "callback:expression_atom_parser:",
            "registry:semantic" => "callback:semantic:",
            "registry:resolver" => "callback:resolver:",
            "registry:workspace" => "callback:workspace:",
            _ => unreachable!("registry constructor restricts registry names"),
        };
        let old_callbacks = callback_ids(existing, prefix)
            .into_keys()
            .collect::<std::collections::BTreeSet<_>>();
        let new_callbacks = callback_ids(generated, prefix)
            .into_keys()
            .collect::<std::collections::BTreeSet<_>>();
        if old_entries
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            != old_callbacks
            || new_entries
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                != new_callbacks
        {
            return Err(format!(
                "host registry `{key}` does not match its generated callbacks"
            ));
        }
        for id in old_entries.keys() {
            if new_entries
                .get(id)
                .is_some_and(|new_entry| old_entries[id] != *new_entry)
            {
                return Err(format!(
                    "host registry entry `{id}` was edited; restore its generated registration before updating"
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn merge_host_scaffold(
    existing_source: &str,
    generated_source: &str,
) -> Result<String, String> {
    let existing = marker_blocks(existing_source)?;
    let generated = marker_blocks(generated_source)?;
    if without_top_level_blocks(existing_source, &existing)
        != without_top_level_blocks(generated_source, &generated)
    {
        return Err("host scaffold has edits outside generated callback bodies".to_owned());
    }
    validate_registries(existing_source, &existing, generated_source, &generated)?;

    let old_callbacks = existing
        .iter()
        .filter(|(key, block)| key.starts_with("callback:") && block.parent.is_none())
        .map(|(key, _)| key.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let new_callbacks = generated
        .iter()
        .filter(|(key, block)| key.starts_with("callback:") && block.parent.is_none())
        .map(|(key, _)| key.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if !old_callbacks.is_subset(&new_callbacks) {
        let removed = old_callbacks
            .difference(&new_callbacks)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "host callbacks were removed ({removed}); migrate them manually before updating"
        ));
    }

    let mut replacements = Vec::new();
    for key in &old_callbacks {
        let old_shape = normalized_callback(existing_source, &existing, key)?;
        let new_shape = normalized_callback(generated_source, &generated, key)?;
        if old_shape != new_shape {
            return Err(format!(
                "host callback `{key}` changed outside its body; migrate it manually before updating"
            ));
        }
        let body_key = existing
            .iter()
            .find(|(_, block)| block.parent.as_deref() == Some(key))
            .map(|(body_key, _)| body_key)
            .expect("normalized callback validated a body marker");
        let old_body = &existing[body_key];
        let new_body = &generated[body_key];
        replacements.push((
            new_body.content_start,
            new_body.content_end,
            existing_source[old_body.content_start..old_body.content_end].to_owned(),
        ));
    }
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut merged = generated_source.to_owned();
    for (start, end, body) in replacements {
        merged.replace_range(start..end, &body);
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::merge_host_scaffold;

    const OLD: &str = "head\n// <langmeld:registry:parser>\nvec![\n CustomConstructParser::new(\"one\", callback_one),\n]\n// </langmeld:registry:parser>\n// <langmeld:callback:parser:one>\nfn callback_one() {\n    // <langmeld:body:parser:one>\n    customized();\n    // </langmeld:body:parser:one>\n}\n// </langmeld:callback:parser:one>\n";
    const NEW: &str = "head\n// <langmeld:registry:parser>\nvec![\n CustomConstructParser::new(\"one\", callback_one),\n CustomConstructParser::new(\"two\", callback_two),\n]\n// </langmeld:registry:parser>\n// <langmeld:callback:parser:one>\nfn callback_one() {\n    // <langmeld:body:parser:one>\n    Ok(None)\n    // </langmeld:body:parser:one>\n}\n// </langmeld:callback:parser:one>\n// <langmeld:callback:parser:two>\nfn callback_two() {\n    // <langmeld:body:parser:two>\n    Ok(None)\n    // </langmeld:body:parser:two>\n}\n// </langmeld:callback:parser:two>\n";
    const NEW_WITH_EXPRESSION_ATOM: &str = "head\n// <langmeld:registry:parser>\nvec![\n CustomConstructParser::new(\"one\", callback_one),\n]\n// </langmeld:registry:parser>\n// <langmeld:registry:expression_atom_parser>\nvec![\n ExpressionAtomParserRegistration::new(\"canvas_reference\", host_expression_atom_parser_canvas_reference),\n]\n// </langmeld:registry:expression_atom_parser>\n// <langmeld:callback:parser:one>\nfn callback_one() {\n    // <langmeld:body:parser:one>\n    Ok(None)\n    // </langmeld:body:parser:one>\n}\n// </langmeld:callback:parser:one>\n// <langmeld:callback:expression_atom_parser:canvas_reference>\nfn host_expression_atom_parser_canvas_reference() {\n    // <langmeld:body:expression_atom_parser:canvas_reference>\n    Ok(None)\n    // </langmeld:body:expression_atom_parser:canvas_reference>\n}\n// </langmeld:callback:expression_atom_parser:canvas_reference>\n";

    #[test]
    fn merge_preserves_existing_body_and_adds_new_callback() {
        let merged = merge_host_scaffold(OLD, NEW).expect("compatible scaffold merges");
        assert!(merged.contains("customized();"));
        assert!(merged.contains("callback_two"));
    }

    #[test]
    fn merge_adds_a_new_expression_atom_registry_without_losing_existing_callbacks() {
        let merged = merge_host_scaffold(OLD, NEW_WITH_EXPRESSION_ATOM)
            .expect("new callback registry can be added without a manual scaffold reset");
        assert!(merged.contains("customized();"));
        assert!(merged.contains("ExpressionAtomParserRegistration::new(\"canvas_reference\""));
        assert!(merged.contains("callback:expression_atom_parser:canvas_reference"));
    }

    #[test]
    fn merge_rejects_removed_callbacks_and_edits_outside_body() {
        let removed = NEW
            .replace(" CustomConstructParser::new(\"one\", callback_one),\n", "")
            .replace(
                "// <langmeld:callback:parser:one>\nfn callback_one() {\n    // <langmeld:body:parser:one>\n    Ok(None)\n    // </langmeld:body:parser:one>\n}\n// </langmeld:callback:parser:one>\n",
                "",
            );
        let removed = merge_host_scaffold(OLD, &removed).expect_err("removal needs migration");
        assert!(removed.contains("removed"));
        let changed = OLD.replace("head\n", "edited head\n");
        assert!(merge_host_scaffold(&changed, NEW).is_err());
        let signature = OLD.replace("fn callback_one()", "fn callback_one(value: u8)");
        assert!(merge_host_scaffold(&signature, NEW).is_err());
        let registry = OLD.replace("callback_one),", "edited_callback),");
        assert!(merge_host_scaffold(&registry, NEW).is_err());
    }

    #[test]
    fn merge_rejects_malformed_markers() {
        let malformed = OLD.replace(
            "// </langmeld:body:parser:one>",
            "// </langmeld:body:other>",
        );
        assert!(merge_host_scaffold(&malformed, NEW).is_err());
    }
}
