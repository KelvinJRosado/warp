use crate::features::FeatureFlag;
use crate::settings::{
    LocalControlInvocationContext, LocalControlPermissionCategory, LocalControlSettings,
};
use ::local_control::{ActionKind, ControlError, ErrorCode, InvocationContext, PermissionCategory};
use warpui::{ModelContext, SingletonEntity};

use crate::local_control::{LocalControlBridge, LocalControlServer};

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

pub(super) fn outside_warp_any_implemented_action_enabled(
    ctx: &ModelContext<LocalControlServer>,
) -> bool {
    let settings = LocalControlSettings::as_ref(ctx);
    ActionKind::implemented_metadata()
        .into_iter()
        .any(|metadata| {
            outside_warp_permission_enabled_for_settings(settings, metadata.permission_category)
        })
}

#[cfg(test)]
pub(crate) fn outside_warp_action_enabled_for_settings(
    settings: &LocalControlSettings,
    action: ActionKind,
) -> bool {
    outside_warp_permission_enabled_for_settings(settings, action.metadata().permission_category)
}

fn outside_warp_permission_enabled_for_settings(
    settings: &LocalControlSettings,
    permission: PermissionCategory,
) -> bool {
    let context = LocalControlInvocationContext::OutsideWarp;
    settings.is_context_enabled(context)
        && settings.is_permission_enabled(context, local_permission(permission))
}

#[cfg(test)]
pub(crate) fn capabilities() -> Vec<ActionKind> {
    ActionKind::implemented_metadata()
        .into_iter()
        .map(|metadata| metadata.kind)
        .collect()
}

fn local_invocation_context(context: InvocationContext) -> LocalControlInvocationContext {
    match context {
        InvocationContext::InsideWarp => LocalControlInvocationContext::InsideWarp,
        InvocationContext::OutsideWarp => LocalControlInvocationContext::OutsideWarp,
    }
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
    let context = local_invocation_context(context);
    if !settings.is_context_enabled(context) {
        return Err(ControlError::new(
            ErrorCode::LocalControlDisabled,
            "local control is disabled for this invocation context",
        ));
    }
    let permission = local_permission(action.metadata().permission_category);
    if !settings.is_permission_enabled(context, permission) {
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
