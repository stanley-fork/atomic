//! Provider module - re-exports from atomic-core

pub use atomic_core::providers::{
    // Config
    ProviderConfig, ProviderType,
    // Factory functions
    create_streaming_llm_provider,
    // Cached factory functions
    get_embedding_provider, get_model_capabilities,
};

// Re-export commonly used items from models at top level
pub use atomic_core::providers::models::{
    fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache,
    AvailableModel,
};

// Re-export submodules for backward compatibility
pub mod models {
    pub use atomic_core::providers::models::*;
}

pub mod traits {
    pub use atomic_core::providers::traits::*;
}

pub mod types {
    pub use atomic_core::providers::types::*;
}

