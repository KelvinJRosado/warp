# Summary
Warp should ship an allowlisted standalone local control CLI binary that lets developers script the same classes of user-visible actions they can already perform inside the running app: manipulating windows, tabs, panes, sessions, appearance, settings, and selected UI surfaces. The CLI should operate against one or more already-running local Warp app processes through a stable machine protocol, with deterministic target selection and clear errors when a process or target is ambiguous.
## Problem
Warp already has rich interactive actions, but they are primarily reachable through UI, keybindings, menus, or deeplinks. Developers cannot reliably compose those same actions into shell scripts, demos, automation, or agent workflows, and there is no general local protocol for addressing a specific running Warp instance, window, pane, or session.
## Goals / Non-goals
Goals:
- Provide a first-class, scriptable standalone local CLI binary for controlling running Warp app processes.
- Keep CLI startup lightweight by avoiding GUI-app startup or full terminal initialization for routine control commands.
- Keep the surface allowlisted and finite instead of exposing arbitrary internal actions.
- Make targeting explicit and deterministic across multiple Warp processes, windows, tabs, panes, and sessions.
- Support both ergonomic active-target defaults and precise selectors for automation.
- Define a complete protocol/catalog up front, while shipping the implementation incrementally.
Non-goals:
- Replacing the Oz CLI or mixing cloud-agent management into this CLI.
- Exposing every internal app action, debug action, developer-only helper, or privileged state mutation.
- Treating the CLI as a general RPC escape hatch into Warp internals.
- Requiring developers or automation to spawn the Warp GUI executable in CLI mode for ordinary control commands.
- Requiring the first implementation PR to ship every action in the catalog.
## Behavior
1. The Warp control CLI operates only on running local Warp app processes. If no compatible Warp process is available, the CLI exits non-zero with a clear “no running Warp instance found” error.
2. The CLI exposes only explicitly allowlisted actions. Unknown action names, unsupported parameter combinations, or requests for non-allowlisted capabilities fail with structured errors; they are never forwarded to arbitrary internal dispatch.
3. Every successful mutating request identifies:
   - The Warp process instance that executed it.
   - The resolved target, when the action addresses a window, tab, pane, or session.
   - A success payload suitable for JSON output.
4. Every failure identifies:
   - A stable machine-readable error code.
   - A human-readable explanation.
   - Any selector that was ambiguous, missing, stale, unsupported, or invalid.
5. The CLI supports human-readable output by default and JSON output for scripts. JSON output has stable field names and is available for discovery commands, read commands, successful mutations, and failures.
6. The CLI supports process discovery and instance selection:
   - `warp instances list` returns all reachable local Warp app processes that support the protocol.
   - Each process has an opaque `instance_id`, a channel/build identity, and enough display metadata for a developer to choose it.
   - If exactly one compatible process is available, commands may target it implicitly.
   - If multiple compatible processes are available, the CLI may select a single clearly active/frontmost instance when that state is unambiguous; otherwise it fails and asks the developer to pass an explicit instance selector.
   - Developers can explicitly choose an instance by opaque instance ID. Channel or PID filters may be offered as convenience filters, but opaque instance ID is the canonical selector.
7. The CLI supports introspection for target discovery:
   - `warp windows list`
   - `warp tabs list`
   - `warp panes list`
   - `warp sessions list`
   - `warp active get`
   These commands return opaque protocol-facing IDs and enough metadata for subsequent commands without requiring knowledge of internal Warp identifiers.
8. The target selector model is hierarchical:
   - Instance selector resolves a running Warp process.
   - Window selector resolves within the instance.
   - Tab selector resolves within the window.
   - Pane selector resolves within the tab or active pane group context.
   - Session selector resolves within the pane when the pane hosts terminal session state.
9. Every selector family supports an ergonomic `active` form when that concept exists:
   - Active instance, if unambiguous.
   - Active window in the selected instance.
   - Active tab in the selected window.
   - Active pane in the selected tab.
   - Active session in the selected pane.
10. Every selector family supports explicit opaque IDs returned by introspection. Tabs may also support index selectors where index-based workflows are already user-visible, but IDs remain the preferred automation surface.
11. “Active session” means the currently selected terminal session for the resolved pane/window context. If the selected target does not contain a terminal session, session-scoped actions fail rather than silently redirecting elsewhere.
12. When a command omits lower-level selectors, it resolves them from the chosen higher-level context using active defaults. Example: a pane split command with only `--instance` uses that instance’s active window, active tab, and active pane.
13. When an explicitly supplied target disappears between discovery and execution, the request fails with a stale-target error. The CLI must not silently choose a different tab, pane, or session.
14. The protocol is command-oriented, not open-ended state mutation. Each action has a named command, validated parameters, and defined target scope.
15. The complete allowlisted action catalog should be organized into these namespaces.
16. Discovery and read-only state actions:
   - List instances.
   - Get protocol/app version information for one instance.
   - List windows, tabs, panes, and sessions.
   - Get the currently active instance/window/tab/pane/session chain when available.
   - Inspect enough target metadata to let a script decide what to address next.
17. Window actions:
   - Create a new window.
   - Focus a target window.
   - Close a target window.
18. Tab actions:
   - Create a new terminal tab.
   - Create a new agent tab where that is already a user-visible app capability.
   - Activate a target tab.
   - Activate previous, next, or last tab.
   - Move a target tab left or right.
   - Rename or reset a tab title.
   - Set or clear active-tab color where that is already supported in the UI.
   - Close the active tab, a target tab, other tabs, or tabs to the right of a target tab.
19. Pane actions:
   - Split a target pane left, right, up, or down.
   - Optionally choose the shell/session profile for split operations when that already maps to user-facing behavior.
   - Focus a target pane.
   - Navigate focus left, right, up, or down among panes.
   - Close a target pane.
   - Toggle maximize for a target pane.
   - Resize pane dividers left, right, up, or down when that is supported by the app.
20. Session and terminal-input actions:
   - Cycle to the previous or next session where the app exposes session cycling.
   - Insert text into the active input without executing it.
   - Replace the active input buffer.
   - Clear the active input buffer where that matches existing user behavior.
   - Run a command in the target session where the app already supports user-triggered command execution.
   - Switch input mode between terminal and agent modes only where that mode switch is already user-visible and valid for the selected target.
   These commands are part of the protocol catalog, but command execution should be treated as a higher-risk mutating action with explicit confirmation in spec/review before rollout.
21. Appearance actions:
   - List available themes.
   - Set the current fixed theme.
   - Toggle or set “follow system theme.”
   - Set the light and dark themes used when following the system theme.
   - Increase, decrease, or reset font size.
   - Increase, decrease, or reset UI zoom.
   - Set other allowlisted appearance controls only when they correspond to stable user-facing controls.
22. Settings actions:
   - Read allowlisted user-facing settings.
   - Set allowlisted settings to validated values.
   - Toggle allowlisted boolean settings.
   - Reject attempts to mutate private, debug-only, unsafe, derived, or unsupported settings.
   - Return a stable error when a named setting exists internally but is not part of the public local-control allowlist.
23. The settings allowlist should initially cover settings families that are already plainly user-facing and valuable for scripting:
   - Theme/system-theme configuration.
   - Font/zoom-related controls.
   - Notifications.
   - Syntax highlighting and error-underlining toggles.
   - Accessibility verbosity where exposed to users.
   - Selected panel/layout toggles when the user-facing behavior is already stable.
   Additional settings families can be added only by extending the allowlist.
24. Panel and surface actions:
   - Open the general settings surface.
   - Open a specific settings page or settings search result.
   - Open or toggle the command palette with an optional initial query where the app already supports query seeding.
   - Open or toggle command search where that is already user-visible.
   - Toggle or open the left panel, Warp Drive surface, right panel, resource center, AI assistant panel, code review panel, and vertical tabs panel where valid.
25. File/path intent actions may be included when they already mirror existing user-visible deep-link behavior:
   - Open a path in a new tab or window.
   - Open a repository picker or repo path flow where the current app already supports it.
   These should remain allowlisted intent actions rather than arbitrary filesystem RPCs.
26. The following categories are explicitly excluded from the initial public allowlist even if there are internal actions for them:
   - Crash, panic, heap-dump, token-copying, debug-reset, and similar developer/debug helpers.
   - Arbitrary auth manipulation.
   - Arbitrary cloud object mutation or broad Warp Drive CRUD.
   - Arbitrary internal view dispatch by string.
   - Arbitrary setting names outside the allowlist.
27. CLI command names should be noun-oriented and discoverable. The standalone control CLI should expose a `warp ...` command surface:
   - `warp instances list`
   - `warp active get`
   - `warp pane split --direction right`
   - `warp pane split --instance <id> --window active --pane active --direction right`
   - `warp theme set "Warp Dark"`
   - `warp settings set appearance.themes.system_theme true`
   - `warp input insert "cargo check" --replace`
   Channelized install names or aliases may vary during packaging, but the product-level command surface should converge on `warp`.
28. The wire protocol mirrors the CLI model. A mutating request contains:
   - An action name from the allowlist.
   - A structured target selector.
   - Validated parameters.
   A response contains:
   - Success/failure status.
   - Resolved instance and target metadata.
   - Result data or structured error data.
29. The protocol is versioned. Clients must be able to determine whether a running Warp process supports the protocol version and action they intend to call.
30. Multiple running Warp processes are a supported normal case, not an error state. A local stable build and local dev build, or multiple supported local app instances, can coexist; the CLI provides deterministic discovery and addressing rather than assuming one global server.
31. Requests should be scoped to local-user control of the running app. A command that fails authentication or local authorization reports that condition explicitly and does not degrade into a less-specific request.
32. If a selected action is valid in general but impossible in the current UI state, the CLI reports a state-specific failure. Examples include:
   - Splitting a pane that no longer exists.
   - Running a session-scoped command against a non-terminal pane.
   - Focusing a window that has closed.
   - Setting a theme that is not available in that instance.
33. The first implementation PR should ship the smallest end-to-end vertical slice that proves:
   - Process discovery and target resolution work.
   - A standalone CLI binary can reach a running local Warp process without launching or initializing the GUI app.
   - One pane/layout mutation works end-to-end.
   - One settings/appearance mutation works end-to-end.
   Recommended first actions are active-pane split and theme selection, plus the minimum health/introspection commands needed to exercise them.
34. A follow-up PR should fill out the remaining catalog in parallelizable groups once the protocol, discovery model, target resolution, error model, and standalone CLI packaging shape have been validated by the first PR.
