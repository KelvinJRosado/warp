use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AuthToken;
use crate::protocol::{ActionKind, ControlError, ErrorCode, PROTOCOL_VERSION};

const DISCOVERY_DIR_ENV: &str = "WARP_LOCAL_CONTROL_DISCOVERY_DIR";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(pub String);

impl InstanceId {
    pub fn new() -> Self {
        Self(format!("inst_{}", uuid::Uuid::new_v4().simple()))
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEndpoint {
    pub host: String,
    pub port: u16,
}

impl ControlEndpoint {
    pub fn localhost(port: u16) -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}/v1/control", self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub protocol_version: u32,
    pub instance_id: InstanceId,
    pub pid: u32,
    pub channel: String,
    pub app_id: String,
    pub app_version: Option<String>,
    pub started_at: DateTime<Utc>,
    pub executable_path: Option<PathBuf>,
    pub endpoint: ControlEndpoint,
    pub auth_token: String,
    pub capabilities: Vec<ActionKind>,
}

impl InstanceRecord {
    pub fn for_current_process(
        endpoint: ControlEndpoint,
        auth_token: &AuthToken,
        channel: impl Into<String>,
        app_id: impl Into<String>,
        app_version: Option<String>,
        capabilities: Vec<ActionKind>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            instance_id: InstanceId::new(),
            pid: std::process::id(),
            channel: channel.into(),
            app_id: app_id.into(),
            app_version,
            started_at: Utc::now(),
            executable_path: std::env::current_exe().ok(),
            endpoint,
            auth_token: auth_token.secret().to_owned(),
            capabilities,
        }
    }

    pub fn auth(&self) -> AuthToken {
        AuthToken::from_secret(self.auth_token.clone())
    }
}

pub struct RegisteredInstance {
    record: InstanceRecord,
    path: PathBuf,
}

impl RegisteredInstance {
    pub fn register(record: InstanceRecord) -> Result<Self, ControlError> {
        let dir = discovery_dir();
        fs::create_dir_all(&dir).map_err(|err| {
            ControlError::with_details(
                ErrorCode::Internal,
                "failed to create local-control discovery directory",
                err.to_string(),
            )
        })?;
        let path = record_path(&dir, &record.instance_id);
        let bytes = serde_json::to_vec_pretty(&record).map_err(|err| {
            ControlError::with_details(
                ErrorCode::Internal,
                "failed to serialize local-control discovery record",
                err.to_string(),
            )
        })?;
        fs::write(&path, bytes).map_err(|err| {
            ControlError::with_details(
                ErrorCode::Internal,
                "failed to write local-control discovery record",
                err.to_string(),
            )
        })?;
        set_private_permissions(&path);
        Ok(Self { record, path })
    }

    pub fn record(&self) -> &InstanceRecord {
        &self.record
    }
}

impl Drop for RegisteredInstance {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn discovery_dir() -> PathBuf {
    if let Some(path) = std::env::var_os(DISCOVERY_DIR_ENV) {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(path).join("warp").join("local-control");
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".warp").join("local-control")
}

pub fn list_instances() -> Vec<InstanceRecord> {
    list_instances_from_dir(&discovery_dir())
}

pub fn list_instances_from_dir(dir: &Path) -> Vec<InstanceRecord> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut records = entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|contents| serde_json::from_str::<InstanceRecord>(&contents).ok())
        .filter(|record| record.protocol_version == PROTOCOL_VERSION)
        .collect::<Vec<_>>();
    records.sort_by_key(|record| record.started_at);
    records
}

fn record_path(dir: &Path, instance_id: &InstanceId) -> PathBuf {
    dir.join(format!("{}.json", instance_id.0))
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_instance_round_trips_discovery_record() {
        let dir = tempfile::tempdir().expect("temp dir");
        let token = AuthToken::from_secret("token");
        let record = InstanceRecord::for_current_process(
            ControlEndpoint::localhost(4000),
            &token,
            "local",
            "dev.warp.WarpLocal",
            Some("test".to_owned()),
            vec![ActionKind::AppPing],
        );
        let _registered = RegisteredInstance::register_in_dir_for_test(record.clone(), dir.path())
            .expect("registered");
        let records = list_instances_from_dir(dir.path());
        assert_eq!(records, vec![record]);
    }

    impl RegisteredInstance {
        fn register_in_dir_for_test(
            record: InstanceRecord,
            dir: &Path,
        ) -> Result<Self, ControlError> {
            fs::create_dir_all(dir).expect("create dir");
            let path = record_path(dir, &record.instance_id);
            let bytes = serde_json::to_vec_pretty(&record).expect("serialize");
            fs::write(&path, bytes).expect("write");
            Ok(Self { record, path })
        }
    }
}
