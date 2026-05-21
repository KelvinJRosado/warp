use base64::Engine as _;
use rand::RngCore as _;

use crate::protocol::{ControlError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn from_secret(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    pub fn secret(&self) -> &str {
        &self.0
    }

    pub fn authorization_value(&self) -> String {
        format!("Bearer {}", self.0)
    }

    pub fn verify_authorization_header(&self, value: Option<&str>) -> Result<(), ControlError> {
        let Some(value) = value else {
            return Err(ControlError::new(
                ErrorCode::AuthenticationRequired,
                "Authorization header is required",
            ));
        };
        let Some(token) = value.strip_prefix("Bearer ") else {
            return Err(ControlError::new(
                ErrorCode::AuthenticationFailed,
                "Authorization header must use the Bearer scheme",
            ));
        };
        if token != self.0 {
            return Err(ControlError::new(
                ErrorCode::AuthenticationFailed,
                "Authorization token is invalid",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_authorization_header() {
        let token = AuthToken::from_secret("secret");
        let err = token
            .verify_authorization_header(None)
            .expect_err("rejected");
        assert_eq!(err.code, ErrorCode::AuthenticationRequired);
    }

    #[test]
    fn rejects_wrong_bearer_token() {
        let token = AuthToken::from_secret("secret");
        let err = token
            .verify_authorization_header(Some("Bearer wrong"))
            .expect_err("rejected");
        assert_eq!(err.code, ErrorCode::AuthenticationFailed);
    }

    #[test]
    fn accepts_matching_bearer_token() {
        let token = AuthToken::from_secret("secret");
        token
            .verify_authorization_header(Some("Bearer secret"))
            .expect("accepted");
    }
}
