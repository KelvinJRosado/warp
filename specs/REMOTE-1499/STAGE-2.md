# Empty-Prompt Local-to-Cloud Handoff — Stage 2 Sub-Tech-Spec (warp-4)
Sub-tech-spec for what **Stage 2** of REMOTE-1499 delivers on the warp-4 side. The full end-to-end architecture lives in `TECH.md`; the full product behavior lives in `PRODUCT.md`. This document covers only the changes that ship on `harry/empty-prompt-handoff-local`.
Branch: `harry/empty-prompt-handoff-local`, stacked on Stage 1's `harry/empty-prompt-handoff-wire-contract`.
Sibling specs (cross-repo):
- `../../../warp-server-4/specs/REMOTE-1499/STAGE-2.md` — Stage 2b server-side derivation + Stage 2c protocol-rev bump.
- `../../../oz-agent-worker/specs/REMOTE-1499/STAGE-2.md` — Stage 2b self-hosted worker side.
- `../../../session-sharing-protocol/specs/REMOTE-1499/STAGE-2.md` — Stage 2c protocol variant.
- `../../../session-sharing-server/specs/REMOTE-1499/STAGE-2.md` — Stage 2c testing-only dep swap.
## Scope
Stage 2 packs four sub-stages onto a single warp-4 branch:
- **2a** — Client-side empty-prompt handoff behavior: feature flag, three entry-point dispatches, `continue in the cloud` wire substitution, `empty_prompt_handoff_indicator` enum + view_impl rendering, telemetry extensions.
- **2b** — Worker-derived skip-initial-turn cutover on the client side: remove the stored `skip_initial_turn` plumbing on `SpawnAgentRequest`, on `AgentRunPrompt::ServerSide`, and in `build_server_side_task`; rewire the `AgentDriver` skip branch to read the bool from a new field populated from the `--skip-initial-turn` CLI flag. The CLI flag itself is kept as the worker→CLI contract.
- **2c** — Cloud Mode Setup V2 wedge fix: `TerminalModel::send_ambient_setup_phase_ended_for_shared_session` helper, `AgentDriver::execute_run` restructured around `IdleTimeoutSender::complete_with_optional_idle`, viewer `event_loop.rs` arm for `OrderedTerminalEventType::AmbientSetupPhaseEnded`, testing-only `session-sharing-protocol` local-path swap in root `Cargo.toml`.
- **2d** — Canonicalize the setup-complete signal onto `AmbientSetupPhaseEnded` for the non-skip `AgentRunPrompt::ServerSide` arm as well, and add inline comments on the two legacy `AppendedExchange`-driven teardowns marking them as transition-compat fallbacks.
2b spans warp-server-4 + oz-agent-worker (see their cross-repo sibling specs). 2c spans session-sharing-protocol + session-sharing-server (also cross-repo).
## Stage 2a — Client-side empty-prompt handoff behavior
### Feature flag
`crates/warp_features/src/lib.rs` — add `FeatureFlag::EmptyPromptHandoff`. Default off. Not added to `DOGFOOD_FLAGS` initially.
### Three entry points
All three converge in `start_local_to_cloud_handoff` (`app/src/workspace/view.rs:13652-13663`), which synthesizes an empty `PendingCloudLaunch { prompt: "".to_owned(), attachments: vec![] }` when `EmptyPromptHandoff` is on. There is **no separate handoff compose pane** for any of the three entry points — all three result in the same immediate-handoff dispatch.
- `app/src/ai/blocklist/agent_view/agent_input_footer/mod.rs:2547-2574` `OpenHandoffPane` action: when `EmptyPromptHandoff` is on, dispatches `WorkspaceAction::OpenLocalToCloudHandoffPane { launch: None, environment_id: None, entry_point: HandoffEntryPoint::FooterChip }` directly. The chip skips `&` compose mode.
- `app/src/terminal/input.rs:4060-4067`: removes the empty-prompt early-return in `maybe_launch_cloud_handoff_request`; always builds `PendingCloudLaunch { prompt: "".to_owned(), attachments }` when `EmptyPromptHandoff` is on. Entry point: `HandoffEntryPoint::Ampersand`.
- `app/src/terminal/input/slash_commands/mod.rs:924-940` `/handoff` with no argument: dispatches the same `OpenLocalToCloudHandoffPane { launch: None, ... }` as the chip when `EmptyPromptHandoff` is on. Entry point: `HandoffEntryPoint::SlashCommand`. The central launch synthesis in `start_local_to_cloud_handoff` covers FooterChip / Ampersand / SlashCommand uniformly.
### Wire-level substitution
`app/src/terminal/view/ambient_agent/model.rs:686-743` `build_handoff_spawn_request`:
- When the submitted prompt is empty AND `pending_handoff.source_conversation_in_progress` is true, substitute `prompt: Some("continue in the cloud".to_owned())` on the wire.
- Otherwise (idle source, regardless of snapshot) send `prompt: None`.
`source_conversation_in_progress` is captured once at handoff initiation by reading `BlocklistAIHistoryModel::active_conversation(...).status()` and stored on the `PendingHandoff` struct so it can't drift mid-flow.
### `empty_prompt_handoff_indicator` enum and rendering
`app/src/terminal/view/ambient_agent/model.rs:565-601` introduces `EmptyPromptHandoffIndicator { Continue, SnapshotRehydrationOnly, None }` and computes it from `(source_conversation_in_progress, has_snapshot)`:
- In-progress → `Continue` (label `"Continuing previous task in the cloud"`).
- Idle + non-empty snapshot → `SnapshotRehydrationOnly` (label `"Applying workspace changes…"`).
- Idle + empty snapshot → `None` (no empty-prompt indicator block; the standard setup indicator covers the warmup phase on its own).
`app/src/terminal/view/ambient_agent/view_impl.rs:154-189` consumes the indicator and inserts a labeled queued-prompt block in place of the literal-prompt block. The label string is **decoupled** from the wire-substitution string; the two are independently tunable for design iteration.
### Telemetry extensions
`app/src/ai/ambient_agents/telemetry.rs`:
- `CloudAgentTelemetryEvent::HandoffInitiated` gains `empty_prompt: bool` and `injection_path: HandoffInjectionPath { None | Continue | SnapshotRehydrationOnly }`.
- New `CloudAgentTelemetryEvent::HandoffSnapshotPrepared { had_snapshot: bool }` fires after `derive_touched_workspace` settles. Analytics can join this against `HandoffInitiated.injection_path` to learn whether `SnapshotRehydrationOnly` paths actually carried snapshot content.
### Stage 2a tests
Behavioral tests in `app/src/terminal/view/ambient_agent/model_tests.rs` cover empty-prompt auto-submit, indicator variants, and feature-flag gating.
## Stage 2b — Worker-derived skip-initial-turn (client-side)
### SpawnAgentRequest field removal
`app/src/server/server_api/ai.rs:249-260` — remove the `skip_initial_turn: Option<bool>` field. The wire shape no longer carries any client-side derivation of the bool.
### Client-side derivation removal
`app/src/terminal/view/ambient_agent/model.rs:742-747,765` — remove the local `let skip_initial_turn = (wire_prompt.is_none() && initial_snapshot_token.is_none()).then_some(true);` block and the `skip_initial_turn` field assignment in the constructed `SpawnAgentRequest`. The docstring on `build_handoff_spawn_request` is updated to note that the worker derives `--skip-initial-turn` for the sandboxed CLI from the execution input at dispatch time. The `spawn_agent` path at `model.rs:1264` drops its `skip_initial_turn: None` field as well.
### `AgentRunPrompt::ServerSide` field removal
`app/src/ai/agent_sdk/driver.rs:405-410` — remove the `skip_initial_turn: bool` field from the `ServerSide` variant. The variant now only carries `skill: Option<ParsedSkill>` and `attachments_dir: Option<String>`. Two destructure sites in `prepare_harness` and the non-skip branch of `execute_run` drop their `skip_initial_turn: _,` ignores.
### `build_server_side_task` cleanup
`app/src/ai/agent_sdk/mod.rs:490` — remove the `skip_initial_turn: args.skip_initial_turn` field assignment when constructing `AgentRunPrompt::ServerSide`. The stale comment at `mod.rs:1216` (`task.prompt.skip_initial_turn is preserved across this update.`) is removed.
### `AgentDriverOptions` + `AgentDriver` field
`app/src/ai/agent_sdk/driver.rs` — add `skip_initial_turn: bool` to both `AgentDriverOptions` (destructured from `RunAgentArgs::skip_initial_turn` by the `build_driver_options_and_task` closure in `mod.rs:855`) and the `AgentDriver` struct itself (populated from the option in `AgentDriver::new`). The `new_for_test` constructor at `driver.rs:711` initializes the field to `false`.
### Skip-branch rewire in `execute_run`
`app/src/ai/agent_sdk/driver.rs:2410-2411` — replace the destructure-from-prompt gate
```rust path=null start=null
let is_skip_initial_turn = matches!(
    &task_prompt,
    AgentRunPrompt::ServerSide {
        skip_initial_turn: true,
        ..
    },
);
```
with a self-field read gated on the prompt being `ServerSide`:
```rust path=null start=null
let is_skip_initial_turn =
    self.skip_initial_turn && matches!(&task_prompt, AgentRunPrompt::ServerSide { .. });
```
The skip-branch body (enter agent view, emit `AmbientSetupPhaseEnded`, schedule deferred `Success` via `IdleTimeoutSender::complete_with_optional_idle`) is unchanged. The non-skip `ServerSide` arm drops its `skip_initial_turn: _,` from the destructure pattern.
### CLI flag (kept)
`crates/warp_cli/src/agent.rs:368-372` — unchanged. The `--skip-initial-turn` flag is still the worker→CLI contract. The CLI parser test at `crates/warp_cli/src/lib_tests.rs:233-252` (`agent_run_accepts_skip_initial_turn_with_task_id`) is kept as-is.
### Stage 2b test removals
`app/src/terminal/view/ambient_agent/model_tests.rs:614-725` — the three tests `build_handoff_spawn_request_sets_skip_initial_turn_when_no_content`, `build_handoff_spawn_request_does_not_set_skip_initial_turn_with_continue_substitution`, and `build_handoff_spawn_request_does_not_set_skip_initial_turn_with_snapshot` are deleted (they asserted on a now-removed client-side derivation). The `retry_request` fixture at `model_tests.rs:91-117` drops its `skip_initial_turn: None` field. Constructors in `app/src/server/server_api/ai_tests.rs:37-89`, `app/src/ai/ambient_agents/spawn_tests.rs` (five sites), `app/src/ai/agent_sdk/mcp_config_tests.rs:262-292`, `app/src/ai/agent_sdk/ambient.rs:481-502`, `app/src/pane_group/pane/terminal_pane.rs:2136-2161`, and `app/src/terminal/view_tests.rs:1322-1340` drop the field similarly.
## Stage 2c — Cloud Mode Setup V2 wedge fix
With the Stage 2b skip path active, the cloud pane came up under Cloud Mode Setup V2, the environment setup commands ran inside the shared session and were visible to the user, but the pane stayed stuck in "setting up…" because no first `AppendedExchange` event fired to trigger the teardown. Two cooperating root causes — (a) the session-sharing-server was typed-decoding `OrderedTerminalEventType` at an older protocol rev that didn't include `AmbientSetupPhaseEnded`, silently dropping the marker, and (b) the AgentDriver skip path ignored `idle_on_complete`, sending `Success` directly to the oneshot — required fixes in three repos.
### Testing-only Cargo.toml swap
`Cargo.toml:248` — swap the `session-sharing-protocol` dep to `path = "../session-sharing-protocol"` so the locally-running stack pulls the new variant. Must be reverted to `git = ..., rev = <merged SHA>` after the protocol PR merges. Listed in PRODUCT.md "Deferred follow-ups".
### `TerminalModel` helper
`app/src/terminal/model/terminal_model.rs` — add `send_ambient_setup_phase_ended_for_shared_session`, modeled on the adjacent `send_agent_conversation_replay_started_for_shared_session`. The helper is a no-op for non-sharer terminals; sharer terminals emit a typed `OrderedTerminalEventType::AmbientSetupPhaseEnded` event through the existing shared-session event channel.
### `AgentDriver::execute_run` restructure
`app/src/ai/agent_sdk/driver.rs` — `execute_run` is restructured so the skip branch builds `IdleTimeoutSender` first, then runs the skip block (`enter_agent_view` + emit marker via `send_ambient_setup_phase_ended_for_shared_session` + `complete_with_optional_idle`) before the history subscription, then sets up the subscription, then conditionally dispatches the non-skip prompt. Scheduling the timer before the subscription means a later `AppendedExchange` from a session-sharing-protocol follow-up correctly invalidates the timer via `IdleTimeoutSender`'s internal generation counter.
### `IdleTimeoutSender::complete_with_optional_idle`
New helper: `IdleTimeoutSender::complete_with_optional_idle(idle_on_complete, value)`. Defers via `end_run_after` when `Some(d)`; falls back to `end_run_now` when `None`. Existing `UpdatedConversationStatus` and harness-exit branches in `execute_run` are refactored to use the same helper for consistency.
### Viewer `event_loop.rs` arm
`app/src/terminal/shared_session/viewer/event_loop.rs` — adds an `AmbientSetupPhaseEnded` arm. Flips `BlockList::set_is_executing_oz_environment_startup_commands(false)`, then calls `AmbientAgentViewModel::tear_down_active_setup_command_group` which runs `finish_setup_command_group` + `set_setup_command_group_visibility(false)`. The arm parallels the `AppendedExchange`-driven teardown at `app/src/terminal/view.rs:5496-5507` and the chip teardown at `app/src/terminal/view/ambient_agent/block/setup_command_text.rs:119-136`. "No active group" is treated as a no-op for idempotency.
### Stage 2c tests
Viewer-side tests in `app/src/terminal/shared_session/viewer/event_loop_tests.rs` cover teardown + idempotency. Sandbox-side unit tests cover `TerminalModel::send_ambient_setup_phase_ended_for_shared_session` (sharer-emits + non-sharer-no-op). Direct `IdleTimeoutSender::complete_with_optional_idle` tests in `app/src/ai/agent_sdk/driver_tests.rs` cover `None` immediate, `Some(d)` deferred, and `Some(d)` + cross-path `cancel_idle_timeout()` invalidation.
## Stage 2d — Canonicalize setup-complete signal onto `AmbientSetupPhaseEnded`
Stage 2c lands the new marker only for the skip-initial-turn path. Stage 2d extends the emission to the non-skip path so every cloud agent run signals "setup phase complete" via the same canonical marker. The legacy `AppendedExchange`-driven teardowns stay in place as a transition-compat fallback for old sharers that don't emit the marker; removing them is tracked in PRODUCT.md "Deferred follow-ups". Both paths are idempotent on the viewer side, so a new-sharer/new-viewer pair triggering both is harmless.
### Non-skip ServerSide marker emission
`app/src/ai/agent_sdk/driver.rs:2760-2792` `AgentDriver::execute_run` non-skip `AgentRunPrompt::ServerSide` arm: after the existing `terminal.enter_agent_view(None, restored_conversation_id, AgentViewEntryOrigin::Cli, ctx)` call and before the `terminal.ai_controller().update(...)` block that fires `AIAgentInput::StartFromAmbientRunPrompt`, invoke `terminal.model.lock().send_ambient_setup_phase_ended_for_shared_session()`. Mirrors the skip-path emission at `driver.rs:2418-2425`. Do NOT touch the `AgentRunPrompt::Local` arm — local runs don't have a setup phase and the helper is internally guarded by `is_sharer()` anyway, but skipping the call keeps the surface narrow. Inline comment notes the legacy `AppendedExchange`-driven teardowns are kept as a transition fallback.
### Legacy fallback comments
- `app/src/terminal/view.rs:5496-5507`: add a comment to the `BlocklistAIHistoryEvent::AppendedExchange`-driven `set_is_executing_oz_environment_startup_commands(false)` block noting that this is a legacy fallback teardown; the canonical signal is `AmbientSetupPhaseEnded` handled in `event_loop.rs`. Both paths are idempotent.
- `app/src/terminal/view/ambient_agent/block/setup_command_text.rs:119-136`: add the same fallback comment on the `BlocklistAIHistoryEvent::AppendedExchange` subscription on `CloudModeSetupTextBlock`. Same idempotency + compat rationale.
- `app/src/terminal/shared_session/viewer/event_loop.rs:348-376` `OrderedTerminalEventType::AmbientSetupPhaseEnded` arm: tighten the existing comment to make explicit that this arm is the canonical teardown signal and now handles both the skip-initial-turn path AND the normal cloud agent path. No code changes — the arm is already path-agnostic.
### Stage 2d tests
Rely on existing Stage 2c viewer-side tests (`event_loop_tests.rs`) for arm behavior since no logic changes there. No new tests needed for the comment-only changes. The driver-side emission is exercised end-to-end by the standard cloud-mode handoff smoke test that's part of Stage 2c's validation.
## Validation
- `cargo fmt --all --check`.
- `cargo check -p warp --tests`.
Per the orchestrator's standing instruction for this refactor, nextest and full clippy are skipped.
## Cross-repo coordination summary
- Stage 2b wire shape: `SpawnAgentRequest.skip_initial_turn` is removed; `TaskAssignmentMessage.SkipInitialTurn` (top-level bool, JSON tag `skip_initial_turn`, `omitempty`) is added between warp-server-4 and oz-agent-worker. The CLI `--skip-initial-turn` flag is unchanged and remains the worker→CLI contract.
- Stage 2c wire shape: `OrderedTerminalEventType::AmbientSetupPhaseEnded` is added between session-sharing-protocol and session-sharing-server. The testing-only protocol dep swap in `Cargo.toml:248` keeps the locally-running relay decoding the new variant until the protocol PR merges and the relay picks up the `rev` bump.
