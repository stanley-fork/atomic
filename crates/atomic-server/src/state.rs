//! Application state and server event types

use atomic_core::AtomicCore;
use serde::Serialize;
use tokio::sync::broadcast;

/// Shared application state for all route handlers
pub struct AppState {
    pub core: AtomicCore,
    pub event_tx: broadcast::Sender<ServerEvent>,
    /// Public URL for OAuth discovery (set via --public-url CLI flag)
    pub public_url: Option<String>,
}

/// Events broadcast to WebSocket clients
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    // Embedding pipeline events
    EmbeddingStarted {
        atom_id: String,
    },
    EmbeddingComplete {
        atom_id: String,
    },
    EmbeddingFailed {
        atom_id: String,
        error: String,
    },
    TaggingComplete {
        atom_id: String,
        tags_extracted: Vec<String>,
        new_tags_created: Vec<String>,
    },
    TaggingFailed {
        atom_id: String,
        error: String,
    },
    TaggingSkipped {
        atom_id: String,
    },

    // Atom lifecycle events
    AtomCreated {
        atom: atomic_core::AtomWithTags,
    },

    // Chat streaming events
    ChatStreamDelta {
        conversation_id: String,
        content: String,
    },
    ChatToolStart {
        conversation_id: String,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    ChatToolComplete {
        conversation_id: String,
        tool_call_id: String,
        results_count: i32,
    },
    ChatComplete {
        conversation_id: String,
        message: atomic_core::ChatMessageWithContext,
    },
    ChatError {
        conversation_id: String,
        error: String,
    },
}

impl From<atomic_core::EmbeddingEvent> for ServerEvent {
    fn from(event: atomic_core::EmbeddingEvent) -> Self {
        match event {
            atomic_core::EmbeddingEvent::Started { atom_id } => {
                ServerEvent::EmbeddingStarted { atom_id }
            }
            atomic_core::EmbeddingEvent::EmbeddingComplete { atom_id } => {
                ServerEvent::EmbeddingComplete { atom_id }
            }
            atomic_core::EmbeddingEvent::EmbeddingFailed { atom_id, error } => {
                ServerEvent::EmbeddingFailed { atom_id, error }
            }
            atomic_core::EmbeddingEvent::TaggingComplete {
                atom_id,
                tags_extracted,
                new_tags_created,
            } => ServerEvent::TaggingComplete {
                atom_id,
                tags_extracted,
                new_tags_created,
            },
            atomic_core::EmbeddingEvent::TaggingFailed { atom_id, error } => {
                ServerEvent::TaggingFailed { atom_id, error }
            }
            atomic_core::EmbeddingEvent::TaggingSkipped { atom_id } => {
                ServerEvent::TaggingSkipped { atom_id }
            }
        }
    }
}

impl From<atomic_core::ChatEvent> for ServerEvent {
    fn from(event: atomic_core::ChatEvent) -> Self {
        match event {
            atomic_core::ChatEvent::StreamDelta {
                conversation_id,
                content,
            } => ServerEvent::ChatStreamDelta {
                conversation_id,
                content,
            },
            atomic_core::ChatEvent::ToolStart {
                conversation_id,
                tool_call_id,
                tool_name,
                tool_input,
            } => ServerEvent::ChatToolStart {
                conversation_id,
                tool_call_id,
                tool_name,
                tool_input,
            },
            atomic_core::ChatEvent::ToolComplete {
                conversation_id,
                tool_call_id,
                results_count,
            } => ServerEvent::ChatToolComplete {
                conversation_id,
                tool_call_id,
                results_count,
            },
            atomic_core::ChatEvent::Complete {
                conversation_id,
                message,
            } => ServerEvent::ChatComplete {
                conversation_id,
                message,
            },
            atomic_core::ChatEvent::Error {
                conversation_id,
                error,
            } => ServerEvent::ChatError {
                conversation_id,
                error,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_started_conversion() {
        let event = atomic_core::EmbeddingEvent::Started {
            atom_id: "a1".into(),
        };
        let server_event = ServerEvent::from(event);
        match server_event {
            ServerEvent::EmbeddingStarted { atom_id } => assert_eq!(atom_id, "a1"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_embedding_complete_conversion() {
        let event = atomic_core::EmbeddingEvent::EmbeddingComplete {
            atom_id: "a2".into(),
        };
        match ServerEvent::from(event) {
            ServerEvent::EmbeddingComplete { atom_id } => assert_eq!(atom_id, "a2"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_embedding_failed_conversion() {
        let event = atomic_core::EmbeddingEvent::EmbeddingFailed {
            atom_id: "a3".into(),
            error: "timeout".into(),
        };
        match ServerEvent::from(event) {
            ServerEvent::EmbeddingFailed { atom_id, error } => {
                assert_eq!(atom_id, "a3");
                assert_eq!(error, "timeout");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_tagging_complete_conversion() {
        let event = atomic_core::EmbeddingEvent::TaggingComplete {
            atom_id: "a4".into(),
            tags_extracted: vec!["t1".into()],
            new_tags_created: vec!["t2".into()],
        };
        match ServerEvent::from(event) {
            ServerEvent::TaggingComplete {
                atom_id,
                tags_extracted,
                new_tags_created,
            } => {
                assert_eq!(atom_id, "a4");
                assert_eq!(tags_extracted, vec!["t1"]);
                assert_eq!(new_tags_created, vec!["t2"]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_chat_stream_delta_conversion() {
        let event = atomic_core::ChatEvent::StreamDelta {
            conversation_id: "c1".into(),
            content: "hello".into(),
        };
        match ServerEvent::from(event) {
            ServerEvent::ChatStreamDelta {
                conversation_id,
                content,
            } => {
                assert_eq!(conversation_id, "c1");
                assert_eq!(content, "hello");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_chat_tool_start_conversion() {
        let event = atomic_core::ChatEvent::ToolStart {
            conversation_id: "c2".into(),
            tool_call_id: "tc1".into(),
            tool_name: "search".into(),
            tool_input: serde_json::json!({"query": "test"}),
        };
        match ServerEvent::from(event) {
            ServerEvent::ChatToolStart {
                conversation_id,
                tool_name,
                tool_input,
                ..
            } => {
                assert_eq!(conversation_id, "c2");
                assert_eq!(tool_name, "search");
                assert_eq!(tool_input["query"], "test");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_chat_error_conversion() {
        let event = atomic_core::ChatEvent::Error {
            conversation_id: "c3".into(),
            error: "api failed".into(),
        };
        match ServerEvent::from(event) {
            ServerEvent::ChatError {
                conversation_id,
                error,
            } => {
                assert_eq!(conversation_id, "c3");
                assert_eq!(error, "api failed");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_server_event_serializes_with_type_tag() {
        let event = ServerEvent::EmbeddingComplete {
            atom_id: "a1".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "EmbeddingComplete");
        assert_eq!(json["atom_id"], "a1");
    }

    #[test]
    fn test_event_broadcast_delivery() {
        let (tx, mut rx) = broadcast::channel::<ServerEvent>(16);
        let event = ServerEvent::EmbeddingStarted {
            atom_id: "a1".into(),
        };
        tx.send(event).unwrap();

        let received = rx.try_recv().unwrap();
        match received {
            ServerEvent::EmbeddingStarted { atom_id } => assert_eq!(atom_id, "a1"),
            _ => panic!("Wrong variant"),
        }
    }
}
