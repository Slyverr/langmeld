//! What `bind` blocks say, and the lines of them that are refused.

use langmeld_meta::{Binding, Source, load_language};

const LISTED: &str = "langmeld 1\nlanguage T { extensions \"t\" }\n\
    tokens { skip ws = / +/ ident = preset ident(ascii) string = preset string(double) }\n\
    rule File = Item*\nrule Item = Use | Let | Get\n\
    rule Use = \"use\" path:string target:ident\n\
    rule Let = \"let\" title:ident\n\
    rule Get = object:ident \".\" member:ident\n";

fn load(bind: &str) -> Result<langmeld_meta::Loaded, String> {
    let text = format!("{LISTED}bind {{\n{bind}\n}}\n");
    load_language(Source::text("t.langmeld", text)).map_err(|error| error.rendered())
}

fn refused(bind: &str) -> String {
    match load(bind) {
        Ok(_) => panic!("accepted"),
        Err(rendered) => rendered,
    }
}

#[test]
fn right_lines_are_accepted_and_kept() {
    let loaded = load(
        "  scope File\n  Let.title defines value after unique\n  Use.path imports value from Use.path\n\
         Get.member refers member of object\n  Get.object refers value\n  Use.target includes members of path\n",
    )
    .expect("accepted");
    assert_eq!(loaded.bindings.rules.len(), 6);
    assert!(matches!(
        &loaded.bindings.rules[3],
        Binding::Member { targets, of, .. }
            if targets[0].parts == ["Get", "member"] && of.parts == ["object"]
    ));
}

#[test]
fn a_node_that_does_not_exist_is_refused_once_with_the_closest() {
    assert_eq!(
        refused("  Let.title defines value\n  Itme.title defines value\n  scope Itme, File"),
        concat!(
            "error[LM-F018]: `defines` names `Itme`, which is not a rule or expression of the language\n",
            "  --> t.langmeld:11:3\n",
            "   |\n",
            "11 |   Itme.title defines value\n",
            "   |   ^^^^^^^^^^\n",
            "   = note: To fix this, write the name of a rule, or of an expression, as declared in the file; perhaps you meant `Item`.\n",
            "\n",
            "error[LM-F018]: `scope` names `Itme`, which is not a rule or expression of the language\n",
            "  --> t.langmeld:12:3\n",
            "   |\n",
            "12 |   scope Itme, File\n",
            "   |   ^^^^^^^^^^^^^^^^\n",
            "   = note: To fix this, write the name of a rule, or of an expression, as declared in the file; perhaps you meant `Item`.\n",
            "\n",
        )
    );
}

#[test]
fn a_part_that_does_not_exist_is_refused_with_the_parts_there_are() {
    assert_eq!(
        refused("  Let.name defines value"),
        concat!(
            "error[LM-F019]: `Let` has no part labelled `name`\n",
            "  --> t.langmeld:10:3\n",
            "   |\n",
            "10 |   Let.name defines value\n",
            "   |   ^^^^^^^^\n",
            "   = note: To fix this, label the part in the rule, as in `name:ident`, or name another part; its labelled parts are `title`.\n",
            "\n",
        )
    );
}

#[test]
fn a_path_of_three_parts_is_refused() {
    assert_eq!(
        refused("  Let.title.more defines value"),
        concat!(
            "error[LM-F019]: `Let.title.more` is not a node, nor a part of one\n",
            "  --> t.langmeld:10:3\n",
            "   |\n",
            "10 |   Let.title.more defines value\n",
            "   |   ^^^^^^^^^^^^^^\n",
            "   = note: To fix this, write a node, or a node and the label of one of its parts, as in `Let.title`.\n",
            "\n",
        )
    );
}

#[test]
fn the_other_part_of_a_line_must_be_labelled_in_the_node() {
    assert_eq!(
        refused("  Use.target includes members of way"),
        concat!(
            "error[LM-F019]: `Use` has no part labelled `way`\n",
            "  --> t.langmeld:10:34\n",
            "   |\n",
            "10 |   Use.target includes members of way\n",
            "   |                                  ^^^\n",
            "   = note: To fix this, label the part in the rule, as in `way:ident`, or name another part; its labelled parts are `path`, `target`.\n",
            "\n",
        )
    );
}

#[test]
fn the_other_part_must_belong_to_the_same_node() {
    assert_eq!(
        refused("  Use.target includes members of Let.title"),
        concat!(
            "error[LM-F021]: `Let.title` is a part of `Let`, but the line is about `Use`\n",
            "  --> t.langmeld:10:34\n",
            "   |\n",
            "10 |   Use.target includes members of Let.title\n",
            "   |                                  ^^^^^^^^^\n",
            "   = note: To fix this, name a part of the same node as the line's, or write only the label of the part.\n",
            "\n",
        )
    );
}

#[test]
fn words_that_contradict_each_other_are_refused() {
    assert_eq!(
        refused("  Let.title defines value after hoisted"),
        concat!(
            "error[LM-F020]: `after` and `hoisted` contradict each other\n",
            "  --> t.langmeld:10:3\n",
            "   |\n",
            "10 |   Let.title defines value after hoisted\n",
            "   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^\n",
            "   = note: To fix this, keep one of them: `after` shows a name only after its node, and `hoisted` in the whole scope.\n",
            "\n",
        )
    );
}
