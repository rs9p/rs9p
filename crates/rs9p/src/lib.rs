#![forbid(unsafe_code)]
//! Asynchronous 9P2000.L filesystem server library for Rust.
//!
//! This crate provides a tokio-based async implementation of the 9P2000.L protocol,
//! allowing you to build virtual filesystem servers that can be mounted using the
//! Linux kernel's v9fs module.
//!
//! # Overview
//!
//! The 9P protocol was originally developed for the Plan 9 distributed operating system.
//! 9P2000.L is an extended variant that adds Linux-specific features like proper
//! permission handling, symbolic links, and other POSIX semantics.
//!
//! # Getting Started
//!
//! To create a 9P filesystem server, you need to:
//!
//! 1. Define a type to represent your per-fid state (or use `()` for stateless fids)
//! 2. Implement the [`srv::Filesystem`] trait for your filesystem type
//! 3. Start the server with [`srv::srv_async`] or related functions
//!
//! # Example
//!
//! ```no_run
//! use rs9p::{srv::{Filesystem, Fid, srv_async}, Result, Fcall};
//! use async_trait::async_trait;
//!
//! // Define your filesystem
//! #[derive(Clone)]
//! struct MyFs;
//!
//! // Define per-fid state (or use () if you don't need state)
//! #[derive(Default)]
//! struct MyFid {
//!     // Your per-fid data here
//! }
//!
//! #[async_trait]
//! impl Filesystem for MyFs {
//!     type Fid = MyFid;
//!
//!     async fn rattach(
//!         &self,
//!         fid: &Fid<Self::Fid>,
//!         _afid: Option<&Fid<Self::Fid>>,
//!         _uname: &str,
//!         _aname: &str,
//!         _n_uname: u32,
//!     ) -> Result<Fcall> {
//!         // Initialize the root fid and return its qid
//!         Ok(Fcall::Rattach {
//!             qid: rs9p::Qid {
//!                 typ: rs9p::QidType::DIR,
//!                 version: 0,
//!                 path: 0,
//!             }
//!         })
//!     }
//!
//!     // Implement other required methods...
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let fs = MyFs;
//!     srv_async(fs, "tcp!127.0.0.1!564").await
//! }
//! ```
//!
//! # Protocol Details
//!
//! ## Message Flow
//!
//! 1. **Version Negotiation**: Client sends `Tversion`, server responds with `Rversion`
//! 2. **Authentication** (optional): `Tauth`/`Rauth` exchange
//! 3. **Attach**: Client attaches to the filesystem root with `Tattach`
//! 4. **Operations**: Client performs file operations (`walk`, `open`, `read`, `write`, etc.)
//! 5. **Cleanup**: Client clunks fids with `Tclunk` to release resources
//!
//! ## Fid Management
//!
//! A "fid" (file identifier) is a 32-bit handle used by the client to reference a file
//! or directory. The server must track the mapping between fids and filesystem objects.
//!
//! **Important invariants:**
//! - Each fid is unique per connection
//! - Fids persist across operations until explicitly clunked
//! - Walking to a new fid creates a new fid (the old one remains valid)
//! - After `Tclunk`, the fid is invalid and will be removed
//!
//! # Error Handling
//!
//! Return errors using the [`error::Error`] type. The server will automatically
//! convert these to `Rlerror` messages with appropriate error codes (errno).
//!
//! Common error codes:
//! - `ENOENT` - File not found
//! - `EACCES` / `EPERM` - Permission denied
//! - `EISDIR` - Is a directory (when file expected)
//! - `ENOTDIR` - Not a directory (when directory expected)
//! - `EBADF` - Bad file descriptor (invalid fid)
//! - `ELOOP` - Too many levels of symbolic links
//!
//! # Transport
//!
//! The library supports multiple transports:
//! - **TCP**: `"tcp!host!port"` (e.g., `"tcp!0.0.0.0!564"`)
//! - **Unix Domain Sockets**: `"unix!path!suffix"` (e.g., `"unix!/tmp/socket!0"`)
//!
//! # Feature Flags
//!
//! This crate uses workspace dependencies and requires:
//! - `tokio` with `full` features for async runtime
//! - `async-trait` for trait async methods
//!
//! # Safety
//!
//! This crate forbids unsafe code (`#![forbid(unsafe_code)]`) and relies on Rust's
//! type system for memory safety. All filesystem operations are async and designed
//! to be cancellation-safe.
pub mod error;
pub mod fcall;
pub mod serialize;
pub mod srv;
#[macro_use]
pub mod utils;

pub use crate::error::Error;
pub use crate::error::errno;
pub use crate::error::string as errstr;
pub use crate::fcall::*;
pub use crate::utils::Result;
