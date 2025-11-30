use std::fmt;

/// Errors that can occur during provider operations
#[derive(Debug)]
pub enum ProviderError {
    /// Network/connection error
    Network(String),

    /// API error with status code
    Api { status: u16, message: String },

    /// Rate limited - may include retry-after hint
    RateLimited { retry_after_secs: Option<u64> },

    /// Model not found or unavailable
    ModelNotFound(String),

    /// Configuration error (missing API key, invalid settings, etc.)
    Configuration(String),

    /// Capability not supported by this provider
    CapabilityNotSupported(String),

    /// Failed to parse response
    ParseError(String),

    /// Provider not initialized
    NotInitialized,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Network(msg) => write!(f, "Network error: {}", msg),
            ProviderError::Api { status, message } => {
                write!(f, "API error ({}): {}", status, message)
            }
            ProviderError::RateLimited { retry_after_secs } => {
                if let Some(secs) = retry_after_secs {
                    write!(f, "Rate limited, retry after {} seconds", secs)
                } else {
                    write!(f, "Rate limited")
                }
            }
            ProviderError::ModelNotFound(model) => write!(f, "Model not found: {}", model),
            ProviderError::Configuration(msg) => write!(f, "Configuration error: {}", msg),
            ProviderError::CapabilityNotSupported(cap) => {
                write!(f, "Capability not supported: {}", cap)
            }
            ProviderError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            ProviderError::NotInitialized => write!(f, "Provider not initialized"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl ProviderError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::RateLimited { .. } | ProviderError::Network(_)
        )
    }

    /// Get suggested retry delay in seconds
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            ProviderError::RateLimited { retry_after_secs } => *retry_after_secs,
            ProviderError::Network(_) => Some(1), // Default 1 second for network errors
            _ => None,
        }
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(err: reqwest::Error) -> Self {
        ProviderError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(err: serde_json::Error) -> Self {
        ProviderError::ParseError(err.to_string())
    }
}

// Allow converting to String for backward compatibility
impl From<ProviderError> for String {
    fn from(err: ProviderError) -> Self {
        err.to_string()
    }
}
