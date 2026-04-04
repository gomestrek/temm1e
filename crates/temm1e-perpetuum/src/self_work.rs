use std::sync::Arc;

use temm1e_core::types::error::Temm1eError;

use crate::bug_reporter;
use crate::cognitive::LlmCaller;
use crate::conscience::SelfWorkKind;
use crate::log_scanner;
use crate::store::Store;

/// Execute a self-work activity during Sleep state.
pub async fn execute_self_work(
    kind: &SelfWorkKind,
    store: &Arc<Store>,
    caller: Option<&Arc<dyn LlmCaller>>,
) -> Result<String, Temm1eError> {
    match kind {
        SelfWorkKind::MemoryConsolidation => consolidate_memory(store).await,
        SelfWorkKind::SessionCleanup => cleanup_sessions(store).await,
        SelfWorkKind::BlueprintRefinement => refine_blueprints(store).await,
        SelfWorkKind::FailureAnalysis => {
            if let Some(caller) = caller {
                analyze_failures(store, caller).await
            } else {
                Ok("Skipped: no LLM caller available".to_string())
            }
        }
        SelfWorkKind::LogIntrospection => {
            if let Some(caller) = caller {
                introspect_logs(store, caller).await
            } else {
                Ok("Skipped: no LLM caller available".to_string())
            }
        }
        SelfWorkKind::BugReview => {
            if let Some(caller) = caller {
                review_bugs(store, caller).await
            } else {
                Ok("Skipped: no LLM caller available".to_string())
            }
        }
    }
}

/// Memory consolidation: clean up expired volition notes, prune old monitor history.
async fn consolidate_memory(store: &Arc<Store>) -> Result<String, Temm1eError> {
    store.cleanup_expired_notes().await?;
    // Prune monitor history older than 7 days (keep last 100 per concern)
    // For now, expired notes cleanup is the primary consolidation
    tracing::info!(target: "perpetuum", work = "memory_consolidation", "Consolidated memory");
    Ok("Memory consolidated: expired notes cleaned".to_string())
}

/// Session cleanup: no-op for now (placeholder for future session pruning).
async fn cleanup_sessions(_store: &Arc<Store>) -> Result<String, Temm1eError> {
    tracing::info!(target: "perpetuum", work = "session_cleanup", "Session cleanup complete");
    Ok("Session cleanup complete".to_string())
}

/// Blueprint refinement: no-op for now (placeholder for future blueprint weight updates).
async fn refine_blueprints(_store: &Arc<Store>) -> Result<String, Temm1eError> {
    tracing::info!(target: "perpetuum", work = "blueprint_refinement", "Blueprint refinement complete");
    Ok("Blueprint refinement complete".to_string())
}

/// Failure analysis: LLM reviews recent errors from volition notes and transition logs.
async fn analyze_failures(
    store: &Arc<Store>,
    caller: &Arc<dyn LlmCaller>,
) -> Result<String, Temm1eError> {
    let notes = store.get_volition_notes(20).await?;
    if notes.is_empty() {
        return Ok("No recent notes to analyze".to_string());
    }

    let notes_text = notes.join("\n- ");
    let prompt = format!(
        "Review these recent agent activity notes and identify any failure patterns or recurring issues:\n\
         - {notes_text}\n\n\
         Summarize findings in 2-3 sentences. Focus on actionable patterns."
    );

    let analysis = caller.call(None, &prompt).await?;

    // Save the analysis as a volition note for future reference
    store
        .save_volition_note(&format!("Failure analysis: {analysis}"), "self_work")
        .await?;

    tracing::info!(target: "perpetuum", work = "failure_analysis", "Failure analysis complete");
    Ok(format!("Failure analysis: {analysis}"))
}

/// Log introspection: LLM reviews recent interaction patterns.
async fn introspect_logs(
    store: &Arc<Store>,
    caller: &Arc<dyn LlmCaller>,
) -> Result<String, Temm1eError> {
    let notes = store.get_volition_notes(10).await?;
    if notes.is_empty() {
        return Ok("No recent activity to introspect".to_string());
    }

    let notes_text = notes.join("\n- ");
    let prompt = format!(
        "Review these recent agent activity notes and extract any learnings about user preferences or effective strategies:\n\
         - {notes_text}\n\n\
         Summarize in 2-3 sentences. Focus on what worked well."
    );

    let insights = caller.call(None, &prompt).await?;

    store
        .save_volition_note(&format!("Introspection: {insights}"), "self_work")
        .await?;

    tracing::info!(target: "perpetuum", work = "log_introspection", "Log introspection complete");
    Ok(format!("Introspection: {insights}"))
}

/// Load the GitHub PAT from credentials.toml (if configured).
fn load_github_token() -> Option<String> {
    let creds = temm1e_core::config::credentials::load_credentials_file()?;
    let github = creds.providers.iter().find(|p| p.name == "github")?;
    github.keys.first().cloned()
}

/// Check if bug reporting consent has been given.
fn is_consent_given() -> bool {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".temm1e")
        .join("bug_reporter.toml");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .contains("consent_given = true")
}

/// Bug review: scan logs for recurring errors, triage via LLM, report to GitHub.
async fn review_bugs(
    store: &Arc<Store>,
    caller: &Arc<dyn LlmCaller>,
) -> Result<String, Temm1eError> {
    // Check rate limit (max 1 report per 6 hours)
    if let Ok(notes) = store.get_volition_notes(20).await {
        for note in &notes {
            if let Some(ts_str) = note.strip_prefix("bug_review_last:") {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str.trim()) {
                    let elapsed = chrono::Utc::now() - dt.with_timezone(&chrono::Utc);
                    if elapsed < chrono::Duration::hours(6) {
                        return Ok("BugReview: rate limited, skipping".to_string());
                    }
                }
                break;
            }
        }
    }

    // Scan logs
    let log_path = temm1e_observable::file_logger::current_log_path();
    let errors = log_scanner::scan_recent_errors(&log_path, 6, 2);

    if errors.is_empty() {
        return Ok("BugReview: no recurring errors found".to_string());
    }

    // Load GitHub token (if not configured, triage only — no reporting)
    let github_token = load_github_token();
    let can_report = github_token.is_some() && is_consent_given();

    // Triage each error group via LLM
    let system = "You are reviewing error logs from TEMM1E, an AI agent runtime.";
    let mut bugs_found = 0;
    let mut reported = 0;
    let client = reqwest::Client::new();
    let version = env!("CARGO_PKG_VERSION");
    let os_info = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

    for error in &errors {
        let prompt = bug_reporter::build_triage_prompt(error);
        match caller.call(Some(system), &prompt).await {
            Ok(response) => {
                let category = bug_reporter::parse_triage_category(&response);
                if category == "BUG" {
                    bugs_found += 1;
                    tracing::info!(
                        target: "perpetuum",
                        signature = %error.signature,
                        count = error.count,
                        "BugReview: found reportable bug"
                    );

                    // Try to report to GitHub if configured
                    if can_report {
                        if let Some(ref token) = github_token {
                            // Dedup — skip if already reported
                            match bug_reporter::is_duplicate(&client, token, &error.signature).await
                            {
                                Ok(true) => {
                                    tracing::debug!(
                                        target: "perpetuum",
                                        signature = %error.signature,
                                        "BugReview: already reported, skipping"
                                    );
                                }
                                Ok(false) => {
                                    // Scrub and create issue
                                    let body = bug_reporter::format_issue_body(
                                        error, &response, version, &os_info,
                                    );
                                    let scrubbed = temm1e_tools::credential_scrub::scrub_for_report(
                                        &body,
                                        &[],
                                    );
                                    let scrubbed =
                                        temm1e_tools::credential_scrub::entropy_scrub(&scrubbed);

                                    let title = format!(
                                        "[BUG] {}",
                                        &error.message[..error.message.len().min(70)]
                                    );

                                    match bug_reporter::create_issue(
                                        &client, token, &title, &scrubbed,
                                    )
                                    .await
                                    {
                                        Ok(url) => {
                                            reported += 1;
                                            tracing::info!(
                                                target: "perpetuum",
                                                url = %url,
                                                "BugReview: issue created"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                error = %e,
                                                "BugReview: GitHub issue creation failed"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        "BugReview: dedup check failed"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "BugReview: LLM triage failed for one error");
            }
        }
    }

    tracing::info!(
        target: "perpetuum",
        total_errors = errors.len(),
        bugs = bugs_found,
        reported,
        "BugReview complete"
    );

    // Record timestamp to enforce rate limit
    store
        .save_volition_note(
            &format!("bug_review_last:{}", chrono::Utc::now().to_rfc3339()),
            "self_work",
        )
        .await?;

    Ok(format!(
        "BugReview: scanned {} error groups, {} bugs found, {} reported to GitHub",
        errors.len(),
        bugs_found,
        reported
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn consolidation_runs() {
        let store = Arc::new(Store::new("sqlite::memory:").await.unwrap());
        let result = consolidate_memory(&store).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cleanup_sessions_runs() {
        let store = Arc::new(Store::new("sqlite::memory:").await.unwrap());
        let result = cleanup_sessions(&store).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn self_work_no_llm_skips_gracefully() {
        let store = Arc::new(Store::new("sqlite::memory:").await.unwrap());
        let result = execute_self_work(&SelfWorkKind::FailureAnalysis, &store, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Skipped"));
    }
}
