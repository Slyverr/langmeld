//! Deterministic planning and execution for optional language passes.

use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt,
};

/// Declares the artifact dependencies for one semantic or transformation pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassDescriptor {
    /// Stable identifier used in dependency declarations and error messages.
    pub id: String,
    /// Named artifacts that must exist before this pass runs.
    pub requires: Vec<String>,
    /// Named artifacts made available by this pass.
    pub provides: Vec<String>,
    /// Passes that must run first, even when no artifact dependency exists.
    pub after: Vec<String>,
}

impl PassDescriptor {
    /// Creates a pass descriptor with no inputs, outputs, or ordering constraints.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            requires: Vec::new(),
            provides: Vec::new(),
            after: Vec::new(),
        }
    }

    /// Declares artifact inputs and outputs.
    #[must_use]
    pub fn artifacts(
        mut self,
        requires: impl IntoIterator<Item = impl Into<String>>,
        provides: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.requires = requires.into_iter().map(Into::into).collect();
        self.provides = provides.into_iter().map(Into::into).collect();
        self
    }

    /// Declares pass ordering constraints independent of artifact flow.
    #[must_use]
    pub fn after(mut self, passes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.after = passes.into_iter().map(Into::into).collect();
        self
    }
}

/// A configuration error found before any pass action is executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelinePlanError {
    /// A pass ID was empty or duplicated.
    InvalidPassId(String),
    /// An artifact has multiple producers, making the chosen value ambiguous.
    AmbiguousArtifactProducer(String),
    /// A requested artifact has no initial value or registered producer.
    MissingArtifact { pass: String, artifact: String },
    /// A requested pipeline output has no initial value or registered producer.
    MissingRequestedArtifact(String),
    /// An explicit pass-order dependency does not name a registered pass.
    MissingPassDependency { pass: String, dependency: String },
    /// An explicit pass-order dependency points back to the same pass.
    SelfDependency(String),
    /// One or more passes remain unscheduled because the dependency graph has a cycle.
    DependencyCycle(Vec<String>),
}

impl fmt::Display for PipelinePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPassId(id) => write!(formatter, "pass ID `{id}` is empty or duplicated"),
            Self::AmbiguousArtifactProducer(artifact) => {
                write!(formatter, "artifact `{artifact}` has multiple producers")
            }
            Self::MissingArtifact { pass, artifact } => {
                write!(
                    formatter,
                    "pass `{pass}` requires missing artifact `{artifact}`"
                )
            }
            Self::MissingRequestedArtifact(artifact) => {
                write!(formatter, "requested output `{artifact}` has no producer")
            }
            Self::MissingPassDependency { pass, dependency } => write!(
                formatter,
                "pass `{pass}` depends on unknown pass `{dependency}`"
            ),
            Self::SelfDependency(pass) => write!(formatter, "pass `{pass}` depends on itself"),
            Self::DependencyCycle(passes) => write!(
                formatter,
                "pass dependencies contain a cycle; unscheduled passes: {}",
                passes.join(", ")
            ),
        }
    }
}

impl Error for PipelinePlanError {}

/// A planning error or a pass execution failure, including the failing pass ID.
#[derive(Debug)]
pub enum PipelineError<E> {
    /// The pass declarations could not be scheduled.
    InvalidPlan(PipelinePlanError),
    /// A pass callback returned an error.
    PassFailed { pass: String, source: E },
    /// A pass callback returned artifacts that differ from its declaration.
    OutputMismatch {
        /// Pass whose runtime result violated its declaration.
        pass: String,
        /// Artifacts declared by the pass.
        expected: Vec<String>,
        /// Artifacts returned by the pass action.
        actual: Vec<String>,
    },
}

impl<E: fmt::Display> fmt::Display for PipelineError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan(error) => error.fmt(formatter),
            Self::PassFailed { pass, source } => {
                write!(formatter, "pass `{pass}` failed: {source}")
            }
            Self::OutputMismatch {
                pass,
                expected,
                actual,
            } => write!(
                formatter,
                "pass `{pass}` declared artifacts [{}] but returned [{}]",
                expected.join(", "),
                actual.join(", ")
            ),
        }
    }
}

impl<E: Error + 'static> Error for PipelineError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPlan(error) => Some(error),
            Self::PassFailed { source, .. } => Some(source),
            Self::OutputMismatch { .. } => None,
        }
    }
}

type PassAction<'a, C, E> = dyn FnMut(&mut C) -> Result<Vec<String>, E> + 'a;

struct Pass<'a, C, E> {
    descriptor: PassDescriptor,
    action: Box<PassAction<'a, C, E>>,
}

/// A typed, deterministic pass pipeline with explicit artifact and ordering contracts.
///
/// Pass actions mutate a caller-owned context; the pipeline checks their declared output IDs
/// but does not store or inspect artifact contents. If an action fails, mutations it already
/// made to the context remain. Independent passes use registration order as the tie-breaker.
///
/// ```
/// use langmeld_language::{PassDescriptor, PassPipeline};
///
/// let mut pipeline = PassPipeline::<Vec<&str>, &'static str>::new(["source"]);
/// pipeline.register(
///     PassDescriptor::new("parse").artifacts(["source"], ["syntax"]),
///     |steps| {
///         steps.push("parse");
///         Ok(vec!["syntax".to_owned()])
///     },
/// );
/// let mut steps = Vec::new();
/// pipeline.run(&mut steps).expect("the pass contract is valid");
/// assert_eq!(steps, ["parse"]);
/// ```
pub struct PassPipeline<'a, C, E> {
    initial_artifacts: BTreeSet<String>,
    passes: Vec<Pass<'a, C, E>>,
}

impl<'a, C, E> PassPipeline<'a, C, E> {
    /// Creates a pipeline with artifacts supplied by its caller before any pass runs.
    #[must_use]
    pub fn new<I, S>(initial_artifacts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            initial_artifacts: initial_artifacts.into_iter().map(Into::into).collect(),
            passes: Vec::new(),
        }
    }

    /// Adds a pass action with a declaration of its required and produced artifacts.
    pub fn register<F>(&mut self, descriptor: PassDescriptor, action: F)
    where
        F: FnMut(&mut C) -> Result<Vec<String>, E> + 'a,
    {
        self.passes.push(Pass {
            descriptor,
            action: Box::new(action),
        });
    }

    /// Returns the deterministic execution order without running pass actions.
    ///
    /// # Errors
    /// Returns the first invalid declaration or dependency cycle.
    pub fn plan(&self) -> Result<Vec<String>, PipelinePlanError> {
        self.schedule().map(|order| {
            order
                .into_iter()
                .map(|index| self.passes[index].descriptor.id.clone())
                .collect()
        })
    }

    /// Returns the pass order needed for requested artifacts and their dependencies.
    ///
    /// The complete pipeline is validated first, including passes outside the selected
    /// dependency closure. Dependencies include both required artifacts and explicit `after`
    /// constraints.
    ///
    /// # Errors
    /// Returns an invalid pipeline declaration or a requested artifact without a producer.
    pub fn plan_for<I, S>(&self, outputs: I) -> Result<Vec<String>, PipelinePlanError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let order = self.schedule()?;
        let selected = self.required_passes(outputs)?;
        Ok(order
            .into_iter()
            .filter(|index| selected.contains(index))
            .map(|index| self.passes[index].descriptor.id.clone())
            .collect())
    }

    /// Plans and runs each pass in order, stopping at the first failure.
    ///
    /// # Errors
    /// Returns a planning error before execution, or the failing pass ID and error.
    pub fn run(&mut self, context: &mut C) -> Result<(), PipelineError<E>> {
        let order = self.schedule().map_err(PipelineError::InvalidPlan)?;
        self.run_order(context, order)
    }

    /// Plans and runs only the passes needed to produce the requested artifacts.
    ///
    /// # Errors
    /// Returns a planning error before execution, or the failing pass ID and error.
    pub fn run_for<I, S>(&mut self, context: &mut C, outputs: I) -> Result<(), PipelineError<E>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let order = self.schedule().map_err(PipelineError::InvalidPlan)?;
        let selected = self
            .required_passes(outputs)
            .map_err(PipelineError::InvalidPlan)?;
        let order = order
            .into_iter()
            .filter(|index| selected.contains(index))
            .collect();
        self.run_order(context, order)
    }

    fn run_order(&mut self, context: &mut C, order: Vec<usize>) -> Result<(), PipelineError<E>> {
        for index in order {
            let pass = &mut self.passes[index];
            let mut actual =
                (pass.action)(context).map_err(|source| PipelineError::PassFailed {
                    pass: pass.descriptor.id.clone(),
                    source,
                })?;
            let mut expected = pass.descriptor.provides.clone();
            expected.sort();
            actual.sort();
            if actual != expected {
                return Err(PipelineError::OutputMismatch {
                    pass: pass.descriptor.id.clone(),
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }

    fn required_passes<I, S>(&self, outputs: I) -> Result<BTreeSet<usize>, PipelinePlanError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let producers = self
            .passes
            .iter()
            .enumerate()
            .flat_map(|(index, pass)| {
                pass.descriptor
                    .provides
                    .iter()
                    .map(move |artifact| (artifact.as_str(), index))
            })
            .collect::<HashMap<_, _>>();
        let passes_by_id = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, pass)| (pass.descriptor.id.as_str(), index))
            .collect::<HashMap<_, _>>();
        let mut selected = BTreeSet::new();
        let mut pending = Vec::new();
        for output in outputs {
            let output = output.into();
            if self.initial_artifacts.contains(&output) {
                continue;
            }
            let Some(producer) = producers.get(output.as_str()) else {
                return Err(PipelinePlanError::MissingRequestedArtifact(output));
            };
            pending.push(*producer);
        }
        while let Some(index) = pending.pop() {
            if !selected.insert(index) {
                continue;
            }
            let descriptor = &self.passes[index].descriptor;
            for artifact in &descriptor.requires {
                if self.initial_artifacts.contains(artifact) {
                    continue;
                }
                let Some(producer) = producers.get(artifact.as_str()) else {
                    return Err(PipelinePlanError::MissingArtifact {
                        pass: descriptor.id.clone(),
                        artifact: artifact.clone(),
                    });
                };
                pending.push(*producer);
            }
            for dependency in &descriptor.after {
                if let Some(dependency) = passes_by_id.get(dependency.as_str()) {
                    pending.push(*dependency);
                }
            }
        }
        Ok(selected)
    }

    fn schedule(&self) -> Result<Vec<usize>, PipelinePlanError> {
        let mut pass_indices = HashMap::with_capacity(self.passes.len());
        let mut producers = HashMap::new();
        for (index, pass) in self.passes.iter().enumerate() {
            let id = &pass.descriptor.id;
            if id.trim().is_empty() || pass_indices.insert(id.as_str(), index).is_some() {
                return Err(PipelinePlanError::InvalidPassId(id.clone()));
            }
            for artifact in &pass.descriptor.provides {
                if self.initial_artifacts.contains(artifact)
                    || producers.insert(artifact.as_str(), index).is_some()
                {
                    return Err(PipelinePlanError::AmbiguousArtifactProducer(
                        artifact.clone(),
                    ));
                }
            }
        }

        let mut dependents = vec![BTreeSet::new(); self.passes.len()];
        let mut remaining = vec![0; self.passes.len()];
        for (index, pass) in self.passes.iter().enumerate() {
            let id = &pass.descriptor.id;
            for artifact in &pass.descriptor.requires {
                if self.initial_artifacts.contains(artifact) {
                    continue;
                }
                let Some(&producer) = producers.get(artifact.as_str()) else {
                    return Err(PipelinePlanError::MissingArtifact {
                        pass: id.clone(),
                        artifact: artifact.clone(),
                    });
                };
                add_dependency(index, producer, id, &mut dependents, &mut remaining)?;
            }
            for dependency in &pass.descriptor.after {
                let Some(&prerequisite) = pass_indices.get(dependency.as_str()) else {
                    return Err(PipelinePlanError::MissingPassDependency {
                        pass: id.clone(),
                        dependency: dependency.clone(),
                    });
                };
                add_dependency(index, prerequisite, id, &mut dependents, &mut remaining)?;
            }
        }

        let mut ready: BTreeSet<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count == 0).then_some(index))
            .collect();
        let mut order = Vec::with_capacity(self.passes.len());
        while let Some(index) = ready.pop_first() {
            order.push(index);
            for &dependent in &dependents[index] {
                remaining[dependent] -= 1;
                if remaining[dependent] == 0 {
                    ready.insert(dependent);
                }
            }
        }
        if order.len() != self.passes.len() {
            let unscheduled = remaining
                .iter()
                .enumerate()
                .filter(|(_, count)| **count > 0)
                .map(|(index, _)| self.passes[index].descriptor.id.clone())
                .collect();
            return Err(PipelinePlanError::DependencyCycle(unscheduled));
        }
        Ok(order)
    }
}

fn add_dependency(
    dependent: usize,
    prerequisite: usize,
    dependent_id: &str,
    dependents: &mut [BTreeSet<usize>],
    remaining: &mut [usize],
) -> Result<(), PipelinePlanError> {
    if dependent == prerequisite {
        return Err(PipelinePlanError::SelfDependency(dependent_id.to_owned()));
    }
    if dependents[prerequisite].insert(dependent) {
        remaining[dependent] += 1;
    }
    Ok(())
}
