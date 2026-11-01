//! Local privacy controls share admission with tool publication and build creation.
use super::*;
use std::sync::atomic::Ordering;

impl AppCore {
    pub(crate) fn ensure_active(&self) -> CoreResult<()> {
        if self.inner.deleting.load(Ordering::SeqCst) {
            return Err(CoreError::Invalid("Local data deletion is in progress. Retry deletion or restart Macroscope to finish cleanup.".into()));
        }
        Ok(())
    }

    pub(crate) fn admit_local(&self) -> CoreResult<std::sync::MutexGuard<'_, ()>> {
        let guard = lock(&self.inner.lifecycle);
        self.ensure_active()?;
        Ok(guard)
    }

    pub fn export_tool(&self, tool_id: &str, dir_handle: &str) -> CoreResult<String> {
        let _admission = self.admit_local()?;
        let tool = self.get_tool(tool_id)?;
        let row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        let version = self
            .db()
            .version(tool_id, &tool.version_hash)?
            .ok_or(CoreError::NotFound("version"))?;
        let (root, _) = self.handle_path(dir_handle, PickKind::Dir)?;
        let name: String = tool
            .summary_row
            .name
            .chars()
            .map(|c| {
                if c.is_control() || ['/', '\\', ':'].contains(&c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let name = format!("{}-v{}", name.trim(), tool.summary_row.version);
        // This structural export deliberately omits selected previews, paths, receipts,
        // draft metadata and samples. The exported contract contains column names only.
        let contract =
            json!({"inputColumns": tool.input_columns, "outputColumns": tool.output_columns});
        let rules = tool
            .mapping
            .output_columns
            .iter()
            .map(|c| format!("{} → {}", c.source, c.name))
            .collect::<Vec<_>>()
            .join("\n");
        let readme = if tool.program.is_some() {
            format!("{}\n\nProgram (sandboxed). Runs through Macroscope runtime v2. Host checks cover selected input, deterministic bytes, output format, example lines and source integrity. The host previews and commits one create-only output after approval.\n", row.name)
        } else {
            format!("{}\n\nWhat it does:\n{}\n\nChecked by Macroscope's host checks: confirmed mapping rules, selected CSV, row counts, source integrity and malformed-input stop cases.\nRuns in Macroscope with the csv-mapping engine; this folder is not a standalone application.\nNo user data or sample rows are included. contract.json contains column names only.\n", row.name, rules)
        };
        let files = vec![
            (
                "manifest.json",
                serde_json::to_vec_pretty(&version.manifest),
            ),
            ("mapping.json", serde_json::to_vec_pretty(&tool.mapping)),
            ("contract.json", serde_json::to_vec_pretty(&contract)),
        ]
        .into_iter()
        .map(|(n, b)| b.map(|b| (n, b)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Internal(e.to_string()))?;
        let mut files = files;
        if let Some(program) = &tool.program {
            files.push((
                "program.json",
                serde_json::to_vec_pretty(program)
                    .map_err(|e| CoreError::Internal(e.to_string()))?,
            ));
        }
        files.push(("README.txt", readme.into_bytes()));
        #[cfg(unix)]
        let path = broker::export_bundle(&root, &name, &files).map_err(|_| CoreError::Invalid("Export refused: choose a folder without links and a destination that does not already exist.".into()))?;
        #[cfg(not(unix))]
        let path: PathBuf = return Err(CoreError::Invalid(
            "Tool export is unavailable on this platform.".into(),
        ));
        self.activity("settings", format!("Exported tool “{}”", row.name), None);
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn revoke_tool(&self, tool_id: &str) -> CoreResult<ToolDetail> {
        let _admission = self.admit_local()?;
        let mut row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        self.db().revoke_grants(tool_id, self.now())?;
        row.state = "paused".into();
        self.db().put_tool(&row)?;
        self.activity("settings", format!("Paused tool “{}”", row.name), None);
        self.tool_detail(&row)
    }

    /// Renames a saved tool. The name is display text only: versions, permissions and schedules are unchanged.
    pub fn rename_tool(&self, tool_id: &str, name: &str) -> CoreResult<ToolDetail> {
        let _admission = self.admit_local()?;
        let name: String = name.chars().filter(|c| !c.is_control()).collect();
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(CoreError::Invalid("Give the tool a name of up to 80 characters.".into()));
        }
        let mut row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        let old = std::mem::replace(&mut row.name, name.to_string());
        self.db().put_tool(&row)?;
        self.activity("settings", format!("Renamed tool “{old}” to “{name}”"), None);
        self.tool_detail(&row)
    }

    pub fn restore_tool(&self, tool_id: &str) -> CoreResult<ToolDetail> {
        let _admission = self.admit_local()?;
        let mut row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        if row.state == "paused" {
            let hash = row
                .active_version
                .as_deref()
                .ok_or(CoreError::NotFound("version"))?;
            let revision = self.db().next_grant_revision(tool_id)?;
            self.db().put_grant(
                &new_id("grant"),
                tool_id,
                hash,
                revision,
                &json!({"read": "selected_input", "create": "one_output_in_selected_root"}),
            )?;
            row.state = "enabled".into();
            self.db().put_tool(&row)?;
            self.activity("settings", format!("Resumed tool “{}”", row.name), None);
        }
        self.tool_detail(&row)
    }

    pub fn delete_preview(&self) -> CoreResult<DeletePreview> {
        self.db().purge_expired_outbound(self.now())?;
        let stats = self.db().stats()?;
        let mut items: Vec<DeleteItem> = [
            ("Drafts", "drafts"),
            ("Builds", "builds"),
            ("Build usage legs", "model_usage"),
            ("Outbound records and payloads", "outbound"),
            ("Connection detections", "detections"),
            ("Suggestion dismissals", "suggestion_dismissals"),
            ("Activity", "activity"),
            ("Today entries", "today"),
            ("Schedules", "schedules"),
        ]
        .into_iter()
        .map(|(label, table)| DeleteItem {
            label: label.into(),
            count: stats
                .iter()
                .find(|s| s.table == table)
                .map_or(0, |s| s.rows),
        })
        .collect();
        for (label, dir) in [
            ("Staging directories", "runs"),
            ("Sample copies", "samples"),
            ("Build workspaces", "builds"),
        ] {
            let count = std::fs::read_dir(self.inner.cfg.data_dir.join(dir))
                .map(|d| d.count() as i64)
                .unwrap_or(0);
            items.push(DeleteItem {
                label: label.into(),
                count,
            });
        }
        Ok(DeletePreview {
            items,
            tools_included: false,
        })
    }

    pub async fn delete_data(&self, options: DeleteOptions) -> CoreResult<()> {
        // Close local admission and wait for any publication/mutation already in progress.
        {
            let _admission = lock(&self.inner.lifecycle);
            self.inner.deleting.store(true, Ordering::SeqCst);
        }
        self.discard_all_recordings();
        let _cloud = self.inner.cloud_controls.lock().await;
        self.db().set_setting("local_only", &json!(true))?;
        self.stop_cloud_work().await?;
        for slot in lock(&self.inner.builds).values() {
            let _ = slot.cancel.send(true);
        }
        // Keep handles registered across awaits: canceling this deletion request must
        // not detach workers and let a retry delete rows while they can still write.
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if lock(&self.inner.builds)
                    .values()
                    .all(|s| s.task.as_ref().is_none_or(|t| t.is_finished()))
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| {
            CoreError::Invalid("Build work is still stopping. Retry deletion shortly.".into())
        })?;
        let _admission = lock(&self.inner.lifecycle);
        self.db().begin_deletion(options.include_tools)?;
        self.resume_deletion()?;
        self.inner.deleting.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn resume_deletion(&self) -> CoreResult<()> {
        let Some(include_tools) = self.db().pending_deletion()? else {
            return Ok(());
        };
        // On restart this runs while policy is closed, before recovering or dispatching jobs.
        self.db().set_setting("local_only", &json!(true))?;
        lock(&self.inner.staged).clear();
        lock(&self.inner.builds).clear();
        lock(&self.inner.handles).clear();
        self.db().delete_local_rows(include_tools)?;
        let mut dirs = vec![
            "runs",
            "samples",
            "builds",
            "muse",
            "model-steps",
            "access-research",
        ];
        if include_tools {
            dirs.push("tools");
            dirs.push("bots");
        }
        for dir in dirs {
            let path = self.inner.cfg.data_dir.join(dir);
            make_writable(&path);
            match std::fs::symlink_metadata(&path) {
                Ok(m) if !m.is_dir() => std::fs::remove_file(&path),
                Ok(_) => std::fs::remove_dir_all(&path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
            .map_err(|e| CoreError::Storage(format!("Local cleanup is unfinished: {e}")))?;
            std::fs::create_dir_all(&path).map_err(|e| CoreError::Storage(e.to_string()))?;
        }
        std::fs::create_dir_all(self.inner.cfg.data_dir.join("muse/home"))
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        self.db().finish_deletion()?;
        Ok(())
    }

    pub fn build_try_input(&self, build_id: &str, input: &str) -> CoreResult<BuildState> {
        let _admission = self.admit_local()?;
        let mut row = self
            .db()
            .build(build_id)?
            .ok_or(CoreError::NotFound("build"))?;
        let mut state = self.build_state(build_id)?;
        if state.phase != BuildPhase::Ready {
            return Err(CoreError::Invalid(
                "Only a ready candidate can check another sample.".into(),
            ));
        }
        let draft = self.load_draft(&state.draft_id)?;
        if row.state.get("draftRevision").and_then(Value::as_u64) != Some(draft.revision as u64) {
            return Err(CoreError::Invalid(
                "The task changed after this build. Build it again.".into(),
            ));
        }
        if draft.no_input {
            return Err(CoreError::Invalid(
                "This build has no input file, so there is no sample to try.".into(),
            ));
        }
        let contract = drafts::contract_for(&draft);
        let mapping = state
            .candidate
            .as_ref()
            .ok_or(CoreError::NotFound("candidate"))?;
        let (path, _) = self.handle_path(input, PickKind::File)?;
        let read = || Self::read_limited(&path).map_err(|e| e.to_string());
        let outcome = if let Some(program) = &state.program {
            if !program.web_sources.is_empty() {
                return Err(CoreError::Invalid(
                    "Trying another sample is unavailable for builds that fetch web sources."
                        .into(),
                ));
            }
            #[cfg(feature = "generated-runtime")]
            {
                let engine = self.runtime()?;
                checks::run_program(&draft, program, &read, &|input, code| {
                    engine
                        .execute_plan(input, code, macroscope_runner::Limits::default())
                        .map_err(|e| e.to_string())
                })
            }
            #[cfg(not(feature = "generated-runtime"))]
            {
                let _ = program;
                checks::program_failure("Generated runtime is disabled.".into())
            }
        } else {
            checks::run(&contract, mapping, &read)
        };
        let passed = outcome.all_passed();
        state.phase = if passed {
            BuildPhase::Ready
        } else {
            BuildPhase::Failed
        };
        state.failure = if passed {
            None
        } else {
            Some(BuildFailure {
                kind: BuildFailureKind::Checks,
                message: outcome
                    .checks
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.detail.as_deref().unwrap_or(&c.label))
                    .collect::<Vec<_>>()
                    .join("; "),
            })
        };
        state.checks = outcome.checks;
        state.comparison = outcome.comparison;
        state.events.push(BuildEvent {
            build_id: build_id.into(),
            seq: state.events.len() as u32 + 1,
            phase: state.phase,
            message: if passed {
                "New sample checks passed".into()
            } else {
                "New sample checks failed".into()
            },
            at_ms: self.now(),
        });
        let revision = row.state["draftRevision"].clone();
        row.state = json!(state);
        row.state["draftRevision"] = revision;
        row.phase = state.phase.as_str().into();
        row.updated_ms = self.now();
        self.db().put_build(&row)?;
        if let Some(slot) = lock(&self.inner.builds).get_mut(build_id) {
            slot.state = state.clone();
            let _ = slot.tx.send(state.clone());
        }
        Ok(state)
    }
}
