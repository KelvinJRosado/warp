//! Permission checks that map protocol action metadata onto local settings.
use crate::auth::AuthStateProvider;
use crate::features::FeatureFlag;
use crate::settings::{LocalControlPermissionCategory, LocalControlSettings};
use ::local_control::auth::CredentialGrant;
use ::local_control::{
    Action, ActionKind, ControlError, ErrorCode, InvocationContext, PermissionCategory,
};
use warpui::{ModelContext, SingletonEntity};

use crate::local_control::LocalControlBridge;

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static TEST_ALLOW_INPUT_RUN_POLICY: AtomicBool = AtomicBool::new(false);

pub(super) fn warp_control_cli_enabled() -> bool {
    FeatureFlag::WarpControlCli.is_enabled()
}

pub(super) fn ensure_feature_enabled() -> Result<(), ControlError> {
    if warp_control_cli_enabled() {
        return Ok(());
    }
    Err(ControlError::new(
        ErrorCode::LocalControlDisabled,
        "Warp control CLI is disabled by feature flag",
    ))
}

#[cfg(test)]
pub(crate) fn outside_warp_action_enabled_for_settings(
    settings: &LocalControlSettings,
    action: ActionKind,
) -> bool {
    outside_warp_permission_enabled_for_settings(settings, action.metadata().permission_category)
}

#[cfg(test)]
fn outside_warp_permission_enabled_for_settings(
    settings: &LocalControlSettings,
    permission: PermissionCategory,
) -> bool {
    settings.allows_outside_warp(local_permission(permission))
}

#[cfg(test)]
pub(crate) fn capabilities() -> Vec<ActionKind> {
    ActionKind::implemented_metadata()
        .into_iter()
        .map(|metadata| metadata.kind)
        .collect()
}

fn local_permission(permission: PermissionCategory) -> LocalControlPermissionCategory {
    match permission {
        PermissionCategory::ReadMetadata => LocalControlPermissionCategory::MetadataReads,
        PermissionCategory::ReadUnderlyingData => {
            LocalControlPermissionCategory::UnderlyingDataReads
        }
        PermissionCategory::MutateAppState => LocalControlPermissionCategory::AppStateMutations,
        PermissionCategory::MutateMetadataConfiguration => {
            LocalControlPermissionCategory::MetadataConfigurationMutations
        }
        PermissionCategory::MutateUnderlyingData => {
            LocalControlPermissionCategory::UnderlyingDataMutations
        }
    }
}

pub(super) fn ensure_action_allowed(
    context: InvocationContext,
    action: ActionKind,
    ctx: &mut ModelContext<LocalControlBridge>,
) -> Result<(), ControlError> {
    let settings = LocalControlSettings::as_ref(ctx);
    ensure_settings_allow_action(settings, context, action)
}

pub(crate) fn ensure_settings_allow_action(
    settings: &LocalControlSettings,
    context: InvocationContext,
    action: ActionKind,
) -> Result<(), ControlError> {
    if context == InvocationContext::InsideWarp {
        return Err(ControlError::new(
            ErrorCode::ExecutionContextNotAllowed,
            "inside-Warp local-control grants are not implemented",
        ));
    }
    let allowed_contexts = action.metadata().allowed_invocation_contexts;
    if !allowed_contexts.contains(&context) {
        return Err(ControlError::new(
            ErrorCode::ExecutionContextNotAllowed,
            format!(
                "{} is not available in the {} invocation context",
                action.as_str(),
                match context {
                    InvocationContext::InsideWarp => "inside-Warp",
                    InvocationContext::OutsideWarp => "outside-Warp",
                }
            ),
        ));
    }
    if !settings.outside_warp_control_enabled() {
        return Err(ControlError::new(
            ErrorCode::LocalControlDisabled,
            "local control is disabled for this invocation context",
        ));
    }
    let permission = local_permission(action.metadata().permission_category);
    if !settings.outside_warp_permission_enabled(permission) {
        return Err(ControlError::new(
            ErrorCode::InsufficientPermissions,
            format!(
                "{} requires a local-control permission that is disabled",
                action.as_str()
            ),
        ));
    }
    Ok(())
}

pub(super) fn authenticated_user_subject_for_action(
    action: ActionKind,
    ctx: &mut ModelContext<LocalControlBridge>,
) -> Result<Option<String>, ControlError> {
    if !action.metadata().requires_authenticated_user {
        return Ok(None);
    }
    AuthStateProvider::as_ref(ctx)
        .get()
        .user_id()
        .map(|uid| Some(uid.as_string()))
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::AuthenticatedUserUnavailable,
                format!("{} requires a logged-in Warp user", action.as_str()),
            )
        })
}

pub(super) fn ensure_authenticated_scripting_grant(
    grant: &CredentialGrant,
    action: ActionKind,
    ctx: &mut ModelContext<LocalControlBridge>,
) -> Result<(), ControlError> {
    let result = grant.verify_scripting_for_action(action);
    if result.is_err() {
        return result;
    }
    if action.metadata().requires_authenticated_scripting {
        let settings = LocalControlSettings::as_ref(ctx);
        if !settings.outside_warp_authenticated_user_actions_enabled() {
            return Err(ControlError::new(
                ErrorCode::AuthenticatedScriptingRequired,
                format!(
                    "{} requires authenticated scripting grants to be enabled in Settings > Scripting",
                    action.as_str()
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn ensure_scripting_grant_for_settings(
    settings: &LocalControlSettings,
    action: ActionKind,
    grant: &CredentialGrant,
) -> Result<(), ControlError> {
    let result = grant.verify_scripting_for_action(action);
    if result.is_err() {
        return result;
    }
    if action.metadata().requires_authenticated_scripting
        && !settings.outside_warp_authenticated_user_actions_enabled()
    {
        return Err(ControlError::new(
            ErrorCode::AuthenticatedScriptingRequired,
            format!(
                "{} requires authenticated scripting grants to be enabled in Settings > Scripting",
                action.as_str()
            ),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_input_run_policy_allows(
    grant: &CredentialGrant,
    action: &Action,
) -> Result<(), ControlError> {
    if input_run_policy_allows(grant, action) {
        return Ok(());
    }
    Err(ControlError::new(
        ErrorCode::InsufficientPermissions,
        "input.run requires explicit local approval policy before command execution",
    ))
}

#[cfg(not(test))]
fn input_run_policy_allows(_grant: &CredentialGrant, _action: &Action) -> bool {
    false
}

#[cfg(test)]
fn input_run_policy_allows(grant: &CredentialGrant, action: &Action) -> bool {
    grant.action == ActionKind::InputRun
        && action.kind == ActionKind::InputRun
        && TEST_ALLOW_INPUT_RUN_POLICY.load(Ordering::SeqCst)
}

#[cfg(test)]
pub(crate) fn allow_input_run_policy_for_test() -> TestInputRunPolicyGuard {
    TestInputRunPolicyGuard {
        previous: TEST_ALLOW_INPUT_RUN_POLICY.swap(true, Ordering::SeqCst),
    }
}

#[cfg(test)]
pub(crate) struct TestInputRunPolicyGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for TestInputRunPolicyGuard {
    fn drop(&mut self) {
        TEST_ALLOW_INPUT_RUN_POLICY.store(self.previous, Ordering::SeqCst);
    }
}

pub(super) fn ensure_authenticated_user_matches(
    grant: &CredentialGrant,
    ctx: &mut ModelContext<LocalControlBridge>,
) -> Result<(), ControlError> {
    if !grant.authenticated_user.required {
        return Ok(());
    }
    let subject = AuthStateProvider::as_ref(ctx)
        .get()
        .user_id()
        .map(|uid| uid.as_string())
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::AuthenticatedUserUnavailable,
                format!("{} requires a logged-in Warp user", grant.action.as_str()),
            )
        })?;
    if grant.authenticated_user.subject.as_deref() != Some(subject.as_str()) {
        return Err(ControlError::new(
            ErrorCode::AuthenticatedUserMismatch,
            format!(
                "{} credential is bound to a different Warp user",
                grant.action.as_str()
            ),
        ));
    }
    Ok(())
}
