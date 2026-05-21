use ::local_control::protocol::{Action, ActionKind, ControlResponse, RequestEnvelope};
use ::local_control::selection::{select_instance, InstanceSelector};
use anyhow::Context as _;
use serde::Serialize;
use serde_json::json;
use warp_cli::agent::OutputFormat;
use warp_cli::local_control::{
    AppCommand, DirectionTargetArgs, FontSizeCommand, InputCommand, InstanceCommand, PaneCommand,
    QueryTargetArgs, SessionCommand, SettingCommand, SettingKeyArgs, SettingValueArgs, TabCommand,
    TargetArgs, TextTargetArgs, ThemeCommand, WindowCommand, ZoomCommand,
};

use crate::ai::agent_sdk::output::{write_json, write_json_line};

#[derive(Serialize)]
struct InstanceSummary {
    instance_id: String,
    pid: u32,
    channel: String,
    app_id: String,
    app_version: Option<String>,
    started_at: String,
    endpoint: ::local_control::discovery::ControlEndpoint,
    capabilities: Vec<&'static str>,
}
pub fn run_window_command(
    command: WindowCommand,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match command {
        WindowCommand::List(args) => {
            run_action(args, ActionKind::WindowList, json!({}), output_format)
        }
        WindowCommand::Create(args) => {
            run_action(args, ActionKind::WindowCreate, json!({}), output_format)
        }
        WindowCommand::Focus(args) => {
            run_action(args, ActionKind::WindowFocus, json!({}), output_format)
        }
        WindowCommand::Close(args) => {
            run_action(args, ActionKind::WindowClose, json!({}), output_format)
        }
    }
}

pub fn run_tab_command(command: TabCommand, output_format: OutputFormat) -> anyhow::Result<()> {
    match command {
        TabCommand::List(args) => run_action(args, ActionKind::TabList, json!({}), output_format),
        TabCommand::Create(args) => {
            run_action(args, ActionKind::TabCreate, json!({}), output_format)
        }
        TabCommand::Activate(args) => {
            run_action(args, ActionKind::TabActivate, json!({}), output_format)
        }
        TabCommand::Move(args) => run_direction_action(args, ActionKind::TabMove, output_format),
        TabCommand::Rename(args) => run_text_action(args, ActionKind::TabRename, output_format),
        TabCommand::Close(args) => run_action(args, ActionKind::TabClose, json!({}), output_format),
    }
}

pub fn run_pane_command(command: PaneCommand, output_format: OutputFormat) -> anyhow::Result<()> {
    match command {
        PaneCommand::List(args) => run_action(args, ActionKind::PaneList, json!({}), output_format),
        PaneCommand::Split(args) => {
            run_direction_action(args, ActionKind::PaneSplit, output_format)
        }
        PaneCommand::Focus(args) => {
            run_action(args, ActionKind::PaneFocus, json!({}), output_format)
        }
        PaneCommand::Navigate(args) => {
            run_direction_action(args, ActionKind::PaneNavigate, output_format)
        }
        PaneCommand::Close(args) => {
            run_action(args, ActionKind::PaneClose, json!({}), output_format)
        }
        PaneCommand::Maximize(args) => {
            run_action(args, ActionKind::PaneMaximize, json!({}), output_format)
        }
        PaneCommand::Resize(args) => {
            run_direction_action(args, ActionKind::PaneResize, output_format)
        }
        PaneCommand::SessionPrevious(args) => run_action(
            args,
            ActionKind::PaneSessionPrevious,
            json!({}),
            output_format,
        ),
        PaneCommand::SessionNext(args) => {
            run_action(args, ActionKind::PaneSessionNext, json!({}), output_format)
        }
    }
}

pub fn run_session_command(
    command: SessionCommand,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match command {
        SessionCommand::List(args) => {
            run_action(args, ActionKind::SessionList, json!({}), output_format)
        }
    }
}

pub fn run_input_command(command: InputCommand, output_format: OutputFormat) -> anyhow::Result<()> {
    match command {
        InputCommand::Insert(args) => run_text_action(args, ActionKind::InputInsert, output_format),
        InputCommand::Replace(args) => {
            run_text_action(args, ActionKind::InputReplace, output_format)
        }
        InputCommand::Clear(args) => {
            run_action(args, ActionKind::InputClear, json!({}), output_format)
        }
        InputCommand::ModeSet(args) => run_action(
            args.target,
            ActionKind::InputModeSet,
            json!({ "mode": args.mode }),
            output_format,
        ),
    }
}

pub fn run_theme_command(command: ThemeCommand, output_format: OutputFormat) -> anyhow::Result<()> {
    match command {
        ThemeCommand::List(args) => {
            run_action(args, ActionKind::ThemeList, json!({}), output_format)
        }
        ThemeCommand::Set(args) => run_text_action(args, ActionKind::ThemeSet, output_format),
    }
}

pub fn run_font_size_command(
    command: FontSizeCommand,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let (args, operation) = match command {
        FontSizeCommand::Increase(args) => (args, "increase"),
        FontSizeCommand::Decrease(args) => (args, "decrease"),
        FontSizeCommand::Reset(args) => (args, "reset"),
    };
    run_action(
        args,
        ActionKind::AppearanceFontSize,
        json!({ "operation": operation }),
        output_format,
    )
}

pub fn run_zoom_command(command: ZoomCommand, output_format: OutputFormat) -> anyhow::Result<()> {
    let (args, operation) = match command {
        ZoomCommand::Increase(args) => (args, "increase"),
        ZoomCommand::Decrease(args) => (args, "decrease"),
        ZoomCommand::Reset(args) => (args, "reset"),
    };
    run_action(
        args,
        ActionKind::AppearanceZoom,
        json!({ "operation": operation }),
        output_format,
    )
}

pub fn run_setting_command(
    command: SettingCommand,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match command {
        SettingCommand::List(args) => {
            run_action(args, ActionKind::SettingList, json!({}), output_format)
        }
        SettingCommand::Get(args) => {
            run_setting_key_action(args, ActionKind::SettingGet, output_format)
        }
        SettingCommand::Set(args) => {
            run_setting_value_action(args, ActionKind::SettingSet, output_format)
        }
        SettingCommand::Toggle(args) => {
            run_setting_key_action(args, ActionKind::SettingToggle, output_format)
        }
    }
}

fn run_query_action(
    args: QueryTargetArgs,
    action: ActionKind,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    run_action(
        args.target,
        action,
        json!({ "query": args.query }),
        output_format,
    )
}

fn run_direction_action(
    args: DirectionTargetArgs,
    action: ActionKind,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    run_action(
        args.target,
        action,
        json!({ "direction": args.direction }),
        output_format,
    )
}

fn run_text_action(
    args: TextTargetArgs,
    action: ActionKind,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    run_action(
        args.target,
        action,
        json!({ "value": args.value }),
        output_format,
    )
}

fn run_setting_key_action(
    args: SettingKeyArgs,
    action: ActionKind,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    run_action(
        args.target,
        action,
        json!({ "key": args.key }),
        output_format,
    )
}

fn run_setting_value_action(
    args: SettingValueArgs,
    action: ActionKind,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    run_action(
        args.target,
        action,
        json!({ "key": args.key, "value": args.value }),
        output_format,
    )
}

impl From<::local_control::discovery::InstanceRecord> for InstanceSummary {
    fn from(record: ::local_control::discovery::InstanceRecord) -> Self {
        Self {
            instance_id: record.instance_id.0,
            pid: record.pid,
            channel: record.channel,
            app_id: record.app_id,
            app_version: record.app_version,
            started_at: record.started_at.to_rfc3339(),
            endpoint: record.endpoint,
            capabilities: record
                .capabilities
                .into_iter()
                .map(ActionKind::as_str)
                .collect(),
        }
    }
}

pub fn run_app_command(command: AppCommand, output_format: OutputFormat) -> anyhow::Result<()> {
    match command {
        AppCommand::Ping(args) => run_action(args, ActionKind::AppPing, json!({}), output_format),
        AppCommand::Inspect(args) => {
            run_action(args, ActionKind::AppInspect, json!({}), output_format)
        }
        AppCommand::Version(args) => {
            run_action(args, ActionKind::AppVersion, json!({}), output_format)
        }
        AppCommand::Active(args) => {
            run_action(args, ActionKind::AppActive, json!({}), output_format)
        }
        AppCommand::SettingsOpen(args) => {
            run_action(args, ActionKind::AppSettingsOpen, json!({}), output_format)
        }
        AppCommand::CommandPaletteOpen(args) => {
            run_query_action(args, ActionKind::AppCommandPaletteOpen, output_format)
        }
        AppCommand::CommandSearchOpen(args) => {
            run_query_action(args, ActionKind::AppCommandSearchOpen, output_format)
        }
        AppCommand::WarpDriveOpen(args) => {
            run_action(args, ActionKind::AppWarpDriveOpen, json!({}), output_format)
        }
        AppCommand::WarpDriveToggle(args) => run_action(
            args,
            ActionKind::AppWarpDriveToggle,
            json!({}),
            output_format,
        ),
        AppCommand::ResourceCenterToggle(args) => run_action(
            args,
            ActionKind::AppResourceCenterToggle,
            json!({}),
            output_format,
        ),
        AppCommand::AiAssistantToggle(args) => run_action(
            args,
            ActionKind::AppAiAssistantToggle,
            json!({}),
            output_format,
        ),
        AppCommand::CodeReviewToggle(args) => run_action(
            args,
            ActionKind::AppCodeReviewToggle,
            json!({}),
            output_format,
        ),
        AppCommand::VerticalTabsToggle(args) => run_action(
            args,
            ActionKind::AppVerticalTabsToggle,
            json!({}),
            output_format,
        ),
    }
}

pub fn run_instance_command(
    command: InstanceCommand,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match command {
        InstanceCommand::List => {
            let summaries = ::local_control::discovery::list_instances()
                .into_iter()
                .map(InstanceSummary::from)
                .collect::<Vec<_>>();
            match output_format {
                OutputFormat::Json => write_json(&summaries, std::io::stdout()),
                OutputFormat::Ndjson => {
                    for summary in summaries {
                        write_json_line(&summary, std::io::stdout())?;
                    }
                    Ok(())
                }
                OutputFormat::Pretty | OutputFormat::Text => {
                    for summary in summaries {
                        println!(
                            "{}\tpid={}\t{}\t{}:{}",
                            summary.instance_id,
                            summary.pid,
                            summary.channel,
                            summary.endpoint.host,
                            summary.endpoint.port
                        );
                    }
                    Ok(())
                }
            }
        }
    }
}

fn run_action(
    args: TargetArgs,
    action: ActionKind,
    params: serde_json::Value,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let records = ::local_control::discovery::list_instances();
    let selector = instance_selector(args);
    let instance = select_instance(&records, &selector)?;
    let request = RequestEnvelope::new(Action {
        kind: action,
        params,
    });
    let response = ::local_control::client::send_request(&instance, &request)?;
    let ControlResponse::Ok { data } = response.response else {
        anyhow::bail!("local-control request failed without an error payload");
    };
    match output_format {
        OutputFormat::Json => write_json(&data, std::io::stdout()),
        OutputFormat::Ndjson => write_json_line(&data, std::io::stdout()),
        OutputFormat::Pretty | OutputFormat::Text => {
            if action == ActionKind::AppPing {
                println!("ok");
                Ok(())
            } else {
                write_json(&data, std::io::stdout()).context("unable to print local-control data")
            }
        }
    }
}

fn instance_selector(args: TargetArgs) -> InstanceSelector {
    if let Some(instance_id) = args.instance {
        return InstanceSelector::Id(::local_control::discovery::InstanceId(instance_id));
    }
    if let Some(pid) = args.pid {
        return InstanceSelector::Pid(pid);
    }
    InstanceSelector::Active
}
