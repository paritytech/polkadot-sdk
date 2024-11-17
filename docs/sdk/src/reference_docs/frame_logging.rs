//! # FRAME Logging
//!
//! FRAME-based.
//!
//!
//! out if executed with `--nocapture`, or if the test panics.
//!
//! fn it_print() {
//! }
//!
//! [`sp_std::if_std`]. Of course, this means that this print code is only available to you in the
//!
//! // somewhere in your pallet. This is not a real pallet code.
//! 	struct Pallet;
//! 		fn print() {
//! 				println!("Hello, world!");
//! 		}
//! }
//!
//!
//!
//! 2. `log-target`, signifying to which component it belongs.
//!
//!
//!
//! #[dependencies]
//!
//! std = [
//! 	"log/std"
//! ```
//!
//!
//! level, and the target is `pallet-example`.
//!
//! mod pallet {
//!
//! 		fn logs() {
//! 		}
//! }
//!
//! not go anywhere**. [`sp_api`] provides a handy function to enable the runtime logging:
//!
//! // in your test
//! 	sp_api::init_runtime_logger();
//! }
//!
//!
//! be printed, you must to add the following compiler flag:
//!
//! RUST_LOG=pallet-example=trace cargo test
//!
//!
//! called `disable-logging`, that can be used to disable all logs in the runtime. This is useful
#![doc = docify::embed!("../../substrate/primitives/api/src/lib.rs", init_runtime_logger)]
//!
//! compiling the runtime.
//!
//!
//! pallet>`, for example `runtime::system`. This then allows one to run a node with a wasm blob
//! target starts with `runtime`.
//!
//!
//! [`wasm_meta_protocol`]). The runtime uses a set of host functions under
//! use these APIs directly.

// Link References

// Link References

// [``]: frame_runtime

// [``]: 
