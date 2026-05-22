use warp_cli::agent::Harness;

pub(crate) const CLAUDE_CODE_INSTALL_DOCS_URL: &str =
    "https://docs.warp.dev/guides/integrations/how-to-set-up-claude-code";
pub(crate) const LOCAL_HARNESS_INSTALLATION_REQUIRED_TOOLTIP: &str =
    "Harness installation required. Learn more";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalChildHarnessSetupState {
    Ready,
    ProductDisabled {
        message: &'static str,
    },
    MissingHarness {
        tooltip: &'static str,
        docs_url: &'static str,
    },
}

impl LocalChildHarnessSetupState {
    pub(crate) fn is_selectable(self) -> bool {
        matches!(self, Self::Ready)
    }
}

pub(crate) fn local_child_harness_disabled_message(harness: Harness) -> Option<&'static str> {
    match harness {
        Harness::Codex => Some("Local Codex child agents are temporarily disabled."),
        Harness::Oz | Harness::Claude | Harness::OpenCode | Harness::Gemini | Harness::Unknown => {
            None
        }
    }
}

pub(crate) fn local_child_harness_is_enabled(harness: Harness) -> bool {
    local_child_harness_disabled_message(harness).is_none()
}

pub(crate) fn local_child_harness_setup_state(harness: Harness) -> LocalChildHarnessSetupState {
    local_child_harness_setup_state_with_cli_resolver(harness, local_cli_is_installed)
}

fn local_child_harness_setup_state_with_cli_resolver(
    harness: Harness,
    cli_is_installed: impl Fn(&str) -> bool,
) -> LocalChildHarnessSetupState {
    if let Some(message) = local_child_harness_disabled_message(harness) {
        return LocalChildHarnessSetupState::ProductDisabled { message };
    }

    match harness {
        Harness::Claude if !cli_is_installed("claude") => {
            LocalChildHarnessSetupState::MissingHarness {
                tooltip: LOCAL_HARNESS_INSTALLATION_REQUIRED_TOOLTIP,
                docs_url: CLAUDE_CODE_INSTALL_DOCS_URL,
            }
        }
        Harness::Oz | Harness::Claude | Harness::OpenCode | Harness::Gemini | Harness::Unknown => {
            LocalChildHarnessSetupState::Ready
        }
        Harness::Codex => unreachable!("Codex is handled by local_child_harness_disabled_message"),
    }
}

fn local_cli_is_installed(command: &str) -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        crate::util::path::resolve_executable(command).is_some()
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = command;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_is_product_enabled_when_cli_is_installed() {
        assert_eq!(
            local_child_harness_setup_state_with_cli_resolver(Harness::Claude, |_| true),
            LocalChildHarnessSetupState::Ready
        );
    }

    #[test]
    fn claude_is_disabled_for_missing_cli_with_docs() {
        assert_eq!(
            local_child_harness_setup_state_with_cli_resolver(Harness::Claude, |_| false),
            LocalChildHarnessSetupState::MissingHarness {
                tooltip: LOCAL_HARNESS_INSTALLATION_REQUIRED_TOOLTIP,
                docs_url: CLAUDE_CODE_INSTALL_DOCS_URL,
            }
        );
    }

    #[test]
    fn codex_remains_product_disabled() {
        assert_eq!(
            local_child_harness_setup_state_with_cli_resolver(Harness::Codex, |_| true),
            LocalChildHarnessSetupState::ProductDisabled {
                message: "Local Codex child agents are temporarily disabled.",
            }
        );
    }
}
