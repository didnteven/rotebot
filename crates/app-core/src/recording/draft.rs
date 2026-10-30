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
        // A chore the tool cannot do at all (every step manual) has no output file.
        let all_manual = manual_ids.len() == self.steps.len();
        if outputs > 1 || (outputs == 0 && !all_manual) {
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

/// What the person changed on the plan screen: wording only. Steps can be reworded or removed,
/// but nothing can be added that the recording did not show, and the output step stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftEdit {
    pub goal: String,
    #[serde(default)]
    pub step_actions: BTreeMap<String, String>,
    #[serde(default)]
    pub removed_steps: Vec<String>,
    /// Extra instructions for the builder, kept as part of the goal.
    #[serde(default)]
    pub note: Option<String>,
}

impl TaskDraftV2 {
    pub fn edited(&self, edit: &DraftEdit) -> Result<Self, DraftError> {
        let mut draft = self.clone();
        let goal = edit.goal.trim();
        if goal.is_empty() {
            return Err(DraftError("the goal cannot be empty"));
        }
        draft.goal = goal.to_string();
        if let Some(note) = edit.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            draft.goal = format!("{} {}", draft.goal.trim_end_matches('.'), note);
        }
        for step in &mut draft.steps {
            if let Some(action) = edit.step_actions.get(&step.id) {
                step.action = action.trim().to_string();
            }
        }
        for id in &edit.removed_steps {
            let step = draft
                .steps
                .iter()
                .find(|s| &s.id == id)
                .ok_or(DraftError("unknown step"))?;
            if step.capability == Capability::OutputFile {
                return Err(DraftError("the output step cannot be removed"));
            }
        }
        draft.steps.retain(|s| !edit.removed_steps.contains(&s.id));
        draft
            .unsupported_steps
            .retain(|u| !edit.removed_steps.contains(&u.step_id));
        for (i, step) in draft.steps.iter_mut().enumerate() {
            step.order = i as u32 + 1;
        }
        draft.validate()?;
        Ok(draft)
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
    if draft.steps.iter().all(|s| s.capability == Capability::Manual) {
        return Err(DraftError(
            "A tool can't do any part of this chore: it needs clicking, typing or other apps. Nothing was built",
        ));
    }
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

#[cfg(test)]
mod edit_tests {
    use super::*;

    fn step(id: &str, order: u32, capability: Capability) -> DraftStep {
        DraftStep { id: id.into(), order, app: "App".into(), action: format!("Do {id}"), capability, evidence_frames: vec![1] }
    }
    fn question(id: &str) -> DraftQuestion {
        let opt = |o: &str, any| DraftOption { id: o.into(), label: o.into(), any_is_fine: any };
        DraftQuestion { id: id.into(), prompt: "Which?".into(), evidence_frames: vec![1], options: vec![opt("a", false), opt("b", true)], default: "a".into() }
    }
    fn draft() -> TaskDraftV2 {
        TaskDraftV2 {
            schema_version: 2,
            goal: "Summarise the inbox.".into(),
            steps: vec![step("read", 1, Capability::Connection), step("open", 2, Capability::Manual), step("save", 3, Capability::OutputFile)],
            variables: vec![],
            constants: vec![],
            output_description: "A text file".into(),
            example_output: None,
            trigger: "On demand".into(),
            unsupported_steps: vec![UnsupportedStep { step_id: "open".into(), reason: "Needs clicking".into() }],
            ambiguities: vec![],
            questions: vec![question("q1"), question("q2")],
        }
    }

    #[test]
    fn rewording_removing_and_noting_keep_the_plan_valid() {
        let edit = DraftEdit {
            goal: "Summarise the newest five emails".into(),
            step_actions: BTreeMap::from([("read".into(), "Read the five newest emails".into())]),
            removed_steps: vec!["open".into()],
            note: Some("Skip newsletters.".into()),
        };
        let out = draft().edited(&edit).unwrap();
        assert_eq!(out.goal, "Summarise the newest five emails Skip newsletters.");
        assert_eq!(out.steps.iter().map(|s| (s.id.as_str(), s.order)).collect::<Vec<_>>(), [("read", 1), ("save", 2)]);
        assert_eq!(out.steps[0].action, "Read the five newest emails");
        assert!(out.unsupported_steps.is_empty());
    }

    #[test]
    fn the_output_step_and_unknown_steps_cannot_be_removed_and_goal_cannot_be_empty() {
        let base = DraftEdit { goal: "Goal".into(), ..DraftEdit::default() };
        assert!(draft().edited(&DraftEdit { removed_steps: vec!["save".into()], ..base.clone() }).is_err());
        assert!(draft().edited(&DraftEdit { removed_steps: vec!["nope".into()], ..base.clone() }).is_err());
        assert!(draft().edited(&DraftEdit { goal: "  ".into(), ..base.clone() }).is_err());
        assert!(draft().edited(&DraftEdit { step_actions: BTreeMap::from([("read".into(), " ".into())]), ..base }).is_err());
    }
}
