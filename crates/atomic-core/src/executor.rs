//! Shared background runtime for all fire-and-forget work.
//!
//! Isolates background processing (embedding, tagging, ingestion) from the
//! server's request-handling runtime. Provides resource-aware semaphores so
//! concurrent API calls stay within safe limits regardless of caller.

use std::sync::LazyLock;
use tokio::sync::Semaphore;

/// Shared background runtime — 4 worker threads, isolated from server/Tauri runtimes.
static BACKGROUND: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("atomic-bg")
        .enable_all()
        .build()
        .expect("Failed to create background runtime")
});

/// Concurrency limit for LLM API calls (tagging, wiki, chat tool calls).
pub static LLM_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

/// Concurrency limit for embedding API calls.
pub static EMBEDDING_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

/// Concurrency limit for embedding batch tasks (limits memory from queued content).
pub static EMBEDDING_BATCH_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));

/// Concurrency limit for HTTP fetches (ingestion pipeline).
pub static FETCH_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(8));

/// Spawn a fire-and-forget task on the shared background runtime.
/// Works from any context (sync or async).
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    BACKGROUND.spawn(future);
}
