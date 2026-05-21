pub mod auth;
pub mod client;
pub mod discovery;
pub mod protocol;
pub mod selection;

pub use auth::AuthToken;
pub use discovery::{
    ControlEndpoint, InstanceId, InstanceRecord, RegisteredInstance, discovery_dir,
};
pub use protocol::{
    Action, ActionKind, ControlError, ControlResponse, ErrorCode, ErrorResponseEnvelope,
    PROTOCOL_VERSION, PaneSelector, RequestEnvelope, ResponseEnvelope, TabSelector, TargetSelector,
    WindowSelector,
};
