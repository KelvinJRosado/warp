# Empty-Prompt Local-to-Cloud Handoff — Stage 2 Sub-Tech-Spec (warp-4)
Sub-tech-spec for what **Stage 2** of REMOTE-1499 delivers on the warp-4 side. The full end-to-end architecture lives in `TECH.md`; the full product behavior lives in `PRODUCT.md`. This document is scoped to the contents of `harry/empty-prompt-handoff-local`.
Branch: `harry/empty-prompt-handoff-local`, stacked on Stage 1's `harry/empty-prompt-handoff-wire-contract`.
Sibling specs (cross-repo):
- `../../../warp-server-4/specs/REMOTE-1499/STAGE-2.md` — server-side `ShouldSkipInitialTurn` derivation + `AmbientSetupPhaseEnded` protocol-rev bump.
- `../../../oz-agent-worker/specs/REMOTE-1499/STAGE-2.md` — self-hosted worker side of the skip-initial-turn flag.
- `../../../session-sharing-protocol/specs/REMOTE-1499/STAGE-2.md` — `OrderedTerminalEventType::AmbientSetupPhaseEnded` variant.
- `../../../session-sharing-server/specs/REMOTE-1499/STAGE-2.md` — testing-only protocol dep swap.
## Scope
Stage 2 delivers the user-facing empty-prompt handoff behavior end-to-end on the warp-4 side, all gated behind `FeatureFlag::EmptyPromptHandoff`. It is organized below by client surface:
- **Feature flag and three entry points** — chip / `&` / `/handoff` all dispatch the same immediate-handoff launch.
- **Wire-level substitution, indicator, and telemetry** — `build_handoff_spawn_request`'s substitution rules, the `empty_prompt_handoff_indicator` enum, the view rendering, and the analytics events.
- **Skip-initial-turn signal** — the `AgentDriver` reads the `--skip-initial-turn` CLI flag rather than destructuring a stored client-side bool.
- **Setup-phase teardown marker** — `AgentDriver::execute_run` emits `AmbientSetupPhaseEnded` on every cloud agent run; the viewer event loop tears down the Cloud Mode Setup V2 UI on receipt.
Cross-repo coordination is summarized at the end of this doc.
## Feature flag
`FeatureFlag::EmptyPromptHandoff` is declared in `crates/warp_features/src/lib.rs`. Default off. Not added to `DOGFOOD_FLAGS`.
## Client entry points
All three entry points converge in `start_local_to_cloud_handoff` (`app/src/workspace/view.rs:13652-13663`), which synthesizes an empty `PendingCloudLaunch { prompt: "".to_owned(), attachments: vec![] }` when `EmptyPromptHandoff` is on. There is no separate handoff compose pane for any of the three entry points — all three result in the same immediate-handoff dispatch.
- `app/src/ai/blocklist/agent_view/agent_input_footer/mod.rs:2547-2574` — the `OpenHandoffPane` action dispatches `WorkspaceAction::OpenLocalToCloudHandoffPane { launch: None, environment_id: None, entry_point: HandoffEntryPoint::FooterChip }` directly when `EmptyPromptHandoff` is on, skipping `&` compose mode.
- `app/src/terminal/input.rs:4060-4067` — `maybe_launch_cloud_handoff_request` builds a `PendingCloudLaunch { prompt: "".to_owned(), attachments }` for `&` + Enter on an empty buffer. Entry point: `HandoffEntryPoint::Ampersand`.
- `app/src/terminal/input/slash_commands/mod.rs:924-940` — `/handoff` with no argument dispatches the same `OpenLocalToCloudHandoffPane { launch: None, ... }` as the chip when `EmptyPromptHandoff` is on. Entry point: `HandoffEntryPoint::SlashCommand`.
## Wire-level substitution
`app/src/terminal/view/ambient_agent/model.rs:686-743` `build_handoff_spawn_request`:
- When the submitted prompt is empty AND `pending_handoff.source_conversation_in_progress` is true: substitute `prompt: Some("continue in the cloud".to_owned())` on the wire.
- Otherwise (idle source, regardless of snapshot): send `prompt: None`.
- When the submitted prompt is non-empty: pass it through unchanged.
`source_conversation_in_progress` is captured once at handoff initiation by reading `BlocklistAIHistoryModel::active_conversation(...).status()` and stored on the `PendingHandoff` struct so it can't drift mid-flow.
## Queued-prompt indicator and rendering
`app/src/terminal/view/ambient_agent/model.rs:565-601` declares `EmptyPromptHandoffIndicator { Continue, SnapshotRehydrationOnly, None }` and computes the variant from `(source_conversation_in_progress, has_snapshot)`:
- In-progress source → `Continue` (label `"Continuing previous task in the cloud"`).
- Idle + non-empty snapshot → `SnapshotRehydrationOnly` (label `"Applying workspace changes…"`).
- Idle + empty snapshot → `None` (no empty-prompt indicator block; the standard Cloud Mode Setup V2 setup indicator covers the warmup phase).
`app/src/terminal/view/ambient_agent/view_impl.rs:154-189` consumes the indicator and inserts a labeled queued-prompt block in place of the literal-prompt block. The label string is **decoupled** from the wire-substitution string; the two are independently tunable for design iteration.
## Telemetry
`app/src/ai/ambient_agents/telemetry.rs`:
- `CloudAgentTelemetryEvent::HandoffInitiated` carries `empty_prompt: bool` and `injection_path: HandoffInjectionPath { None | Continue | SnapshotRehydrationOnly }`.
- `CloudAgentTelemetryEvent::HandoffSnapshotPrepared { had_snapshot: bool }` fires after `derive_touched_workspace` settles. Analytics joins this against `HandoffInitiated.injection_path` to learn whether `SnapshotRehydrationOnly` paths actually carried snapshot content.
## Skip-initial-turn signal
The decision "should the cloud agent skip its initial LLM turn?" is computed fresh per execution on the server (`common.ShouldSkipInitialTurn(task, execution)` in warp-server-4) and reaches the sandboxed CLI as the `--skip-initial-turn` flag. The flag is the entire worker→driver contract; the wire shape between client and server is silent on it.
- `app/src/server/server_api/ai.rs` — `SpawnAgentRequest` carries no `skip_initial_turn` field. The client never derives or transmits this signal.
- `app/src/terminal/view/ambient_agent/model.rs:686-743` (`build_handoff_spawn_request`) and `:1264` (`spawn_agent`) decide only the wire-level prompt; neither emits a `skip_initial_turn` value.
- `app/src/ai/agent_sdk/driver.rs:405-410` — the `AgentRunPrompt::ServerSide` variant carries only `skill: Option<ParsedSkill>` and `attachments_dir: Option<String>`. The skip-initial-turn signal is intentionally not part of the prompt variant because the variant must round-trip through `prepare_harness` (which is harness-agnostic) without ferrying a flag that's meaningful only on the Oz harness.
- `app/src/ai/agent_sdk/driver.rs` — `AgentDriverOptions` and `AgentDriver` each carry a `skip_initial_turn: bool` field. The value is sourced from `RunAgentArgs::skip_initial_turn` (which the clap parser populates from `--skip-initial-turn`) by the `build_driver_options_and_task` closure in `mod.rs:855`, then threaded into `AgentDriver::new`. The `new_for_test` constructor at `driver.rs:711` initializes the field to `false`.
- `app/src/ai/agent_sdk/driver.rs:2410-2411` — the gate in `execute_run` is
  ```rust path=null start=null
  let is_skip_initial_turn =
      self.skip_initial_turn && matches!(&task_prompt, AgentRunPrompt::ServerSide { .. });
  ```
  The `ServerSide` match gates the flag onto the Oz harness path. Third-party harnesses always resolve the server-side prompt through `prepare_harness` and ignore the flag by construction.
- `crates/warp_cli/src/agent.rs:368-372` — `--skip-initial-turn` is a `requires = "task_id"` boolean flag on `oz agent run`. The CLI parser test at `crates/warp_cli/src/lib_tests.rs:233-252` (`agent_run_accepts_skip_initial_turn_with_task_id`) pins its parsing and constraint.
### Considered alternatives
- **Storing the decision on the task config snapshot at dispatch time.** Rejected: a single stored decision drifts across executions. A cloud→cloud follow-up that submits a non-empty prompt against the same task would inherit the stamped flag and incorrectly skip the LLM turn. Computing fresh per execution makes the decision reactive to the current execution input and trivially supports future content sources (e.g. orchestration system prompts) without changes outside `ShouldSkipInitialTurn`.
- **Deriving the flag client-side and shipping it on `SpawnAgentRequest`.** Rejected for the same reason: the client only sees the first execution and cannot reactively re-derive for subsequent executions. Centralizing on the server keeps the worker, the wire shape, and the driver simple and concentrates the policy in one place.
## Setup-phase teardown marker
Every cloud agent run signals "environment setup phase complete" via the `OrderedTerminalEventType::AmbientSetupPhaseEnded` shared-session-protocol marker. The sharer emits the marker once setup commands have finished; the viewer's event loop receives it and tears down the Cloud Mode Setup V2 UI. The marker is path-agnostic — it fires on both the skip-initial-turn path (no first LLM turn) and the normal `ServerSide` path (a first LLM turn follows). This makes the setup-phase teardown independent of whether a first `AppendedExchange` event will ever fire.
### Testing-only Cargo.toml swap
`Cargo.toml:248` — the `session-sharing-protocol` dep is set to `path = "../session-sharing-protocol"` while the protocol PR is in flight. Reverted to `git = ..., rev = <merged SHA>` after the protocol PR merges; the locally-running session-sharing-server must pick up the same `rev` bump before warp-4 lands, or the relay will type-decode `OrderedTerminalEventType` against an older protocol rev that lacks the new variant and silently drop the marker. Tracked in PRODUCT.md "Deferred follow-ups".
### `TerminalModel` helper
`app/src/terminal/model/terminal_model.rs` — `send_ambient_setup_phase_ended_for_shared_session` is modeled on the adjacent `send_agent_conversation_replay_started_for_shared_session`. The helper is a no-op for non-sharer terminals; sharer terminals emit a typed `OrderedTerminalEventType::AmbientSetupPhaseEnded` event through the existing shared-session event channel.
### `AgentDriver::execute_run` structure
`app/src/ai/agent_sdk/driver.rs` — `execute_run` is structured so the skip branch builds `IdleTimeoutSender` first, runs the skip block (`enter_agent_view` + emit marker via `send_ambient_setup_phase_ended_for_shared_session` + `complete_with_optional_idle`) before the history subscription, sets up the subscription, then conditionally dispatches the non-skip prompt. Scheduling the timer before the subscription means a later `AppendedExchange` from a session-sharing-protocol follow-up correctly invalidates the timer via `IdleTimeoutSender`'s internal generation counter.
### Marker emission on the non-skip path
`app/src/ai/agent_sdk/driver.rs:2760-2792` — the non-skip `AgentRunPrompt::ServerSide` arm invokes `terminal.model.lock().send_ambient_setup_phase_ended_for_shared_session()` after `terminal.enter_agent_view(None, restored_conversation_id, AgentViewEntryOrigin::Cli, ctx)` and before the `terminal.ai_controller().update(...)` block that fires `AIAgentInput::StartFromAmbientRunPrompt`. This mirrors the skip-path emission at `driver.rs:2418-2425`. The `AgentRunPrompt::Local` arm intentionally does not call the helper — local runs do not have a setup phase, and even though the helper is internally guarded by `is_sharer()`, keeping the surface narrow makes the emission boundary obvious.
### `IdleTimeoutSender::complete_with_optional_idle`
`IdleTimeoutSender::complete_with_optional_idle(idle_on_complete, value)` defers via `end_run_after` when `idle_on_complete` is `Some(d)` and falls back to `end_run_now` when `None`. The `UpdatedConversationStatus` and harness-exit branches in `execute_run` use the same helper so all completion paths honor the optional idle window uniformly.
### Viewer event_loop.rs arm
`app/src/terminal/shared_session/viewer/event_loop.rs:348-376` — the `OrderedTerminalEventType::AmbientSetupPhaseEnded` arm flips `BlockList::set_is_executing_oz_environment_startup_commands(false)`, then calls `AmbientAgentViewModel::tear_down_active_setup_command_group` which runs `finish_setup_command_group` + `set_setup_command_group_visibility(false)`. "No active group" is a no-op for idempotency. The arm is path-agnostic — it handles both the skip-initial-turn path and the normal cloud agent path.
### Legacy fallback teardowns
Two `BlocklistAIHistoryEvent::AppendedExchange`-driven teardowns at `app/src/terminal/view.rs:5496-5507` and `app/src/terminal/view/ambient_agent/block/setup_command_text.rs:119-136` remain in place as a compatibility fallback for viewers that connect to sharers running pre-feature builds. Both teardowns are idempotent with the `AmbientSetupPhaseEnded` arm, so a new sharer + new viewer pair triggering both is harmless. Inline comments on these blocks call out the compat rationale and point at `event_loop.rs` for the canonical signal. Removal is tracked in PRODUCT.md "Deferred follow-ups".
### Considered alternatives
- **Reusing `AppendedExchange` as the teardown signal everywhere.** Rejected: the skip-initial-turn path never fires `AppendedExchange`, so any teardown that depends on it would leave the cloud pane stuck in the "setting up…" UI. A dedicated marker decouples setup-phase teardown from first-LLM-turn semantics.
- **Letting the AgentDriver send `Success` directly to the oneshot on the skip path (no `IdleTimeoutSender` involvement).** Rejected: the driver tears down ~80ms after sending `Success`, which is too fast for a follow-up session-sharing-protocol exchange to arrive. Routing the skip path through `IdleTimeoutSender::complete_with_optional_idle` honors `idle_on_complete` uniformly across completion paths.
## Tests
- Behavioral tests in `app/src/terminal/view/ambient_agent/model_tests.rs` cover empty-prompt auto-submit, the three `EmptyPromptHandoffIndicator` variants, and feature-flag gating.
- The CLI parser test at `crates/warp_cli/src/lib_tests.rs:233-252` (`agent_run_accepts_skip_initial_turn_with_task_id`) pins the `--skip-initial-turn` flag's parsing and `requires = "task_id"` constraint.
- Viewer-side tests in `app/src/terminal/shared_session/viewer/event_loop_tests.rs` cover the `AmbientSetupPhaseEnded` arm and its idempotency.
- Sandbox-side unit tests cover `TerminalModel::send_ambient_setup_phase_ended_for_shared_session` (sharer-emits + non-sharer-no-op).
- Direct `IdleTimeoutSender::complete_with_optional_idle` tests in `app/src/ai/agent_sdk/driver_tests.rs` cover `None` immediate completion, `Some(d)` deferred completion, and `Some(d)` + cross-path `cancel_idle_timeout()` invalidation.
- The driver-side `AmbientSetupPhaseEnded` emission on the non-skip path is exercised end-to-end by the standard cloud-mode handoff smoke test.
## Validation
- `cargo fmt --all --check`.
- `cargo check -p warp --tests`.
Nextest and full clippy are intentionally not part of the per-PR validation for this work — the changes touch isolated client-side wiring, and the targeted `cargo check` plus the per-stage unit tests cover the relevant surfaces.
## Cross-repo coordination summary
- `SpawnAgentRequest` (`POST /agent/run`): does not carry `skip_initial_turn`. The wire-shape change is local to this stage and is the only Stage-2 contract between warp-4 and warp-server-4.
- `TaskAssignmentMessage` (warp-server-4 → self-hosted worker): top-level `SkipInitialTurn bool` (JSON tag `skip_initial_turn`, `omitempty`). warp-server-4 computes the bool fresh per execution via `ShouldSkipInitialTurn` and stamps it onto the message. The self-hosted worker forwards the bool to `--skip-initial-turn`.
- `--skip-initial-turn` CLI flag (worker → CLI): the sole worker→driver contract for the skip-initial-turn decision.
- `OrderedTerminalEventType::AmbientSetupPhaseEnded` (sharer → viewer via session-sharing-protocol): new variant in the protocol crate. The testing-only protocol dep swap in `Cargo.toml:248` keeps the locally-running relay decoding the new variant until the protocol PR merges and the relay picks up the `rev` bump.
