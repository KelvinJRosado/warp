use crate::discovery::InstanceRecord;
use crate::protocol::{
    ControlError, ControlResponse, ErrorCode, ErrorResponseEnvelope, RequestEnvelope,
    ResponseEnvelope,
};

pub fn send_request(
    instance: &InstanceRecord,
    request: &RequestEnvelope,
) -> Result<ResponseEnvelope, ControlError> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(instance.endpoint.url())
        .header("Authorization", instance.auth().authorization_value())
        .json(request)
        .send()
        .map_err(|err| {
            ControlError::with_details(
                ErrorCode::TransportUnavailable,
                "failed to send local-control request",
                err.to_string(),
            )
        })?;
    let status = response.status();
    let text = response.text().map_err(|err| {
        ControlError::with_details(
            ErrorCode::TransportUnavailable,
            "failed to read local-control response",
            err.to_string(),
        )
    })?;
    if status.is_success() {
        let envelope = serde_json::from_str::<ResponseEnvelope>(&text).map_err(|err| {
            ControlError::with_details(
                ErrorCode::InvalidRequest,
                "failed to decode local-control response",
                err.to_string(),
            )
        })?;
        if let ControlResponse::Error { error } = &envelope.response {
            return Err(error.clone());
        }
        return Ok(envelope);
    }
    if let Ok(envelope) = serde_json::from_str::<ErrorResponseEnvelope>(&text) {
        return Err(envelope.error);
    }
    Err(ControlError::with_details(
        ErrorCode::TransportUnavailable,
        format!("local-control request failed with HTTP {status}"),
        text,
    ))
}
