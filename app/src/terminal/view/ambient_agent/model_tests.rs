use url::Url;
use warpui::{App, EntityId};

use super::*;
use crate::ai::blocklist::handoff::HandoffLaunchAttachments;
use crate::ai::llms::LLMPreferences;
use crate::test_util::terminal::initialize_app_for_terminal_view;

fn attachment() -> AttachmentInput {
    AttachmentInput {
        file_name: "context.txt".to_owned(),
        mime_type: "text/plain".to_owned(),
        data: "hello".to_owned(),
    }
}

fn pending_launch() -> PendingCloudLaunch {
    PendingCloudLaunch {
        prompt: "fix tests".to_owned(),
        attachments: HandoffLaunchAttachments {
            request_attachments: vec![attachment()],
            display_attachments: vec![],
        },
    }
}

/// Empty-prompt launch fixture for empty-prompt handoff tests. Mirrors what
/// the workspace synthesizes when the chip / `&` / `/handoff` is dispatched
/// with `launch: None` and `EmptyPromptHandoff` is on.
fn empty_pending_launch() -> PendingCloudLaunch {
    PendingCloudLaunch {
        prompt: String::new(),
        attachments: HandoffLaunchAttachments::default(),
    }
}

fn pending_handoff() -> PendingHandoff {
    PendingHandoff {
        forked_conversation_id: Some("forked-conversation".to_owned()),
        title: None,
        touched_workspace: None,
        snapshot_upload: SnapshotUploadStatus::Pending,
        submission_state: HandoffSubmissionState::Idle,
        auto_submit: Some(pending_launch()),
        source_conversation_in_progress: false,
        submitted_with_empty_prompt: false,
    }
}

fn pending_handoff_fresh_launch() -> PendingHandoff {
    PendingHandoff {
        forked_conversation_id: None,
        title: None,
        touched_workspace: None,
        snapshot_upload: SnapshotUploadStatus::Pending,
        submission_state: HandoffSubmissionState::Idle,
        auto_submit: Some(pending_launch()),
        source_conversation_in_progress: false,
        submitted_with_empty_prompt: false,
    }
}

/// Variant of `pending_handoff` for empty-prompt handoff tests. Lets the caller
/// set the source-conversation state and substitute an empty-prompt launch.
fn pending_handoff_empty(source_in_progress: bool) -> PendingHandoff {
    PendingHandoff {
        forked_conversation_id: Some("forked-conversation".to_owned()),
        title: None,
        touched_workspace: None,
        snapshot_upload: SnapshotUploadStatus::Pending,
        submission_state: HandoffSubmissionState::Idle,
        auto_submit: Some(empty_pending_launch()),
        source_conversation_in_progress: source_in_progress,
        submitted_with_empty_prompt: false,
    }
}

/// Builds a non-empty `TouchedWorkspace` so the snapshot-rehydration indicator
/// branch can fire in `empty_prompt_handoff_indicator`.
fn touched_workspace_with_orphan_file() -> TouchedWorkspace {
    TouchedWorkspace {
        repos: vec![],
        orphan_files: vec![std::path::PathBuf::from("/tmp/handoff-fixture.txt")],
    }
}

fn add_model(app: &mut App) -> warpui::ModelHandle<AmbientAgentViewModel> {
    app.add_model(|ctx| AmbientAgentViewModel::new(EntityId::new(), ctx))
}

fn retry_request(prompt: impl Into<String>) -> SpawnAgentRequest {
    SpawnAgentRequest {
        prompt: Some(prompt.into()),
        mode: crate::server::server_api::ai::UserQueryMode::Normal,
        config: Some(AgentConfigSnapshot {
            environment_id: Some("env-123".to_string()),
            model_id: Some("model-123".to_string()),
            worker_host: Some("worker-123".to_string()),
            computer_use_enabled: Some(false),
            ..Default::default()
        }),
        title: Some("Retry title".to_string()),
        team: Some(true),
        agent_identity_uid: Some("agent-123".to_string()),
        skill: None,
        attachments: vec![attachment()],
        interactive: Some(true),
        parent_run_id: Some("parent-run-123".to_string()),
        runtime_skills: vec!["runtime-skill".to_string()],
        referenced_attachments: vec!["referenced-attachment".to_string()],
        conversation_id: Some("conversation-123".to_string()),
        initial_snapshot_token: Some(
            serde_json::from_str("\"snapshot-token-123\"").expect("snapshot token should parse"),
        ),
        snapshot_disabled: Some(true),
        skip_initial_turn: None,
    }
}

fn test_environment_id() -> ServerId {
    ServerId::from(123)
}

#[test]
fn github_auth_url_for_initial_run_includes_focus_cloud_mode_next() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.status = Status::WaitingForSession {
                progress: AgentProgress::new(),
                kind: SessionStartupKind::InitialRun,
            };
            model.request = Some(retry_request("fix tests"));
            model.handle_needs_github_auth(
                "https://example.com/oauth/connect/github?scheme=warpdev".to_string(),
                "auth required".to_string(),
                ctx,
            );
        });

        model.read(&app, |model, _| {
            let auth_url = model.github_auth_url().expect("auth url should be present");
            assert_eq!(model.github_auth_error_message(), Some("auth required"));
            let parsed = Url::parse(auth_url).expect("auth url should parse");
            let next = parsed
                .query_pairs()
                .find(|(key, _)| key == "next")
                .map(|(_, value)| value.into_owned());
            assert_eq!(
                next,
                Some("warpdev://action/focus_cloud_mode?source=cloud_setup".to_string())
            );
        });
    });
}

#[test]
fn github_auth_completed_retries_stored_initial_run_request() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.status = Status::NeedsGithubAuth {
                progress: AgentProgress::new(),
                error_message: "auth required".to_string(),
                auth_url: "https://example.com/oauth/connect/github".to_string(),
            };
            model.request = Some(retry_request("retry this"));

            model.handle_github_auth_completed(ctx);

            assert!(matches!(
                model.status(),
                Status::WaitingForSession {
                    kind: SessionStartupKind::InitialRun,
                    ..
                }
            ));
            let request = model.request().expect("retry should spawn a request");
            assert_eq!(request.prompt.as_deref(), Some("retry this"));
            assert_eq!(request.attachments.len(), 1);
            assert_eq!(request.interactive, Some(true));
            assert_eq!(request.team, Some(true));
            assert_eq!(request.parent_run_id.as_deref(), Some("parent-run-123"));
            assert_eq!(request.title.as_deref(), Some("Retry title"));
            assert_eq!(request.agent_identity_uid.as_deref(), Some("agent-123"));
            assert_eq!(request.runtime_skills, vec!["runtime-skill"]);
            assert_eq!(
                request.referenced_attachments,
                vec!["referenced-attachment"]
            );
            assert_eq!(request.conversation_id.as_deref(), Some("conversation-123"));
            assert_eq!(
                request
                    .initial_snapshot_token
                    .as_ref()
                    .map(|token| token.as_str()),
                Some("snapshot-token-123")
            );
            assert_eq!(request.snapshot_disabled, Some(true));
            let config = request.config.as_ref().expect("config should be preserved");
            assert_eq!(config.environment_id.as_deref(), Some("env-123"));
            assert_eq!(config.model_id.as_deref(), Some("model-123"));
            assert_eq!(config.worker_host.as_deref(), Some("worker-123"));
            assert_eq!(config.computer_use_enabled, Some(false));
        });
    });
}

#[test]
fn viewed_task_config_preserves_environment_before_cloud_model_load() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);
        let environment_id = test_environment_id();

        model.update(&mut app, |model, ctx| {
            model.apply_viewed_task_config_snapshot(
                Some(&AgentConfigSnapshot {
                    environment_id: Some(environment_id.to_string()),
                    ..Default::default()
                }),
                ctx,
            );
            model.validate_environment_after_initial_load(ctx);
        });

        model.read(&app, |model, _| {
            assert_eq!(
                model.selected_environment_id(),
                Some(&SyncId::ServerId(environment_id))
            );
        });
    });
}

#[test]
fn viewed_task_config_applies_oz_model_override() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);
        let terminal_view_id = model.read(&app, |model, _| model.terminal_view_id);

        model.update(&mut app, |model, ctx| {
            model.apply_viewed_task_config_snapshot(
                Some(&AgentConfigSnapshot {
                    model_id: Some("model-from-run".to_string()),
                    ..Default::default()
                }),
                ctx,
            );
        });

        let override_value = model.read(&app, |_, app| {
            LLMPreferences::as_ref(app)
                .get_base_llm_override(terminal_view_id)
                .expect("viewed run model should be stored as a pane override")
        });
        assert_eq!(override_value, "\"model-from-run\"");
    });
}

#[test]
fn followup_github_auth_does_not_reuse_stored_initial_request() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.status = Status::WaitingForSession {
                progress: AgentProgress::new(),
                kind: SessionStartupKind::Followup,
            };
            model.request = Some(retry_request("do not retry"));
            model.handle_needs_github_auth(
                "https://example.com/oauth/connect/github".to_string(),
                "auth required".to_string(),
                ctx,
            );

            assert!(matches!(model.status(), Status::NeedsGithubAuth { .. }));
            assert!(model.request().is_none());

            model.handle_github_auth_completed(ctx);

            assert!(matches!(model.status(), Status::NeedsGithubAuth { .. }));
        });
    });
}

#[test]
fn queue_handoff_auto_submit_enters_waiting_state_without_consuming_launch() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(Some(pending_handoff()), ctx);
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));

        assert!(queued);
        model.read(&app, |model, _| {
            assert!(matches!(
                model.status(),
                Status::WaitingForSession {
                    kind: SessionStartupKind::InitialRun,
                    ..
                }
            ));
            let request = model.request().expect("request should be populated");
            assert_eq!(request.prompt.as_deref(), Some("fix tests"));
            assert_eq!(
                request.conversation_id.as_deref(),
                Some("forked-conversation")
            );
            assert_eq!(request.attachments.len(), 1);
            assert!(request.initial_snapshot_token.is_none());

            let handoff = model
                .pending_handoff
                .as_ref()
                .expect("handoff should remain");
            assert_eq!(handoff.submission_state, HandoffSubmissionState::Queued);
            assert!(handoff.auto_submit.is_some());
        });

        let queued_again =
            model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(!queued_again);
    });
}

#[test]
fn maybe_auto_submit_handoff_waits_for_workspace_and_snapshot_then_consumes_launch() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(Some(pending_handoff()), ctx);
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());

            model.set_pending_handoff_workspace(TouchedWorkspace::default(), ctx);
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());

            model.set_pending_handoff_snapshot_upload(
                SnapshotUploadStatus::SkippedEmptyWorkspace,
                ctx,
            );
            let launch = model
                .maybe_auto_submit_handoff(ctx)
                .expect("ready handoff should auto-submit");
            assert_eq!(launch.prompt, "fix tests");
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());
        });
    });
}

#[test]
fn fresh_launch_queues_handoff_with_no_conversation_id() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(Some(pending_handoff_fresh_launch()), ctx);
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));

        assert!(queued);
        model.read(&app, |model, _| {
            let request = model.request().expect("request should be populated");
            assert_eq!(request.prompt.as_deref(), Some("fix tests"));
            assert!(request.conversation_id.is_none());
            assert_eq!(request.attachments.len(), 1);
        });
    });
}

#[test]
fn fresh_launch_auto_submits_after_snapshot_settles() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(Some(pending_handoff_fresh_launch()), ctx);
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());

            model.set_pending_handoff_workspace(TouchedWorkspace::default(), ctx);
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());

            model.set_pending_handoff_snapshot_upload(
                SnapshotUploadStatus::SkippedEmptyWorkspace,
                ctx,
            );
            let launch = model
                .maybe_auto_submit_handoff(ctx)
                .expect("ready fresh-launch handoff should auto-submit");
            assert_eq!(launch.prompt, "fix tests");
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());
        });
    });
}

#[test]
fn snapshot_failure_is_treated_as_settled_for_auto_submit() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(Some(pending_handoff()), ctx);
            model.set_pending_handoff_workspace(TouchedWorkspace::default(), ctx);
            model.set_pending_handoff_snapshot_upload(
                SnapshotUploadStatus::Failed("upload failed".to_owned()),
                ctx,
            );

            let launch = model
                .maybe_auto_submit_handoff(ctx)
                .expect("Failed snapshot should be treated as settled");
            assert_eq!(launch.prompt, "fix tests");
            assert!(model.maybe_auto_submit_handoff(ctx).is_none());
        });
    });
}

#[test]
fn empty_prompt_auto_submit_with_in_progress_source_injects_continue_in_the_cloud() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ true)),
                ctx,
            );
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(
            queued,
            "empty-prompt auto-submit should be accepted, not gated"
        );

        model.read(&app, |model, _| {
            let request = model.request().expect("request should be populated");
            assert_eq!(
                request.prompt.as_deref(),
                Some("continue in the cloud"),
                "in-progress source + empty prompt must substitute the wire prompt",
            );
            let handoff = model
                .pending_handoff
                .as_ref()
                .expect("handoff should remain");
            assert!(
                handoff.submitted_with_empty_prompt,
                "submitted_with_empty_prompt must be stamped on the pending handoff",
            );
        });
    });
}

#[test]
fn empty_prompt_auto_submit_with_idle_source_sends_none_on_the_wire() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ false)),
                ctx,
            );
            // Stage a non-empty touched workspace so the indicator could pick
            // `SnapshotRehydrationOnly` once the submit completes — but it
            // must not influence the wire substitution decision.
            model.set_pending_handoff_workspace(touched_workspace_with_orphan_file(), ctx);
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            let request = model.request().expect("request should be populated");
            assert!(
                request.prompt.is_none(),
                "idle source + empty prompt must send prompt: None (got {:?})",
                request.prompt
            );
            let handoff = model
                .pending_handoff
                .as_ref()
                .expect("handoff should remain");
            assert!(handoff.submitted_with_empty_prompt);
        });
    });
}

#[test]
fn empty_prompt_indicator_returns_continue_for_in_progress_source() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _guard = FeatureFlag::EmptyPromptHandoff.override_enabled(true);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ true)),
                ctx,
            );
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            assert_eq!(
                model.empty_prompt_handoff_indicator(),
                Some(EmptyPromptHandoffIndicator::Continue),
            );
        });
    });
}

#[test]
fn empty_prompt_indicator_returns_snapshot_rehydration_for_idle_source_with_content() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _guard = FeatureFlag::EmptyPromptHandoff.override_enabled(true);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ false)),
                ctx,
            );
            model.set_pending_handoff_workspace(touched_workspace_with_orphan_file(), ctx);
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            assert_eq!(
                model.empty_prompt_handoff_indicator(),
                Some(EmptyPromptHandoffIndicator::SnapshotRehydrationOnly),
            );
        });
    });
}

#[test]
fn empty_prompt_indicator_returns_none_for_idle_source_with_empty_snapshot() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _guard = FeatureFlag::EmptyPromptHandoff.override_enabled(true);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ false)),
                ctx,
            );
            model.set_pending_handoff_workspace(TouchedWorkspace::default(), ctx);
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            assert!(
                model.empty_prompt_handoff_indicator().is_none(),
                "idle + empty snapshot must yield no indicator block",
            );
        });
    });
}

#[test]
fn empty_prompt_indicator_returns_none_when_flag_disabled() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _guard = FeatureFlag::EmptyPromptHandoff.override_enabled(false);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ true)),
                ctx,
            );
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            assert!(
                model.empty_prompt_handoff_indicator().is_none(),
                "indicator must short-circuit to None when the feature flag is off",
            );
        });
    });
}

#[test]
fn build_handoff_spawn_request_sets_skip_initial_turn_when_no_content() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ false)),
                ctx,
            );
            model.set_pending_handoff_workspace(TouchedWorkspace::default(), ctx);
            model.set_pending_handoff_snapshot_upload(
                SnapshotUploadStatus::SkippedEmptyWorkspace,
                ctx,
            );
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);
        let launch = model.update(&mut app, |model, ctx| model.maybe_auto_submit_handoff(ctx));
        assert!(
            launch.is_some(),
            "workspace and snapshot have settled; maybe_auto_submit_handoff must consume the launch"
        );

        model.read(&app, |model, _| {
            let request = model.request().expect("request should be populated");
            assert_eq!(
                request.skip_initial_turn,
                Some(true),
                "empty prompt + no substitution + no snapshot token must set skip_initial_turn",
            );
            assert!(
                request.prompt.is_none(),
                "wire prompt must be None (got {:?})",
                request.prompt,
            );
        });
    });
}

#[test]
fn build_handoff_spawn_request_does_not_set_skip_initial_turn_with_continue_substitution() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ true)),
                ctx,
            );
            model.set_pending_handoff_workspace(TouchedWorkspace::default(), ctx);
            model.set_pending_handoff_snapshot_upload(
                SnapshotUploadStatus::SkippedEmptyWorkspace,
                ctx,
            );
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            let request = model.request().expect("request should be populated");
            assert!(
                request.skip_initial_turn.is_none(),
                "`continue in the cloud` substitution must suppress skip_initial_turn (got {:?})",
                request.skip_initial_turn,
            );
            assert_eq!(
                request.prompt.as_deref(),
                Some("continue in the cloud"),
                "in-progress source must substitute the wire prompt",
            );
        });
    });
}

#[test]
fn build_handoff_spawn_request_does_not_set_skip_initial_turn_with_snapshot() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let model = add_model(&mut app);

        let token: InitialSnapshotToken =
            serde_json::from_str("\"snapshot-token-abc\"").expect("snapshot token should parse");

        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(
                Some(pending_handoff_empty(/*source_in_progress*/ false)),
                ctx,
            );

            let request = model.build_handoff_spawn_request(
                None,
                vec![],
                Some("forked-conversation".to_owned()),
                Some(token),
                ctx,
            );

            assert!(
                request.skip_initial_turn.is_none(),
                "snapshot rehydration must suppress skip_initial_turn (got {:?})",
                request.skip_initial_turn,
            );
            assert!(request.prompt.is_none());
            assert!(request.initial_snapshot_token.is_some());
        });
    });
}

#[test]
fn empty_prompt_indicator_returns_none_when_submitted_with_nonempty_prompt() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _guard = FeatureFlag::EmptyPromptHandoff.override_enabled(true);
        let model = add_model(&mut app);

        // Non-empty `pending_handoff` ("fix tests") so `queue_handoff_auto_submit`
        // sets `submitted_with_empty_prompt = false`.
        model.update(&mut app, |model, ctx| {
            model.set_pending_handoff(Some(pending_handoff()), ctx);
        });

        let queued = model.update(&mut app, |model, ctx| model.queue_handoff_auto_submit(ctx));
        assert!(queued);

        model.read(&app, |model, _| {
            assert!(
                model.empty_prompt_handoff_indicator().is_none(),
                "non-empty prompt submit must not surface the empty-prompt indicator",
            );
        });
    });
}
