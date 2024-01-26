//! # WASM Memory Limitations.
//!
//! Notes:
//!
//! - Stack: Need to use `Box<_>`
//! - Heap: Substrate imposes a limit. PvF execution has its own limits
//! - Heap: There is also a maximum amount that a single allocation can have.
