//! Error types for traceview.

use thiserror::Error;

/// The main error type for traceview operations.
#[derive(Debug, Error)]
pub enum TraceviewError {
    /// Database operation failed.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// JSON serialization/deserialization failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Protobuf decode failed.
    #[error("protobuf error: {0}")]
    Protobuf(#[from] prost::DecodeError),

    /// Invalid span data encountered.
    #[error("invalid span: {reason}")]
    InvalidSpan { reason: String },

    /// Invalid OTLP data encountered.
    #[error("invalid OTLP: {reason}")]
    InvalidOtlp { reason: String },

    /// Failed to send on broadcast channel.
    #[error("channel send error")]
    ChannelSend,

    /// I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A Result type alias using TraceviewError.
pub type Result<T> = std::result::Result<T, TraceviewError>;

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_invalid_span() {
        let err = TraceviewError::InvalidSpan { reason: "missing trace_id".to_string() };
        assert_eq!(err.to_string(), "invalid span: missing trace_id");
    }

    #[test]
    fn test_error_display_invalid_otlp() {
        let err = TraceviewError::InvalidOtlp { reason: "malformed protobuf".to_string() };
        assert_eq!(err.to_string(), "invalid OTLP: malformed protobuf");
    }

    #[test]
    fn test_error_display_channel_send() {
        let err = TraceviewError::ChannelSend;
        assert_eq!(err.to_string(), "channel send error");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: TraceviewError = io_err.into();
        assert!(matches!(err, TraceviewError::Io(_)));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_from_json_error() {
        let json_result: std::result::Result<serde_json::Value, _> =
            serde_json::from_str("invalid json");
        let json_err = json_result.unwrap_err();
        let err: TraceviewError = json_err.into();
        assert!(matches!(err, TraceviewError::Json(_)));
        assert!(err.to_string().contains("json error"));
    }

    #[test]
    fn test_pattern_matching() {
        let errors = vec![
            TraceviewError::InvalidSpan { reason: "test".to_string() },
            TraceviewError::InvalidOtlp { reason: "test".to_string() },
            TraceviewError::ChannelSend,
        ];

        for err in errors {
            match err {
                TraceviewError::Database(_) => panic!("unexpected Database variant"),
                TraceviewError::Json(_) => panic!("unexpected Json variant"),
                TraceviewError::InvalidSpan { reason } => {
                    assert_eq!(reason, "test");
                }
                TraceviewError::InvalidOtlp { reason } => {
                    assert_eq!(reason, "test");
                }
                TraceviewError::ChannelSend => {}
                TraceviewError::Io(_) => panic!("unexpected Io variant"),
                TraceviewError::Protobuf(_) => panic!("unexpected Protobuf variant"),
            }
        }
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_ok() -> Result<i32> {
            Ok(42)
        }

        fn returns_err() -> Result<i32> {
            Err(TraceviewError::InvalidSpan { reason: "test error".to_string() })
        }

        assert!(returns_ok().is_ok());
        assert!(returns_err().is_err());
    }
}
