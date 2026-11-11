//! C ABI for HTTP and reliable message-session applications over Orbit Link.
//!
//! The caller owns its process and dispatches completion events. Rust owns the
//! Link membership, bounded I/O and application-generation cancellation.

mod abi;
mod application;
mod registry;
mod runtime;
mod serve;

pub use abi::*;
