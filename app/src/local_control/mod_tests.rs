use ::local_control::auth::CredentialGrant;
use ::local_control::protocol::ActionKind;
use ::local_control::protocol::{
    Action, ControlResponse, PaneSelector, PaneTarget, SessionSelector, SessionTarget, TabSelector,
    TabTarget, TargetSelector, WindowSelector, WindowTarget,
};
use ::local_control::{
    ErrorCode, InputRunParams, InstanceId, InvocationContext, PermissionCategory, RequestEnvelope,
};
use chrono::Duration;
use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warpui::{App, SingletonEntity};

use super::{
    action_metadata_for_name, allow_input_run_policy_for_test, appearance_state_result,
    capabilities, ensure_feature_enabled, ensure_input_run_policy_allows,
    ensure_settings_allow_action, outside_warp_action_enabled_for_settings, rejected_setting_key,
    require_active_window_id, setting_get_result, setting_list_result, theme_list_result,
    validate_action_params, validate_tab_create_target, LocalControlBridge,
};
use crate::auth::AuthStateProvider;
use crate::settings::{
    AllowOutsideWarpAppStateMutations, AllowOutsideWarpControl,
    AllowOutsideWarpMetadataConfigurationMutations, AllowOutsideWarpMetadataReads,
    AllowOutsideWarpUnderlyingDataMutations, AllowOutsideWarpUnderlyingDataReads,
    LocalControlSettings,
};
use crate::test_util::settings::initialize_settings_for_tests;
fn settings_with_values(
    outside_control: bool,
    outside_metadata_reads: bool,
    outside_underlying_data_reads: bool,
    outside_app_state_mutations: bool,
    outside_metadata_configuration_mutations: bool,
    outside_underlying_data_mutations: bool,
) -> LocalControlSettings {
    LocalControlSettings {
        allow_outside_warp_control: AllowOutsideWarpControl::new(Some(outside_control)),
        allow_outside_warp_metadata_reads: AllowOutsideWarpMetadataReads::new(Some(
            outside_metadata_reads,
        )),
        allow_outside_warp_underlying_data_reads: AllowOutsideWarpUnderlyingDataReads::new(Some(
            outside_underlying_data_reads,
        )),
        allow_outside_warp_app_state_mutations: AllowOutsideWarpAppStateMutations::new(Some(
            outside_app_state_mutations,
        )),
        allow_outside_warp_metadata_configuration_mutations:
            AllowOutsideWarpMetadataConfigurationMutations::new(Some(
                outside_metadata_configuration_mutations,
            )),
        allow_outside_warp_underlying_data_mutations: AllowOutsideWarpUnderlyingDataMutations::new(
            Some(outside_underlying_data_mutations),
        ),
    }
}

fn settings_with_outside_warp(
    outside_control: bool,
    outside_app_state_mutations: bool,
) -> LocalControlSettings {
    settings_with_values(
        outside_control,
        false,
        false,
        outside_app_state_mutations,
        false,
        false,
    )
}

fn enable_outside_warp_metadata_reads(app: &mut App) {
    app.update(|ctx| {
        LocalControlSettings::handle(ctx).update(ctx, |settings, ctx| {
            let _ = settings.allow_outside_warp_control.set_value(true, ctx);
            let _ = settings
                .allow_outside_warp_metadata_reads
                .set_value(true, ctx);
        });
    });
}
fn grant_for(action: ActionKind) -> CredentialGrant {
    CredentialGrant::new(
        InstanceId("test-instance".to_owned()),
        action,
        InvocationContext::OutsideWarp,
        Duration::minutes(5),
    )
}

fn request_with_target(action: ActionKind, target: TargetSelector) -> RequestEnvelope {
    let mut request = RequestEnvelope::new(Action::new(action));
    request.target = target;
    request
}

fn response_error_code(response: ::local_control::ResponseEnvelope) -> ErrorCode {
    match response.response {
        ControlResponse::Error { error } => error.code,
        ControlResponse::Ok { data } => panic!("expected error response, got {data:?}"),
    }
}

#[test]
fn tab_create_accepts_default_and_active_targets() {
    validate_tab_create_target(&TargetSelector::default()).expect("default target is accepted");

    validate_tab_create_target(&TargetSelector {
        window: Some(WindowTarget::Active),
        tab: Some(TabTarget::Active),
        pane: Some(PaneTarget::Active),
        session: Some(SessionTarget::Active),
    })
    .expect("active target is accepted");
}

#[test]
fn tab_create_rejects_concrete_targets() {
    let err = validate_tab_create_target(&TargetSelector {
        window: Some(WindowTarget::Id {
            id: WindowSelector("window".to_owned()),
        }),
        ..TargetSelector::default()
    })
    .expect_err("concrete window target is rejected");
    assert_eq!(err.code, ErrorCode::StaleTarget);

    let err = validate_tab_create_target(&TargetSelector {
        tab: Some(TabTarget::Id {
            id: TabSelector("tab".to_owned()),
        }),
        ..TargetSelector::default()
    })
    .expect_err("concrete tab target is rejected");
    assert_eq!(err.code, ErrorCode::StaleTarget);

    let err = validate_tab_create_target(&TargetSelector {
        pane: Some(PaneTarget::Id {
            id: PaneSelector("pane".to_owned()),
        }),
        ..TargetSelector::default()
    })
    .expect_err("concrete pane target is rejected");
    assert_eq!(err.code, ErrorCode::StaleTarget);

    let err = validate_tab_create_target(&TargetSelector {
        session: Some(SessionTarget::Id {
            id: SessionSelector("session".to_owned()),
        }),
        ..TargetSelector::default()
    })
    .expect_err("concrete session target is rejected");
    assert_eq!(err.code, ErrorCode::StaleTarget);
}

#[test]
fn tab_create_rejects_unsupported_selector_forms() {
    let err = validate_tab_create_target(&TargetSelector {
        window: Some(WindowTarget::Index { index: 0 }),
        ..TargetSelector::default()
    })
    .expect_err("indexed window target is rejected");
    assert_eq!(err.code, ErrorCode::InvalidSelector);

    let err = validate_tab_create_target(&TargetSelector {
        tab: Some(TabTarget::Index { index: 0 }),
        ..TargetSelector::default()
    })
    .expect_err("indexed tab target is rejected");
    assert_eq!(err.code, ErrorCode::InvalidSelector);
}

#[test]
fn capabilities_advertises_core_and_metadata_slice_actions() {
    assert_eq!(
        capabilities(),
        vec![
            ActionKind::InstanceList,
            ActionKind::AppPing,
            ActionKind::AppInspect,
            ActionKind::AppVersion,
            ActionKind::AppActive,
            ActionKind::ActionList,
            ActionKind::ActionGet,
            ActionKind::WindowList,
            ActionKind::TabList,
            ActionKind::TabCreate,
            ActionKind::PaneList,
            ActionKind::SessionList,
            ActionKind::BlockList,
            ActionKind::BlockGet,
            ActionKind::InputGet,
            ActionKind::InputRun,
            ActionKind::HistoryList,
            ActionKind::ThemeList,
            ActionKind::AppearanceGet,
            ActionKind::SettingGet,
            ActionKind::SettingList,
        ]
    );
}

#[test]
fn outside_warp_discovery_requires_context_and_action_permission() {
    assert!(!outside_warp_action_enabled_for_settings(
        &settings_with_outside_warp(false, true),
        ActionKind::TabCreate
    ));
    assert!(!outside_warp_action_enabled_for_settings(
        &settings_with_outside_warp(true, false),
        ActionKind::TabCreate
    ));
    assert!(outside_warp_action_enabled_for_settings(
        &settings_with_outside_warp(true, true),
        ActionKind::TabCreate
    ));
    assert!(!outside_warp_action_enabled_for_settings(
        &settings_with_values(true, false, false, true, false, false),
        ActionKind::WindowList
    ));
    assert!(outside_warp_action_enabled_for_settings(
        &settings_with_values(true, true, false, false, false, false),
        ActionKind::WindowList
    ));
}

#[test]
fn tab_create_requires_active_window() {
    let active = warpui::WindowId::from_usize(1);

    assert_eq!(
        require_active_window_id(Some(active)).expect("active"),
        active
    );
    let err = require_active_window_id(None).expect_err("missing active window");
    assert_eq!(err.code, ErrorCode::MissingTarget);
}

#[test]
fn feature_flag_disabled_denies_local_control() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(false);
    let err = ensure_feature_enabled().expect_err("feature flag disabled");
    assert_eq!(err.code, ErrorCode::LocalControlDisabled);
}

#[test]
fn disabled_outside_warp_denies_before_granular_permission() {
    let settings = settings_with_values(false, true, false, true, false, false);

    let err = ensure_settings_allow_action(
        &settings,
        InvocationContext::OutsideWarp,
        ActionKind::TabCreate,
    )
    .expect_err("outside-Warp parent context is disabled");
    assert_eq!(err.code, ErrorCode::LocalControlDisabled);
}

#[test]
fn inside_warp_context_is_not_implemented() {
    let settings = settings_with_values(true, true, false, true, false, false);

    let err = ensure_settings_allow_action(
        &settings,
        InvocationContext::InsideWarp,
        ActionKind::TabCreate,
    )
    .expect_err("inside-Warp grants are not implemented");
    assert_eq!(err.code, ErrorCode::ExecutionContextNotAllowed);
}

#[test]
fn disabled_granular_permission_denies_with_insufficient_permissions() {
    let settings = settings_with_values(true, true, false, false, false, false);

    let err = ensure_settings_allow_action(
        &settings,
        InvocationContext::OutsideWarp,
        ActionKind::TabCreate,
    )
    .expect_err("read-write permission is disabled");
    assert_eq!(err.code, ErrorCode::InsufficientPermissions);
}

#[test]
fn tab_create_rejects_malformed_params() {
    let err = validate_action_params(&Action {
        kind: ActionKind::TabCreate,
        params: serde_json::json!({ "unexpected": true }),
    })
    .expect_err("tab.create params must be empty");
    assert_eq!(err.code, ErrorCode::InvalidParams);

    validate_action_params(&Action {
        kind: ActionKind::TabCreate,
        params: serde_json::json!({}),
    })
    .expect("empty tab.create params are accepted");
}

#[test]
fn metadata_handlers_return_successful_empty_metadata_without_windows() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        enable_outside_warp_metadata_reads(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        for action in [
            ActionKind::AppActive,
            ActionKind::AppInspect,
            ActionKind::AppVersion,
            ActionKind::ActionList,
            ActionKind::WindowList,
            ActionKind::TabList,
            ActionKind::PaneList,
            ActionKind::SessionList,
        ] {
            let response = bridge.update(&mut app, |bridge, ctx| {
                bridge.handle_request(
                    RequestEnvelope::new(Action::new(action)),
                    grant_for(action),
                    ctx,
                )
            });
            match response.response {
                ControlResponse::Ok { data } => {
                    assert_eq!(data["action"], action.as_str());
                }
                ControlResponse::Error { error } => {
                    panic!("{} returned {error}", action.as_str());
                }
            }
        }
    });
}

#[test]
fn metadata_list_handlers_reject_stale_and_unsupported_selectors() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        enable_outside_warp_metadata_reads(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        let cases = [
            (
                ActionKind::WindowList,
                TargetSelector {
                    tab: Some(TabTarget::Active),
                    ..TargetSelector::default()
                },
                ErrorCode::InvalidSelector,
            ),
            (
                ActionKind::WindowList,
                TargetSelector {
                    window: Some(WindowTarget::Id {
                        id: WindowSelector("stale-window".to_owned()),
                    }),
                    ..TargetSelector::default()
                },
                ErrorCode::StaleTarget,
            ),
            (
                ActionKind::TabList,
                TargetSelector {
                    tab: Some(TabTarget::Title {
                        title: "unsupported".to_owned(),
                    }),
                    ..TargetSelector::default()
                },
                ErrorCode::InvalidSelector,
            ),
            (
                ActionKind::PaneList,
                TargetSelector {
                    pane: Some(PaneTarget::Id {
                        id: PaneSelector("stale-pane".to_owned()),
                    }),
                    ..TargetSelector::default()
                },
                ErrorCode::StaleTarget,
            ),
            (
                ActionKind::SessionList,
                TargetSelector {
                    session: Some(SessionTarget::Id {
                        id: SessionSelector("stale-session".to_owned()),
                    }),
                    ..TargetSelector::default()
                },
                ErrorCode::StaleTarget,
            ),
        ];

        for (action, target, code) in cases {
            let response = bridge.update(&mut app, |bridge, ctx| {
                bridge.handle_request(request_with_target(action, target), grant_for(action), ctx)
            });
            assert_eq!(response_error_code(response), code);
        }
    });
}

#[test]
fn metadata_actions_require_metadata_permission_not_app_state_mutation_permission() {
    let metadata_without_mutation = settings_with_values(true, true, false, false, false, false);
    let mutation_without_metadata = settings_with_values(true, false, false, true, false, false);

    for action in [
        ActionKind::InstanceList,
        ActionKind::AppPing,
        ActionKind::AppInspect,
        ActionKind::AppVersion,
        ActionKind::AppActive,
        ActionKind::ActionList,
        ActionKind::ActionGet,
        ActionKind::WindowList,
        ActionKind::TabList,
        ActionKind::PaneList,
        ActionKind::SessionList,
        ActionKind::ThemeList,
        ActionKind::AppearanceGet,
        ActionKind::SettingGet,
        ActionKind::SettingList,
    ] {
        assert_eq!(
            action.metadata().permission_category,
            PermissionCategory::ReadMetadata
        );
        ensure_settings_allow_action(
            &metadata_without_mutation,
            InvocationContext::OutsideWarp,
            action,
        )
        .expect("metadata read permission allows metadata action");
        let err = ensure_settings_allow_action(
            &mutation_without_metadata,
            InvocationContext::OutsideWarp,
            action,
        )
        .expect_err("metadata action is denied without metadata read permission");
        assert_eq!(err.code, ErrorCode::InsufficientPermissions);
    }

    assert_eq!(
        ActionKind::TabCreate.metadata().permission_category,
        PermissionCategory::MutateAppState
    );
    ensure_settings_allow_action(
        &mutation_without_metadata,
        InvocationContext::OutsideWarp,
        ActionKind::TabCreate,
    )
    .expect("app-state mutation permission allows tab.create");
}

#[test]
fn data_actions_require_underlying_data_permission_not_metadata_permission() {
    let underlying_data_without_metadata =
        settings_with_values(true, false, true, false, false, false);
    let metadata_without_underlying_data =
        settings_with_values(true, true, false, false, false, false);

    for action in [
        ActionKind::BlockList,
        ActionKind::BlockGet,
        ActionKind::InputGet,
        ActionKind::HistoryList,
    ] {
        assert_eq!(
            action.metadata().permission_category,
            PermissionCategory::ReadUnderlyingData
        );
        ensure_settings_allow_action(
            &underlying_data_without_metadata,
            InvocationContext::OutsideWarp,
            action,
        )
        .expect("underlying data read permission allows data action");
        let err = ensure_settings_allow_action(
            &metadata_without_underlying_data,
            InvocationContext::OutsideWarp,
            action,
        )
        .expect_err("data action is denied without underlying data read permission");
        assert_eq!(err.code, ErrorCode::InsufficientPermissions);
    }
}

#[test]
fn action_get_rejects_unallowlisted_action_names() {
    let err = validate_action_params(&Action {
        kind: ActionKind::ActionGet,
        params: serde_json::json!({ "action": "shell.exec" }),
    })
    .expect_err("unallowlisted action is rejected");
    assert_eq!(err.code, ErrorCode::NotAllowlisted);

    validate_action_params(&Action {
        kind: ActionKind::ActionGet,
        params: serde_json::json!({ "action": "input.run" }),
    })
    .expect("input.run is an allowlisted action");
}

#[test]
fn action_metadata_lookup_reports_stub_status_for_allowlisted_future_actions() {
    let metadata = action_metadata_for_name("window.create").expect("allowlisted action");

    assert_eq!(metadata.kind, ActionKind::WindowCreate);
    assert_eq!(
        metadata.implementation_status,
        ::local_control::ActionImplementationStatus::Stub
    );
}

#[test]
fn app_target_metadata_reads_reject_malformed_params() {
    for action in [
        ActionKind::AppVersion,
        ActionKind::AppActive,
        ActionKind::AppInspect,
        ActionKind::ActionList,
        ActionKind::WindowList,
        ActionKind::TabList,
        ActionKind::PaneList,
        ActionKind::SessionList,
        ActionKind::ThemeList,
        ActionKind::AppearanceGet,
        ActionKind::SettingList,
    ] {
        let err = validate_action_params(&Action {
            kind: action,
            params: serde_json::json!({ "unexpected": true }),
        })
        .expect_err("app target metadata read params must be empty");
        assert_eq!(err.code, ErrorCode::InvalidParams);

        validate_action_params(&Action {
            kind: action,
            params: serde_json::json!({}),
        })
        .expect("empty app target metadata read params are accepted");
    }

    validate_action_params(&Action {
        kind: ActionKind::SettingGet,
        params: serde_json::json!({ "key": "appearance.themes.theme" }),
    })
    .expect("setting.get accepts a key parameter");
}

#[test]
fn settings_and_appearance_handlers_return_allowlisted_metadata() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        bridge.update(&mut app, |_, ctx| {
            let themes = theme_list_result(ctx).expect("themes are listed");
            assert!(themes.themes.iter().any(|theme| theme.name == "Dark"));

            let appearance = appearance_state_result(ctx).expect("appearance is readable");
            assert_eq!(appearance.theme.as_deref(), Some("Dark"));
            assert_eq!(appearance.light_theme.as_deref(), Some("Light"));
            assert_eq!(appearance.dark_theme.as_deref(), Some("Dark"));
            assert_eq!(appearance.ui_zoom_percent, Some(100));

            let settings = setting_list_result(ctx).expect("settings are listed");
            assert!(settings
                .settings
                .iter()
                .any(|setting| setting.key == "appearance.themes.system_theme"));

            let setting = setting_get_result("appearance.themes.system_theme", ctx)
                .expect("allowlisted setting is readable");
            assert_eq!(setting.setting.value, serde_json::json!(false));
            assert_eq!(setting.setting.value_type, "bool");
        });
    });
}

#[test]
fn setting_get_rejects_unknown_and_private_settings() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        bridge.update(&mut app, |_, ctx| {
            let err = setting_get_result("appearance.secrets.token", ctx)
                .expect_err("unknown settings are rejected");
            assert_eq!(err.code, ErrorCode::NotAllowlisted);

            let err = setting_get_result("local_control.allow_outside_warp_control", ctx)
                .expect_err("private settings are rejected");
            assert_eq!(err.code, ErrorCode::NotAllowlisted);
            assert!(err.message.contains("private or sensitive"));
        });
    });
}

#[test]
fn rejected_setting_key_distinguishes_private_settings() {
    let private_err = rejected_setting_key("terminal.input.inline_menu_custom_content_heights");
    assert_eq!(private_err.code, ErrorCode::NotAllowlisted);
    assert!(private_err.message.contains("private or sensitive"));

    let unknown_err = rejected_setting_key("terminal.input.not_real");
    assert_eq!(unknown_err.code, ErrorCode::NotAllowlisted);
    assert!(unknown_err.message.contains("not an allowlisted"));
}

#[test]
fn settings_and_appearance_bridge_handlers_return_success() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        enable_outside_warp_metadata_reads(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        for action in [
            ActionKind::ThemeList,
            ActionKind::AppearanceGet,
            ActionKind::SettingList,
        ] {
            let response = bridge.update(&mut app, |bridge, ctx| {
                bridge.handle_request(
                    RequestEnvelope::new(Action::new(action)),
                    grant_for(action),
                    ctx,
                )
            });
            match response.response {
                ControlResponse::Ok { data } => assert!(data.is_object()),
                ControlResponse::Error { error } => {
                    panic!("{} returned {error}", action.as_str());
                }
            }
        }

        let action = Action::with_params(
            ActionKind::SettingGet,
            ::local_control::SettingGetParams {
                key: "appearance.themes.system_theme".to_owned(),
            },
        )
        .expect("setting.get params serialize");
        let response = bridge.update(&mut app, |bridge, ctx| {
            bridge.handle_request(
                RequestEnvelope::new(action),
                grant_for(ActionKind::SettingGet),
                ctx,
            )
        });
        match response.response {
            ControlResponse::Ok { data } => {
                assert_eq!(data["setting"]["key"], "appearance.themes.system_theme");
            }
            ControlResponse::Error { error } => {
                panic!("setting.get returned {error}");
            }
        }
    });
}

fn enable_outside_warp_underlying_data_mutations(app: &mut App) {
    app.update(|ctx| {
        LocalControlSettings::handle(ctx).update(ctx, |settings, ctx| {
            let _ = settings.allow_outside_warp_control.set_value(true, ctx);
            let _ = settings
                .allow_outside_warp_underlying_data_mutations
                .set_value(true, ctx);
        });
    });
}

fn authenticated_input_run_grant() -> CredentialGrant {
    let mut grant = CredentialGrant::new(
        InstanceId("test-instance".to_owned()),
        ActionKind::InputRun,
        InvocationContext::OutsideWarp,
        Duration::minutes(5),
    );
    grant.authenticated_user.subject = Some("test_user_uid".to_owned());
    grant
}

#[test]
fn data_reads_reject_malformed_params() {
    validate_action_params(&Action {
        kind: ActionKind::InputGet,
        params: serde_json::json!({}),
    })
    .expect("input.get accepts empty params");

    let err = validate_action_params(&Action {
        kind: ActionKind::InputGet,
        params: serde_json::json!({ "unexpected": true }),
    })
    .expect_err("input.get params must be empty");
    assert_eq!(err.code, ErrorCode::InvalidParams);

    validate_action_params(&Action {
        kind: ActionKind::BlockList,
        params: serde_json::json!({ "limit": 10 }),
    })
    .expect("block.list accepts limit");

    validate_action_params(&Action {
        kind: ActionKind::HistoryList,
        params: serde_json::json!({ "limit": 20 }),
    })
    .expect("history.list accepts limit");

    let err = validate_action_params(&Action {
        kind: ActionKind::BlockGet,
        params: serde_json::json!({ "block_id": "" }),
    })
    .expect_err("block.get requires a block id");
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn input_run_rejects_empty_command_params() {
    let err = validate_action_params(
        &Action::with_params(
            ActionKind::InputRun,
            InputRunParams {
                command: "  \t  ".to_owned(),
            },
        )
        .expect("input.run params serialize"),
    )
    .expect_err("whitespace-only command is rejected");
    assert_eq!(err.code, ErrorCode::InvalidParams);

    validate_action_params(
        &Action::with_params(
            ActionKind::InputRun,
            InputRunParams {
                command: "cargo check".to_owned(),
            },
        )
        .expect("input.run params serialize"),
    )
    .expect("non-empty command is accepted");
}

#[test]
fn mutating_underlying_data_permission_separates_from_app_state() {
    assert_eq!(
        ActionKind::InputRun.metadata().permission_category,
        PermissionCategory::MutateUnderlyingData
    );
    assert_ne!(
        ActionKind::InputRun.metadata().permission_category,
        PermissionCategory::MutateAppState
    );

    let mut tampered_grant = CredentialGrant::new(
        InstanceId("test-instance".to_owned()),
        ActionKind::InputRun,
        InvocationContext::OutsideWarp,
        Duration::minutes(5),
    );
    tampered_grant.permission_category = PermissionCategory::MutateAppState;
    tampered_grant.authenticated_user.subject = Some("test_user_uid".to_owned());

    let err = tampered_grant
        .verify_for_action(ActionKind::InputRun)
        .expect_err("app-state mutation category does not satisfy command execution");
    assert_eq!(err.code, ErrorCode::InsufficientPermissions);
}

#[test]
fn input_run_policy_gate_fails_closed_and_allows_test_override() {
    let action = Action::with_params(
        ActionKind::InputRun,
        InputRunParams {
            command: "echo hi".to_owned(),
        },
    )
    .expect("input.run params serialize");
    let grant = authenticated_input_run_grant();

    let err = ensure_input_run_policy_allows(&grant, &action)
        .expect_err("input.run policy fails closed by default");
    assert_eq!(err.code, ErrorCode::InsufficientPermissions);

    let guard = allow_input_run_policy_for_test();
    ensure_input_run_policy_allows(&grant, &action).expect("test policy override allows input.run");
    drop(guard);

    let err = ensure_input_run_policy_allows(&grant, &action)
        .expect_err("policy reverts to closed after guard drops");
    assert_eq!(err.code, ErrorCode::InsufficientPermissions);
}

#[test]
fn input_run_denials_happen_before_selector_resolution() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        enable_outside_warp_underlying_data_mutations(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        let stale_request = RequestEnvelope {
            target: TargetSelector {
                window: Some(WindowTarget::Id {
                    id: WindowSelector("stale-window".to_owned()),
                }),
                ..TargetSelector::default()
            },
            ..RequestEnvelope::new(
                Action::with_params(
                    ActionKind::InputRun,
                    InputRunParams {
                        command: "echo hi".to_owned(),
                    },
                )
                .expect("input.run params serialize"),
            )
        };

        bridge.update(&mut app, |bridge, ctx| {
            let mut wrong_perm_grant = authenticated_input_run_grant();
            wrong_perm_grant.permission_category = PermissionCategory::MutateAppState;
            let response = bridge.handle_request(stale_request.clone(), wrong_perm_grant, ctx);
            assert_eq!(
                response_error_code(response),
                ErrorCode::InsufficientPermissions,
                "tampered permission category is rejected before selector resolution"
            );

            let mut spoofed_grant = authenticated_input_run_grant();
            spoofed_grant.authenticated_user.subject = Some("spoofed-uid".to_owned());
            let response = bridge.handle_request(stale_request.clone(), spoofed_grant, ctx);
            assert_eq!(
                response_error_code(response),
                ErrorCode::AuthenticatedUserMismatch,
                "spoofed auth subject is rejected before selector resolution"
            );

            let valid_grant = authenticated_input_run_grant();
            let response = bridge.handle_request(stale_request.clone(), valid_grant, ctx);
            assert_eq!(
                response_error_code(response),
                ErrorCode::InsufficientPermissions,
                "policy gate blocks execution before selector resolution"
            );
        });
    });
}

#[test]
fn input_run_reaches_target_resolution_only_with_explicit_policy_gate() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        enable_outside_warp_underlying_data_mutations(&mut app);
        let bridge = app.add_model(LocalControlBridge::new);

        let request = RequestEnvelope::new(
            Action::with_params(
                ActionKind::InputRun,
                InputRunParams {
                    command: "echo hi".to_owned(),
                },
            )
            .expect("input.run params serialize"),
        );

        bridge.update(&mut app, |bridge, ctx| {
            let guard = allow_input_run_policy_for_test();
            let response =
                bridge.handle_request(request, authenticated_input_run_grant(), ctx);
            assert_eq!(
                response_error_code(response),
                ErrorCode::MissingTarget,
                "with policy gate open, execution reaches target resolution and fails on missing window"
            );
            drop(guard);
        });
    });
}

#[test]
fn accepted_command_and_agent_prompt_submission_remain_unavailable() {
    let excluded = [
        "accepted_command.submit",
        "agent.prompt.submit",
        "input.agent_prompt",
        "shell.exec",
    ];
    for name in excluded {
        assert!(
            ActionKind::ALL.iter().all(|kind| kind.as_str() != name),
            "{name} must not be an allowlisted local-control action"
        );
    }
}
