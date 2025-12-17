//! Executes PVQ programs on top of [`polkavm`].
//!
//! This crate provides [`PvqExecutor`], a small wrapper around a [`polkavm::Engine`] and
//! [`polkavm::Linker`] that:
//!
//! - Instantiates a PVQ program from a PolkaVM program blob.
//! - Writes the provided argument bytes into the module's auxiliary data region.
//! - Calls the guest entrypoint named `"pvq"`.
//! - Optionally enables gas metering and returns the remaining gas.
//!
//! Host functions are registered through the [`PvqExecutorContext`] trait, which is also
//! responsible for providing the mutable user data passed to host calls.
//!
//! ## Example
//!
//! The executor expects the guest to export a function named `"pvq"`. This crate does not define
//! the guest ABI beyond passing `args` to the auxiliary data region and expecting the guest to
//! return a `(ptr, len)` pair encoded in a `u64`.
//!
//! ```no_run
//! use pvq_executor::{PvqExecutor, PvqExecutorContext};
//! use polkavm::{Config, Linker};
//!
//! struct MyCtx {
//!     data: (),
//! }
//!
//! impl PvqExecutorContext for MyCtx {
//!     type UserData = ();
//!     type UserError = core::convert::Infallible;
//!
//!     fn register_host_functions(&mut self, _linker: &mut Linker<Self::UserData, Self::UserError>) {}
//!
//!     fn data(&mut self) -> &mut Self::UserData {
//!         &mut self.data
//!     }
//! }
//!
//! let mut executor = PvqExecutor::new(Config::default(), MyCtx { data: () });
//! let program = std::fs::read("program.polkavm")?;
//! let args = b"\x01\x02\x03";
//! let (result, gas_remaining) = executor.execute(&program, args, None);
//! # let _ = (result, gas_remaining);
//! # Ok::<(), std::io::Error>(())
//! ```
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use alloc::vec::Vec;
pub use polkavm::{Caller, Config, Engine, Linker, Module, ProgramBlob};

mod context;
mod error;
mod executor;

pub use context::PvqExecutorContext;
pub use error::PvqExecutorError;
pub use executor::PvqExecutor;
