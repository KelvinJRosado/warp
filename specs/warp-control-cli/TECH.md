# Context
`PRODUCT.md` defines a standalone local Warp control CLI binary with an allowlisted action catalog, deterministic addressing across multiple running Warp app processes, and a two-phase implementation plan.
The existing app already has three relevant building blocks:
- `crates/http_server/src/lib.rs (7-61)` runs a native-only loopback Axum server on fixed port `9277`.
- `app/src/lib.rs (1993-2001)` registers that HTTP server in the native app and currently merges only installation-detection and profiling routers.
- `crates/app-installation-detection/src/lib.rs (15-60)` and `app/src/profiling.rs (208-242)` show the current local HTTP routes. They are narrow endpoints, not a general control plane.
Warp also already has the app-side behaviors the control API should reuse rather than reimplement:
- `app/src/terminal/view/action.rs (193-196)` defines split-pane terminal actions.
- `app/src/pane_group/mod.rs (4266-4360, 5377-5414)` shows pane creation/splitting semantics and how split events mutate pane layout.
- `app/src/settings/theme.rs (9-82)` defines persisted theme settings.
- `app/src/themes/theme_chooser.rs (416-458)` shows persisted theme selection behavior.
- `app/src/workspace/action.rs (95-776)` is the largest existing inventory of user-visible workspace actions and informs the allowlist catalog.
- `app/src/workspace/util.rs (12-18)` defines `PaneViewLocator`, and `app/src/pane_group/pane/mod.rs (84-177)` defines serializable pane identifiers, both useful reference points for selector resolution.
- `app/src/uri/mod.rs (822-1093, 1166-1364)` demonstrates external intents being resolved into active windows/workspaces and dispatched into running app state.
The current Oz CLI build/distribution model is also directly relevant because the control CLI should follow the same standalone-artifact approach rather than relying on the Warp GUI executable to service ordinary shell invocations:
- `crates/warp_cli/src/lib.rs (88-188, 316-418)` defines the existing CLI/parser conventions and channel-specific command naming support.
- `app/src/lib.rs (631-746)` routes CLI invocations into CLI execution rather than GUI launch.
- `script/macos/bundle (353-735)` and `script/linux/bundle (157-294)` build standalone CLI artifacts with the `standalone` feature.
- `.github/workflows/create_release.yml (423-554, 660-858, 992-1276)` publishes macOS/Linux CLI artifacts.
- `script/windows/windows-installer.iss (235-263)` shows the current Windows helper-wrapper pattern for CLI access.
The most important constraint surfaced by this code is that the current fixed-port local HTTP server cannot be the entire solution for a multi-process control API. If multiple local Warp processes attempt to expose mutating routes through the same fixed port, only one can own it. The control design therefore needs explicit per-process discovery and addressing.
## Proposed changes
### 1. Protocol crate and stable envelope
Create a small shared protocol crate or equivalent shared module used by both the app server and standalone CLI client. It should define:
- Protocol version metadata.
- Discovery/health response types.
- Selector types:
  - `InstanceSelector`
  - `WindowSelector`
  - `TabSelector`
  - `PaneSelector`
  - `SessionSelector`
- Opaque protocol-facing ID newtypes for instance/window/tab/pane/session identifiers.
- Allowlisted `ControlAction` variants and typed parameter payloads.
- Success/error envelopes with stable machine-readable error codes.
The protocol should treat target IDs as opaque. The app may encode existing runtime identifiers internally, but the public wire contract should not require callers to understand `EntityId`, `PaneId`, or other implementation types.
Recommended top-level request shape:
```json
{
  "protocol_version": 1,
  "request_id": "client-generated-id",
  "action": "pane.split",
  "target": {
    "window": "active",
    "tab": "active",
    "pane": "active"
  },
  "params": {
    "direction": "right"
  }
}
```
Recommended response shape:
```json
{
  "ok": true,
  "protocol_version": 1,
  "request_id": "client-generated-id",
  "instance_id": "opaque-instance-id",
  "resolved_target": {
    "window_id": "opaque-window-id",
    "tab_id": "opaque-tab-id",
    "pane_id": "opaque-pane-id"
  },
  "result": {}
}
```
Error payloads should include a stable code such as `no_instance`, `ambiguous_instance`, `stale_target`, `invalid_selector`, `unsupported_action`, `not_allowlisted`, `invalid_params`, `target_state_conflict`, or `unauthorized_local_client`.
### 2. Per-process discovery instead of fixed-port-only routing
Keep the existing fixed-port HTTP behavior intact for installation detection/profiling compatibility. Add a separate local-control listener that follows the same native Axum/Tokio pattern but supports multiple local Warp app processes.
Recommended design:
- Each participating Warp process creates a random opaque `instance_id` at startup.
- Each process binds a loopback control listener on an ephemeral port or an app-managed available port.
- Each process writes a discovery record into a secure per-user Warp state directory. The record should contain:
  - `instance_id`
  - PID
  - channel/build metadata
  - control-listener endpoint
  - protocol version
  - start timestamp
  - local-auth material reference or token metadata
- The CLI loads discovery records, removes or ignores stale records after health checks, and chooses an instance using the product selector rules.
- `instances list` is a CLI-first projection of this discovery registry plus health responses.
This design preserves the current `9277` behavior while avoiding cross-process port contention for the new control API.
### 3. Local authentication boundary
Mutating localhost routes should not copy the permissive CORS posture of `/install_detection`.
Recommended local trust model:
- No browser-readable CORS allowance on control endpoints.
- Per-instance random bearer token or equivalent local credential stored in the discovery record or adjacent secure local state with user-only permissions.
- CLI automatically loads and presents the credential.
- The app rejects missing/invalid local credentials before action resolution.
- Health metadata exposed without credentials, if needed for stale-record pruning, must not reveal mutating capabilities or sensitive target state.
This keeps the protocol local and scriptable without creating an ambient browser-to-localhost control surface.
### 4. App-side request bridge onto the UI/application context
The HTTP handler should not mutate Warp models directly from the server runtime thread. Introduce a control bridge that safely hands requests back to app-owned execution context and waits for a typed response.
Recommended shape:
- `LocalControlModel` or equivalent singleton created during app initialization.
- An async request channel shared with the control router.
- Each inbound HTTP request:
  1. Authenticates.
  2. Deserializes and validates the protocol request.
  3. Sends a typed work item plus oneshot response handle to the app-side control model.
  4. Awaits the result and serializes the response envelope.
- The app-side control model resolves selectors and executes the allowlisted action on the correct app context/view model boundary.
This mirrors existing external-intent handling in `app/src/uri/mod.rs` but returns structured results rather than fire-and-forget behavior.
### 5. Target resolution model
Implement target resolution as a reusable component rather than scattering lookup logic across handlers.
Recommended resolution order:
1. Select instance in the CLI/discovery layer.
2. Resolve window inside the target process.
3. Resolve tab within the window.
4. Resolve pane within the tab/pane-group context.
5. Resolve session only for session-scoped commands.
Selector behavior:
- `active` resolves from current app focus/selection state.
- Explicit opaque IDs must resolve exactly or return `stale_target`.
- Index selectors are allowed only for user-visible indexed concepts such as tabs and should resolve to a concrete opaque ID before execution.
- A session-scoped request against a non-terminal pane returns `target_state_conflict`.
Implementation references:
- Window-level active selection already exists inside the app through `WindowManager`.
- Pane scoping can build on the conceptual model of `PaneViewLocator` in `app/src/workspace/util.rs (12-18)`.
- Existing URI intent routing in `app/src/uri/mod.rs (895-1093)` shows how to locate workspaces/windows and avoid silently acting in the wrong place.
### 6. Allowlisted handler families
Use one handler module per action family. The protocol layer owns parsing/validation; handler modules own target resolution and delegation to existing app logic.
Recommended modules/families:
- Discovery/state:
  - instances, version, active chain, windows/tabs/panes/sessions listings.
- Window/tab:
  - new, focus, close, activate, move, rename, color, close variants.
- Pane:
  - split, focus, navigate, close, maximize, resize.
- Input/session:
  - insert, replace, clear, run command, cycle session, mode switch where supported.
- Appearance/settings:
  - theme list/set, system-theme controls, font/zoom actions, allowlisted settings reads/writes/toggles.
- Panels/surfaces:
  - settings/page/search, palettes, left/right panels, Drive, resource center, code review, vertical tabs, AI assistant.
Do not use a generic “dispatch action by string” endpoint. Every handler should be reachable only through an explicit `ControlAction` variant.
### 7. First PR: prove the end-to-end vertical slice
The first implementation PR should land the minimum cross-cutting architecture plus one or two representative mutations:
- Shared protocol types and error envelopes.
- Discovery registry and CLI instance selection.
- A standalone CLI binary or artifact path that runs control commands without starting the GUI app runtime.
- Per-process authenticated local-control server.
- App-side request bridge and selector resolver.
- Read-only `ping/version` plus `instances list` or equivalent minimal discovery command.
- End-to-end `pane.split` for the active pane.
- End-to-end `theme.set` for a named fixed theme.
Why these two actions:
- `pane.split` proves a UI/layout action can be targeted and executed against live app state.
- `theme.set` proves a persisted settings mutation can be invoked remotely and reflected in the running app.
The PR should also introduce the shell-facing CLI command grammar that the remainder of the protocol will reuse and establish a lightweight CLI startup path distinct from GUI startup.
### 8. Second PR: fill out the remaining protocol in parallel
After the first PR validates discovery, auth, selector resolution, CLI syntax, and server-to-app execution, a second PR can add the remaining allowlisted catalog in parallelized action-family slices. The baseline code should make new action additions mostly additive:
- Extend `ControlAction`.
- Add typed params/results.
- Add a handler.
- Add validation/tests.
- Add CLI surface/tests.
### 9. CLI packaging and release shape
The shipped product shape should be a separate bundled CLI binary that reuses shared CLI/protocol crates but does not depend on launching the GUI binary in command mode. Follow the Oz CLI release model as closely as practical:
- macOS:
  - Add a standalone control CLI artifact path next to the existing Oz standalone CLI artifact flow.
  - If the app bundle also exposes a wrapper/install flow, keep channelized naming consistent with the final product name decision.
- Linux:
  - Extend bundle/release scripts to emit control CLI standalone artifacts and packages in the same broad pattern as the current Oz CLI tarball/deb/rpm/Arch package flow.
- Windows:
  - Mirror the existing installer-generated helper-wrapper pattern first if that remains the canonical Oz behavior on Windows.
  - If the product decision is to ship a true standalone Windows control CLI binary, add a dedicated release path in follow-up work rather than silently diverging from existing Oz precedent.
Startup and dependency expectations:
- The CLI process should initialize only command parsing, discovery, authentication material loading, protocol serialization, HTTP transport, and output formatting needed for the requested command.
- The CLI should not initialize GUI state, rendering, terminal session models, app workspaces, or other main-app-only subsystems.
- Startup cost should be treated as part of the product contract because control commands are expected to compose naturally in scripts and repeated interactive shell usage.
Naming decision:
- Product examples use `warp ...`, and that should be the target public command surface.
- Final artifact filenames, channelized aliases, and installer exposure should be chosen before implementation to avoid churn in bundle scripts, docs, shell completions, and release workflow files.
## End-to-end flow
```mermaid
sequenceDiagram
    participant CLI as Warp control CLI
    participant REG as Local discovery registry
    participant PROC as Selected Warp process
    participant HTTP as Local control listener
    participant BRIDGE as App request bridge
    participant RES as Target resolver
    participant ACT as Allowlisted action handler
    participant UI as Live Warp app state

    CLI->>REG: Read local instance records
    CLI->>PROC: Health/protocol check for candidates
    PROC-->>CLI: Instance metadata + compatibility
    CLI->>CLI: Resolve instance selector
    CLI->>HTTP: Authenticated POST action request
    HTTP->>BRIDGE: Typed request + response channel
    BRIDGE->>RES: Resolve window/tab/pane/session selectors
    RES-->>BRIDGE: Concrete target handles or typed error
    BRIDGE->>ACT: Execute allowlisted ControlAction
    ACT->>UI: Reuse existing app action/settings behavior
    UI-->>ACT: Mutation/read result
    ACT-->>BRIDGE: Typed result
    BRIDGE-->>HTTP: Response envelope
    HTTP-->>CLI: JSON success/error response
    CLI-->>CLI: Pretty or JSON output
```
## Testing and validation
Map tests directly to `PRODUCT.md` behavior.
- Behavior 1-6, 29-31:
  - Protocol version/unit tests.
  - Discovery-registry tests with zero, one, multiple, stale, and incompatible instance records.
  - Local-auth tests for missing/invalid/valid credentials.
- Behavior 7-13:
  - Selector-resolution unit tests for active, explicit ID, index, stale target, ambiguous target, and non-terminal session target.
  - Tests that no lower-level selector silently retargets after an explicit stale selector fails.
- Behavior 15-28:
  - Parser/serde tests for every first-PR `ControlAction` variant.
  - Router tests proving unknown/unallowlisted actions are rejected.
  - CLI parse/output tests for pretty and JSON rendering.
- Behavior 19 and 33:
  - App-side tests for active-pane split using the existing pane/layout helpers or a narrow extracted helper.
  - Manual local verification that `warp pane split --direction right` changes a running app as expected.
- Behavior 21-23 and 33:
  - App-side tests for setting a fixed theme and observing persisted theme model changes.
  - Manual local verification that `warp theme set ...` changes the running app and persists according to existing setting semantics.
- Behavior 30:
  - Multi-process integration-style coverage using two synthetic discovery records and mock health responders, plus manual testing with multiple channel builds where practical.
- Packaging:
  - `--artifact cli`-style bundle smoke tests or script-level checks for each supported platform path touched by the first PR.
  - Startup-path tests or focused checks confirming the standalone CLI dispatches commands without entering GUI-app launch code.
  - Shell completions/help output checks once final command naming is selected.
## Parallelization
The first PR should stay mostly sequential because protocol envelope, discovery, authentication, selector resolution, and the two proof actions are tightly coupled and need one coherent architecture.
The second PR is a strong fit for remote Oz cloud-agent fan-out after the first PR lands. Proposed parallel workstreams:
- `control-window-tab-pane` — remote agent owns window/tab/pane action expansion, including CLI syntax, protocol variants, app handlers, and tests. Branch suggestion: `zach/warp-control-cli-window-tab-pane`.
- `control-settings-appearance` — remote agent owns settings/theme/font/zoom allowlist expansion and validation. Branch suggestion: `zach/warp-control-cli-settings-appearance`.
- `control-input-surfaces` — remote agent owns session/input plus panel/palette/settings-surface commands, with extra care around command execution risk. Branch suggestion: `zach/warp-control-cli-input-surfaces`.
- `control-introspection-packaging` — remote agent owns richer list/read commands, documentation/examples, and any follow-on bundle/release plumbing not completed in PR1. Branch suggestion: `zach/warp-control-cli-introspection-packaging`.
Merge strategy:
- Each remote agent works from the first PR’s merged baseline or a designated PR2 integration base.
- Each returns a branch or compact patch plus validation notes.
- A lead integrator folds accepted slices into one combined second PR so the public protocol remains coherent.
```mermaid
flowchart LR
    P1["PR1 merged<br/>protocol + discovery + bridge + split/theme"] --> Launch["Launch PR2 cloud agents"]
    Launch --> A["control-window-tab-pane"]
    Launch --> B["control-settings-appearance"]
    Launch --> C["control-input-surfaces"]
    Launch --> D["control-introspection-packaging"]
    A --> Merge["Lead integrates protocol additions"]
    B --> Merge
    C --> Merge
    D --> Merge
    Merge --> Validate["Full validation + docs review"]
    Validate --> P2["Single PR2 with remaining allowlist"]
```
## Risks and mitigations
- Fixed-port server assumptions:
  - Mitigation: leave current `9277` endpoints undisturbed and use a per-process control listener plus discovery registry.
- Browser-to-localhost abuse:
  - Mitigation: no permissive CORS, explicit local auth, and mutating routes gated before selector resolution.
- Action catalog drift from real UI behavior:
  - Mitigation: each control action reuses or factors existing UI action paths rather than duplicating behavior.
- Leaking internal unstable identifiers:
  - Mitigation: public protocol exposes opaque IDs and selectors; internal runtime IDs stay implementation details.
- Over-broad settings mutation:
  - Mitigation: allowlisted setting keys only, with private/debug/derived settings rejected.
- Command execution risk:
  - Mitigation: keep `input.run`/session execution in the catalog but require explicit follow-up product/review decision before broad rollout.
- Packaging churn due to unresolved executable naming:
  - Mitigation: settle naming before touching bundle scripts and release workflow files.
- Heavyweight CLI startup caused by sharing the GUI binary's launch path:
  - Mitigation: ship a separate control CLI artifact with a narrow initialization path and keep GUI-only subsystems out of ordinary CLI command execution.
## Follow-ups
- Decide the final artifact filename/channel alias scheme around the `warp ...` public command surface.
- Decide whether Windows should follow the current Oz wrapper pattern indefinitely or gain standalone control CLI artifacts.
- Decide whether a future subscription/watch protocol is useful for scripts that want live state changes, rather than single request/response calls only.
