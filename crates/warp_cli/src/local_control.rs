use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum AppCommand {
    /// Check that a local Warp instance accepts authenticated control requests.
    Ping(TargetArgs),
    /// Print a small snapshot of live app state from a local Warp instance.
    Inspect(TargetArgs),
    /// Print local-control and app version metadata.
    Version(TargetArgs),
    /// Print the active-instance summary exposed by the local bridge.
    Active(TargetArgs),
    /// Open settings when the app bridge supports the mutation.
    SettingsOpen(TargetArgs),
    /// Open the command palette when the app bridge supports the mutation.
    CommandPaletteOpen(QueryTargetArgs),
    /// Open command search when the app bridge supports the mutation.
    CommandSearchOpen(QueryTargetArgs),
    /// Open Warp Drive when the app bridge supports the mutation.
    WarpDriveOpen(TargetArgs),
    /// Toggle Warp Drive when the app bridge supports the mutation.
    WarpDriveToggle(TargetArgs),
    /// Toggle Resource Center when the app bridge supports the mutation.
    ResourceCenterToggle(TargetArgs),
    /// Toggle the AI assistant panel when the app bridge supports the mutation.
    AiAssistantToggle(TargetArgs),
    /// Toggle the code review panel when the app bridge supports the mutation.
    CodeReviewToggle(TargetArgs),
    /// Toggle vertical tabs when the app bridge supports the mutation.
    VerticalTabsToggle(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum InstanceCommand {
    /// List locally discoverable Warp instances.
    List,
}

#[derive(Debug, Clone, Subcommand)]
pub enum WindowCommand {
    List(TargetArgs),
    Create(TargetArgs),
    Focus(TargetArgs),
    Close(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum TabCommand {
    List(TargetArgs),
    Create(TargetArgs),
    Activate(TargetArgs),
    Move(DirectionTargetArgs),
    Rename(TextTargetArgs),
    Close(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum PaneCommand {
    List(TargetArgs),
    Split(DirectionTargetArgs),
    Focus(TargetArgs),
    Navigate(DirectionTargetArgs),
    Close(TargetArgs),
    Maximize(TargetArgs),
    Resize(DirectionTargetArgs),
    SessionPrevious(TargetArgs),
    SessionNext(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum SessionCommand {
    List(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum InputCommand {
    Insert(TextTargetArgs),
    Replace(TextTargetArgs),
    Clear(TargetArgs),
    ModeSet(ModeTargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum ThemeCommand {
    List(TargetArgs),
    Set(TextTargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum FontSizeCommand {
    Increase(TargetArgs),
    Decrease(TargetArgs),
    Reset(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum ZoomCommand {
    Increase(TargetArgs),
    Decrease(TargetArgs),
    Reset(TargetArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum SettingCommand {
    List(TargetArgs),
    Get(SettingKeyArgs),
    Set(SettingValueArgs),
    Toggle(SettingKeyArgs),
}

#[derive(Debug, Clone, Args, Default)]
pub struct TargetArgs {
    /// Target a specific local Warp instance id from `warp instance list`.
    #[arg(long = "instance")]
    pub instance: Option<String>,

    /// Target a specific local Warp process id.
    #[arg(long = "pid", conflicts_with = "instance")]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Args)]
pub struct QueryTargetArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct DirectionTargetArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    #[arg(long)]
    pub direction: String,
}

#[derive(Debug, Clone, Args)]
pub struct ModeTargetArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    pub mode: String,
}

#[derive(Debug, Clone, Args)]
pub struct TextTargetArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    pub value: String,
}

#[derive(Debug, Clone, Args)]
pub struct SettingKeyArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    pub key: String,
}

#[derive(Debug, Clone, Args)]
pub struct SettingValueArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    pub key: String,
    pub value: String,
}
