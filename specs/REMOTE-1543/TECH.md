# Queued Prompts UI — Technical Spec
See `specs/REMOTE-1543/PRODUCT.md` for user-visible behavior. This document covers implementation only.
## Context
The new queued prompts panel should be behind a new opt-in feature flag. General prompt queueing should continue to use the new multi-row queue model regardless of the flag; when the flag is off, those prompts may queue without rendering the new management panel. The only old queued-prompt UI behavior we need to preserve is Cloud Mode / cloud-agent setup, where the initial prompt should keep using the old pending user query block when the new UI flag is off.
The implementation should therefore be additive and narrowly scoped:
- Keep the restored legacy pending-query block for Cloud Mode setup when `NewQueuedPromptUI` is off.
- Keep the new multi-row queue model as the source of truth for general queued prompts.
- Keep the new input-adjacent panel as the opt-in rendering / management surface.
- Avoid rewriting the old Cloud Mode pending-block code just to support the new path.
## Feature flag
Add a new runtime feature flag:
- Cargo feature: `new_queued_prompt_ui` in `app/Cargo.toml`.
- Runtime flag: `FeatureFlag::NewQueuedPromptUI` in `crates/warp_features/src/lib.rs`.
- App registration: `#[cfg(feature = "new_queued_prompt_ui")] FeatureFlag::NewQueuedPromptUI` in `app/src/lib.rs`.
- Opt-in only: do not include `FeatureFlag::NewQueuedPromptUI` in `DOGFOOD_FLAGS`, `PREVIEW_FLAGS`, or `RELEASE_FLAGS`.
Do not add `new_queued_prompt_ui` to the default Cargo feature list. Users should not see the new queued prompts panel unless they explicitly opt into this feature, but general prompt queueing should continue to use the new queue model.
## Rollout behavior
The rollout matrix is:
- `QueueSlashCommand` off: queue trigger surfaces that depend on `/queue` remain disabled as they do today.
- `QueueSlashCommand` on and `NewQueuedPromptUI` off: general queued prompts use `QueuedQueryModel`, but the new panel is not constructed/rendered. Cloud Mode setup uses the legacy pending user query block.
- `QueueSlashCommand` on and `NewQueuedPromptUI` on: general queued prompts and Cloud Mode setup use `QueuedQueryModel`, and `QueuedPromptsPanelView` renders queued rows.
`PendingUserQueryIndicator` remains relevant only to the restored legacy Cloud Mode pending block. `NewQueuedPromptUI` is the rollout switch for the new input-adjacent queue panel.
## Legacy Cloud Mode feature-off path
Restore the old code as-is where possible, but use it only for Cloud Mode / cloud-agent setup when `NewQueuedPromptUI` is off:
- `app/src/ai/blocklist/block/pending_user_query_block.rs`
- `app/src/terminal/view/pending_user_query.rs`
- `RichContentMetadata::PendingUserQuery` and `RichContent::is_pending_user_query` in `app/src/terminal/view/rich_content.rs`
- `PendingUserQueryKind`, `pending_user_query_view_id`, and `pending_user_query_kind` in `app/src/terminal/view.rs`
- selected-text plumbing for `PendingUserQueryBlock` in `app/src/terminal/model/blocks/selection.rs` and `TerminalView::pending_user_query_selected_text`
Do not restore the legacy single-slot queued prompt callback as the general prompt queueing implementation. General queued prompts should continue to append to `QueuedQueryModel`; with `NewQueuedPromptUI` off, they simply do not render in the new panel.
Legacy Cloud Mode behavior:
- Cloud Mode initial prompt setup shows the old pending user query block when `NewQueuedPromptUI` is off and `PendingUserQueryIndicator` is enabled.
- The old Cloud Mode block has no dismiss or send-now affordances; the cloud run lifecycle owns removal.
- When the real shared-session transcript content, auth, cancellation, or non-setup-v2 failure path takes over, the old block is removed by the restored legacy removal helper.
- For `CloudModeSetupV2` failures, keep the old block visible above the failure/tombstone state so the user can still see the prompt that was submitted.
## New feature-on path
General queued prompts always use the new multi-row queue implementation. When `FeatureFlag::NewQueuedPromptUI.is_enabled()` is true, also render/manage that queue with the new panel:
- `QueuedQueryModel` in `app/src/ai/blocklist/queued_query.rs`
- `QueuedPromptsPanelView` in `app/src/ai/blocklist/queued_prompts_panel.rs`
- `Input::queued_prompts_panel` in `app/src/terminal/input.rs`
- `TerminalView::drain_queued_prompts` in `app/src/terminal/view.rs`
`QueuedPromptsPanelView::should_render` must require:
- `FeatureFlag::QueueSlashCommand.is_enabled()`
- `FeatureFlag::NewQueuedPromptUI.is_enabled()`
- an active conversation with queued rows
It should not require `PendingUserQueryIndicator`; the new feature flag is the rollout switch for the new UI.
### `QueuedQueryModel`
`QueuedQueryModel` owns general prompt queueing behavior regardless of `NewQueuedPromptUI`:
- `queues: HashMap<AIConversationId, Vec<QueuedQuery>>`
- `editing: Option<EditingRow>`
- `collapsed: HashSet<AIConversationId>`
- `queue_next_prompt_enabled: bool`
`QueuedQueryOrigin::InitialCloudMode` remains non-user-managed. It renders in the new panel when the flag is on, but cannot be edited, deleted, reordered, or auto-fired by the local queue drain.
### Queue trigger routing
General prompt trigger surfaces should not branch to legacy behavior. They should always append to `QueuedQueryModel` with the appropriate origin:
- `Input::maybe_queue_input_for_in_progress_conversation` appends `QueuedQueryOrigin::AutoQueueToggle`.
- `/queue <prompt>` in `app/src/terminal/input/slash_commands/mod.rs` appends `QueuedQueryOrigin::QueueSlashCommand` while the selected conversation is in progress; the idle path still submits immediately.
- `/compact-and <prompt>` in `Workspace::summarize_active_ai_conversation` summarizes immediately, then appends `QueuedQueryOrigin::CompactAnd`.
- `/fork-and-compact <prompt>` in `Workspace::handle_forked_conversation_prompts` summarizes the fork immediately, then appends `QueuedQueryOrigin::ForkAndCompact`.
- `WorkspaceAction::QueuePromptForConversation` appends `QueuedQueryOrigin::AutoQueueToggle`.
`NewQueuedPromptUI` gates only the panel rendering/management surface for those general queued prompts, not whether they use the new queue model.
## Cloud Mode setup
Cloud Mode setup is the most important compatibility requirement.
When `NewQueuedPromptUI` is off:
- Keep using the old `insert_cloud_mode_queued_user_query_block(prompt, ctx)` path in `app/src/terminal/view/pending_user_query.rs`.
- Do not render the new input-adjacent queue panel for Cloud Mode setup.
- Remove the old Cloud Mode pending block with the legacy removal function when the run lifecycle hands off to real transcript content, auth, cancellation, or a non-setup-v2 failure.
- Keep the old Cloud Mode pending block across `CloudModeSetupV2` `Failed` events so the prompt remains visible above the failure/tombstone state.
When `NewQueuedPromptUI` is on:
- Use `QueuedQueryModel` with `QueuedQueryOrigin::InitialCloudMode`.
- Store the returned `QueuedQueryId` on `AmbientAgentViewModel::cloud_mode_queued_query_id`.
- Remove that row on the same lifecycle handoff events that removed the legacy block.
- Keep the row across `CloudModeSetupV2` `Failed` events so the prompt remains visible above the failure/tombstone state.
Cloud Mode lifecycle handlers in `app/src/terminal/view/ambient_agent/view_impl.rs` should branch once and call the appropriate removal/insertion helper for the active feature path. The old helper should remain old-code-shaped; the new helper should remain queue-model-shaped.
## Terminal view wiring
`TerminalView::new` should only construct and attach `QueuedPromptsPanelView` when `FeatureFlag::NewQueuedPromptUI.is_enabled()` is true. When the flag is off, `Input::queued_prompts_panel` stays `None`; general queued prompts still live in `QueuedQueryModel`, and Cloud Mode setup uses the old rich-content block.
`TerminalView::handle_ai_controller_event` should always call `drain_queued_prompts(conversation_id, finish_reason, ctx)` for general queued prompts. That drain is model-owned behavior, not panel-owned behavior.
When an active AI block is detected for a different conversation, keep the restored legacy guard only for the Cloud Mode pending block path so stale Cloud Mode placeholder UI is cleared correctly. General queued prompts should rely on `QueuedQueryModel` conversation scoping.
## Rich content and selection
Because the old UI returns for Cloud Mode setup when `NewQueuedPromptUI` is off, restore the rich-content metadata and selection support:
- `RichContentMetadata::PendingUserQuery { pending_user_query_block_handle }`
- `RichContent::is_pending_user_query`
- `read_selected_text_from_pending_user_query_block`
- `TerminalView::pending_user_query_selected_text`
This code should be used by the legacy path only, but it can remain compiled unconditionally to minimize churn and keep the restored code close to the old implementation.
## Telemetry
New panel-specific telemetry should be emitted only from `QueuedPromptsPanelView`, which only exists/renders when `NewQueuedPromptUI` is enabled:
- `QueuedPrompt.Edited`
- `QueuedPrompt.Deleted`
- `QueuedPrompt.Reordered`
- `QueuedPrompt.PanelCollapseToggled`
The enablement state for those telemetry events should be `FeatureFlag::NewQueuedPromptUI`, not `QueueSlashCommand`, because these events describe the new panel UI rather than queue trigger availability.
General queueing should keep existing telemetry behavior from slash-command acceptance and prompt submission paths. Do not add new telemetry to the restored Cloud Mode legacy block.
## Tests
Update tests to cover both rollout paths.
Feature-off tests:
- `/queue` or the queue workspace action appends to `QueuedQueryModel` but does not construct/render `QueuedPromptsPanelView`.
- Cloud Mode `DispatchedAgent` inserts the old pending user query block when `NewQueuedPromptUI` is off.
- Cloud Mode lifecycle removal removes the old block when the transcript/harness handoff arrives.
Feature-on tests:
- Existing `QueuedQueryModel` and `QueuedPromptsPanelView` tests should set `FeatureFlag::NewQueuedPromptUI` where rendering or panel construction depends on it.
- Cloud Mode `DispatchedAgent` appends an `InitialCloudMode` row and records `cloud_mode_queued_query_id`.
- `drain_queued_prompts` runs model drain behavior regardless of panel visibility.
Regression checks:
- With `NewQueuedPromptUI` off, the new panel is not constructed or rendered.
- With `NewQueuedPromptUI` off, Cloud Mode setup displays the old pending user query block and never displays the new queued prompt panel.
- With `NewQueuedPromptUI` on, legacy `pending_user_query_view_id` remains unused for new queue rows.
## Validation
Run:
- `cargo fmt`
- A targeted compile/test pass for the touched client code, preferably the queued prompt and terminal view tests.
- Full presubmit before PR submission.
Do not run the app as part of this change.
## Risks and mitigations
- **Accidentally rewriting legacy Cloud Mode behavior**: restore the old files and keep the feature-off Cloud Mode path calling the old helpers. Do not reuse the old callback path for general prompt queueing.
- **Two sources of queue UI truth**: `QueuedQueryModel` is the source of truth for general queued prompts regardless of panel visibility. The legacy pending block is only a Cloud Mode setup placeholder when `NewQueuedPromptUI` is off.
- **Cloud Mode setup regression**: explicitly branch Cloud Mode insertion/removal helpers on `NewQueuedPromptUI` and add tests for both paths.
- **Telemetry misattribution**: gate panel telemetry on `NewQueuedPromptUI` so the new UI metrics do not fire for legacy users.
