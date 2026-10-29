use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    FileRead,
    WebSource,
    Connection,
    ModelStep,
    OutputFile,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftStep {
    pub id: String,
    pub order: u32,
    pub app: String,
    pub action: String,
    pub capability: Capability,
    pub evidence_frames: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftVariable {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftConstant {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedStep {
    pub step_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftOption {
    pub id: String,
    pub label: String,
    pub any_is_fine: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftQuestion {
    pub id: String,
    pub prompt: String,
    pub evidence_frames: Vec<u64>,
    pub options: Vec<DraftOption>,
    pub default: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DraftFields")]
pub struct TaskDraftV2 {
    pub schema_version: u8,
    pub goal: String,
    pub steps: Vec<DraftStep>,
    pub variables: Vec<DraftVariable>,
    pub constants: Vec<DraftConstant>,
    pub output_description: String,
    pub example_output: Option<String>,
    pub trigger: String,
    pub unsupported_steps: Vec<UnsupportedStep>,
    pub ambiguities: Vec<String>,
    pub questions: Vec<DraftQuestion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftFields {
    schema_version: u8,
    goal: String,
    steps: Vec<DraftStep>,
    variables: Vec<DraftVariable>,
    constants: Vec<DraftConstant>,
    output_description: String,
    example_output: Option<String>,
    trigger: String,
    unsupported_steps: Vec<UnsupportedStep>,
    ambiguities: Vec<String>,
    questions: Vec<DraftQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct DraftError(pub &'static str);

impl TryFrom<DraftFields> for TaskDraftV2 {
    type Error = DraftError;

    fn try_from(f: DraftFields) -> Result<Self, Self::Error> {
        let draft = Self {
            schema_version: f.schema_version,
            goal: f.goal,
            steps: f.steps,
            variables: f.variables,
            constants: f.constants,
            output_description: f.output_description,
            example_output: f.example_output,
            trigger: f.trigger,
            unsupported_steps: f.unsupported_steps,
            ambiguities: f.ambiguities,
            questions: f.questions,
        };
        draft.validate()?;
        Ok(draft)
    }
}

fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4000 && !value.chars().any(char::is_control)
}

fn evidence(frames: &[u64]) -> bool {
    !frames.is_empty()
        && frames.len() <= 150
        && frames.iter().collect::<BTreeSet<_>>().len() == frames.len()
}

impl TaskDraftV2 {
    pub fn validate(&self) -> Result<(), DraftError> {
        if self.schema_version != 2
            || !text(&self.goal)
            || !text(&self.output_description)
            || !text(&self.trigger)
            || self.steps.is_empty()
            || self.steps.len() > 50
            || !(2..=4).contains(&self.questions.len())
            || self.variables.len() > 50
            || self.constants.len() > 50
            || self.ambiguities.len() > 50
            || self.ambiguities.iter().any(|s| !text(s))
            || self.example_output.as_ref().is_some_and(|s| s.len() > 8000)
        {
            return Err(DraftError("invalid draft fields or bounds"));
        }
        let mut step_ids = BTreeSet::new();
        let mut manual_ids = BTreeSet::new();
        let mut outputs = 0;
        for (i, step) in self.steps.iter().enumerate() {
            if step.order as usize != i + 1
                || !text(&step.id)
                || !text(&step.app)
                || !text(&step.action)
                || !evidence(&step.evidence_frames)
                || !step_ids.insert(&step.id)
            {
                return Err(DraftError("steps must be ordered, unique and grounded"));
            }
            if step.capability == Capability::Manual {
                manual_ids.insert(&step.id);
            }
            if step.capability == Capability::OutputFile {
                outputs += 1;
            }
        }
        if outputs != 1 {
            return Err(DraftError(
                "exactly one create-only output file is required",
            ));
        }
        let mut unsupported_ids = BTreeSet::new();
        for unsupported in &self.unsupported_steps {
            if !text(&unsupported.reason) || !unsupported_ids.insert(&unsupported.step_id) {
                return Err(DraftError("invalid unsupported step"));
            }
        }
        if unsupported_ids != manual_ids {
            return Err(DraftError(
                "all manual steps must be listed in unsupported_steps",
            ));
        }
        let mut names = BTreeSet::new();
        for variable in &self.variables {
            if !text(&variable.name)
                || !text(&variable.description)
                || !names.insert(&variable.name)
            {
                return Err(DraftError("invalid or duplicate variable"));
            }
        }
        for constant in &self.constants {
            if !text(&constant.name) || !text(&constant.value) || !names.insert(&constant.name) {
                return Err(DraftError(
                    "constants and variables must have distinct names",
                ));
            }
        }
        let mut question_ids = BTreeSet::new();
        for question in &self.questions {
            let mut options = BTreeSet::new();
            if !text(&question.id)
                || !text(&question.prompt)
                || !evidence(&question.evidence_frames)
                || !question_ids.insert(&question.id)
                || !(2..=6).contains(&question.options.len())
                || question
                    .options
                    .iter()
                    .any(|o| !text(&o.id) || !text(&o.label) || !options.insert(&o.id))
                || !options.contains(&question.default)
                || question.options.iter().filter(|o| o.any_is_fine).count() != 1
            {
                return Err(DraftError(
                    "questions need unique options, evidence, a default and one any-is-fine choice",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_evidence(&self, known_frames: &[u64]) -> Result<(), DraftError> {
        self.validate()?;
        if self
            .steps
            .iter()
            .flat_map(|s| &s.evidence_frames)
            .chain(self.questions.iter().flat_map(|q| &q.evidence_frames))
            .any(|seq| !known_frames.contains(seq))
        {
            return Err(DraftError("draft references an unknown frame"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClarifiedTask {
    /// Review this text before passing it to AppCore::draft_task_no_input.
    pub description: String,
    pub example_output: Option<String>,
}

/// Every question needs an explicit choice, including when accepting its displayed default.
pub fn apply_answers(
    draft: &TaskDraftV2,
    answers: &BTreeMap<String, String>,
) -> Result<ClarifiedTask, DraftError> {
    draft.validate()?;
    if answers.len() != draft.questions.len()
        || answers
            .keys()
            .any(|id| !draft.questions.iter().any(|q| &q.id == id))
    {
        return Err(DraftError("answer each question exactly once"));
    }
    let mut lines = vec![draft.goal.clone(), format!("Trigger: {}", draft.trigger),
        "Use only selected files, approved web sources and approved connections. Create one new output file; never overwrite. Other-app UI actions remain manual and cannot run in the tool.".into()];
    for step in &draft.steps {
        if step.capability == Capability::Manual {
            let unsupported = draft
                .unsupported_steps
                .iter()
                .find(|s| s.step_id == step.id)
                .ok_or(DraftError("unknown manual step"))?;
            // The reason is shown to the person on the review screen; repeating it per step only bloats the goal.
            let _ = unsupported;
            lines.push(format!(
                "{}. Manual only (unsupported) — {}: {}",
                step.order, step.app, step.action
            ));
        } else {
            lines.push(format!(
                "{}. In {}: {} ({})",
                step.order,
                step.app,
                step.action,
                match step.capability {
                    Capability::FileRead => "selected file read",
                    Capability::WebSource => "approved web source",
                    Capability::Connection => "approved connection",
                    Capability::ModelStep => "model step",
                    Capability::OutputFile => "one create-only output file",
                    Capability::Manual => unreachable!(),
                }
            ));
        }
    }
    for variable in &draft.variables {
        lines.push(format!(
            "Changes per run — {}: {}",
            variable.name, variable.description
        ));
    }
    for constant in &draft.constants {
        lines.push(format!("Constant — {}: {}", constant.name, constant.value));
    }
    lines.push(format!("Output: {}", draft.output_description));
    for question in &draft.questions {
        let selected = question
            .options
            .iter()
            .find(|o| answers.get(&question.id) == Some(&o.id))
            .ok_or(DraftError("answer does not match a question option"))?;
        lines.push(format!(
            "Decision — {}: {}{}",
            question.prompt,
            selected.label,
            if selected.any_is_fine {
                " (any is fine; builder may choose within approved scope)"
            } else {
                ""
            }
        ));
    }
    for ambiguity in &draft.ambiguities {
        lines.push(format!("Unresolved ambiguity: {ambiguity}"));
    }
    let description = lines.join("\n");
    // The builder truncates its goal at GOAL_LIMIT. Never silently lose decisions.
    if description.chars().count() > crate::GOAL_LIMIT {
        return Err(DraftError(
            "clarified task is too long for the builder; discard the recording and record a shorter task",
        ));
    }
    Ok(ClarifiedTask {
        description,
        example_output: draft.example_output.clone(),
    })
}
