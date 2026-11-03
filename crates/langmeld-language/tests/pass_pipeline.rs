use langmeld_language::{PassDescriptor, PassPipeline, PipelineError, PipelinePlanError};

#[test]
fn pass_pipeline_orders_artifact_dependencies_deterministically() {
    let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new(["source"]);
    pipeline.register(
        PassDescriptor::new("render").artifacts(["ir"], ["artifact"]),
        |trace| {
            trace.push("render");
            Ok(vec!["artifact".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("parse").artifacts(["source"], ["ast"]),
        |trace| {
            trace.push("parse");
            Ok(vec!["ast".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("analyze").artifacts(["ast"], ["ir"]),
        |trace| {
            trace.push("analyze");
            Ok(vec!["ir".to_owned()])
        },
    );

    assert_eq!(
        pipeline.plan().expect("passes form a DAG"),
        ["parse", "analyze", "render"]
    );
    let mut trace = Vec::new();
    pipeline.run(&mut trace).expect("all pass actions succeed");
    assert_eq!(trace, ["parse", "analyze", "render"]);
}

#[test]
fn pass_pipeline_runs_only_requested_artifact_dependencies() {
    let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new(["source"]);
    pipeline.register(
        PassDescriptor::new("render").artifacts(["ir"], ["artifact"]),
        |trace| {
            trace.push("render");
            Ok(vec!["artifact".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("parse").artifacts(["source"], ["ast"]),
        |trace| {
            trace.push("parse");
            Ok(vec!["ast".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("lower").artifacts(["ast"], ["ir"]),
        |trace| {
            trace.push("lower");
            Ok(vec!["ir".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("debug-export").artifacts(["source"], ["debug-json"]),
        |trace| {
            trace.push("debug-export");
            Ok(vec!["debug-json".to_owned()])
        },
    );

    assert_eq!(
        pipeline
            .plan_for(["artifact"])
            .expect("requested artifact has a complete producer chain"),
        ["parse", "lower", "render"]
    );
    let mut trace = Vec::new();
    pipeline
        .run_for(&mut trace, ["artifact"])
        .expect("requested pass actions succeed");
    assert_eq!(trace, ["parse", "lower", "render"]);
}

#[test]
fn pass_pipeline_includes_explicit_ordering_dependencies_for_requested_outputs() {
    let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("emit").artifacts(Vec::<String>::new(), ["binary"]),
        |trace| {
            trace.push("emit");
            Ok(vec!["binary".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("validate").artifacts(Vec::<String>::new(), ["checked"]),
        |trace| {
            trace.push("validate");
            Ok(vec!["checked".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("package")
            .artifacts(["binary"], ["package"])
            .after(["validate"]),
        |trace| {
            trace.push("package");
            Ok(vec!["package".to_owned()])
        },
    );

    let mut trace = Vec::new();
    pipeline
        .run_for(&mut trace, ["package"])
        .expect("requested pass includes its explicit dependency");
    assert_eq!(trace, ["emit", "validate", "package"]);
}

#[test]
fn pass_pipeline_reports_requested_outputs_without_a_producer() {
    let pipeline = PassPipeline::<(), &'static str>::new(["source"]);

    assert_eq!(
        pipeline.plan_for(["artifact"]),
        Err(PipelinePlanError::MissingRequestedArtifact(
            "artifact".to_owned()
        ))
    );
}

#[test]
fn pass_pipeline_rejects_missing_inputs_before_running_actions() {
    let mut pipeline = PassPipeline::<bool, &'static str>::new(["source"]);
    pipeline.register(
        PassDescriptor::new("lower").artifacts(["semantic-model"], ["output"]),
        |ran| {
            *ran = true;
            Ok(vec!["output".to_owned()])
        },
    );

    let mut ran = false;
    let error = pipeline
        .run(&mut ran)
        .expect_err("required input is undeclared");
    assert!(!ran, "invalid pass plans must be rejected before execution");
    assert!(matches!(
        error,
        PipelineError::InvalidPlan(PipelinePlanError::MissingArtifact { .. })
    ));
}

#[test]
fn pass_pipeline_rejects_ambiguous_artifact_producers() {
    let mut pipeline = PassPipeline::<(), &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("parse").artifacts(Vec::<String>::new(), ["ast"]),
        |_| Ok(vec!["ast".to_owned()]),
    );
    pipeline.register(
        PassDescriptor::new("custom-parse").artifacts(Vec::<String>::new(), ["ast"]),
        |_| Ok(vec!["ast".to_owned()]),
    );

    assert_eq!(
        pipeline.plan(),
        Err(PipelinePlanError::AmbiguousArtifactProducer(
            "ast".to_owned()
        ))
    );
}

#[test]
fn pass_pipeline_rejects_artifact_cycles() {
    let mut pipeline = PassPipeline::<(), &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("parse").artifacts(["ir"], ["ast"]),
        |_| Ok(vec!["ast".to_owned()]),
    );
    pipeline.register(
        PassDescriptor::new("lower").artifacts(["ast"], ["ir"]),
        |_| Ok(vec!["ir".to_owned()]),
    );

    assert!(matches!(
        pipeline.plan(),
        Err(PipelinePlanError::DependencyCycle(passes)) if passes == ["parse", "lower"]
    ));
}

#[test]
fn pass_pipeline_honors_explicit_ordering_without_artifact_edges() {
    let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("render")
            .artifacts(Vec::<String>::new(), ["image"])
            .after(["validate"]),
        |trace| {
            trace.push("render");
            Ok(vec!["image".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("validate").artifacts(Vec::<String>::new(), ["checked"]),
        |trace| {
            trace.push("validate");
            Ok(vec!["checked".to_owned()])
        },
    );
    let mut trace = Vec::new();

    pipeline
        .run(&mut trace)
        .expect("ordering dependency is valid");
    assert_eq!(trace, ["validate", "render"]);
}

#[test]
fn pass_pipeline_rejects_unknown_explicit_dependencies() {
    let mut pipeline = PassPipeline::<(), &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("render")
            .artifacts(Vec::<String>::new(), Vec::<String>::new())
            .after(["validate"]),
        |_| Ok(Vec::new()),
    );

    assert_eq!(
        pipeline.plan(),
        Err(PipelinePlanError::MissingPassDependency {
            pass: "render".to_owned(),
            dependency: "validate".to_owned(),
        })
    );
}

#[test]
fn pass_pipeline_stops_after_failure_and_identifies_the_pass() {
    let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("load").artifacts(Vec::<String>::new(), ["loaded"]),
        |trace| {
            trace.push("load");
            Err("invalid source")
        },
    );
    pipeline.register(
        PassDescriptor::new("run").artifacts(["loaded"], ["result"]),
        |trace| {
            trace.push("run");
            Ok(vec!["result".to_owned()])
        },
    );
    let mut trace = Vec::new();

    let error = pipeline.run(&mut trace).expect_err("load pass fails");
    assert!(matches!(
        error,
        PipelineError::PassFailed {
            pass,
            source: "invalid source"
        } if pass == "load"
    ));
    assert_eq!(trace, ["load"]);
}

#[test]
fn pass_pipeline_rejects_runtime_outputs_that_break_the_declaration() {
    let mut pipeline = PassPipeline::<(), &'static str>::new([] as [&str; 0]);
    pipeline.register(
        PassDescriptor::new("parse").artifacts(Vec::<String>::new(), ["syntax"]),
        |_| Ok(vec!["tokens".to_owned()]),
    );

    assert!(matches!(
        pipeline.run(&mut ()),
        Err(PipelineError::OutputMismatch {
            pass,
            expected,
            actual
        }) if pass == "parse" && expected == ["syntax"] && actual == ["tokens"]
    ));
}

#[test]
fn pass_pipeline_rejects_empty_and_duplicate_ids_before_running_actions() {
    let mut empty_id = PassPipeline::<bool, &'static str>::new([] as [&str; 0]);
    empty_id.register(PassDescriptor::new("  "), |ran| {
        *ran = true;
        Ok(Vec::new())
    });
    let mut ran = false;
    assert!(matches!(
        empty_id.run(&mut ran),
        Err(PipelineError::InvalidPlan(PipelinePlanError::InvalidPassId(id))) if id == "  "
    ));
    assert!(!ran);

    let mut duplicate_ids = PassPipeline::<bool, &'static str>::new([] as [&str; 0]);
    duplicate_ids.register(PassDescriptor::new("same"), |_| Ok(Vec::new()));
    duplicate_ids.register(PassDescriptor::new("same"), |ran| {
        *ran = true;
        Ok(Vec::new())
    });
    assert!(matches!(
        duplicate_ids.run(&mut ran),
        Err(PipelineError::InvalidPlan(PipelinePlanError::InvalidPassId(id))) if id == "same"
    ));
    assert!(!ran);
}

#[test]
fn pass_pipeline_does_not_run_producers_for_requested_initial_artifacts() {
    let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new(["syntax"]);
    pipeline.register(
        PassDescriptor::new("debug-export").artifacts(["syntax"], ["debug-json"]),
        |trace| {
            trace.push("debug-export");
            Ok(vec!["debug-json".to_owned()])
        },
    );

    assert!(
        pipeline
            .plan_for(["syntax"])
            .expect("initial artifacts need no producer")
            .is_empty()
    );
    let mut trace = Vec::new();
    pipeline
        .run_for(&mut trace, ["syntax"])
        .expect("the requested artifact already exists");
    assert!(trace.is_empty());
}
