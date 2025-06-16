//! # FRAME Logging
//!
//! This reference docs briefly explores how to do logging and printing runtimes, mainly
//! FRAME-based.
//!
//! > Please make sure to read [the section below](#using-logging-in-production) on using logging in
//! > production.
//!
//! ## Using `println!`
//!
//! To recap, as with standard Rust, you can use `println!` _in your tests_, but it will only print
//! out if executed with `--nocapture`, or if the test panics.
//!
//! ```
//! fn it_print() {
//! 	println!("Hello, world!");
//! }
//! ```
//!
//! within the pallet, if you want to use the standard `println!`, it needs to be wrapped in
//! [`sp_std::if_std`]. Of course, this means that this print code is only available to you in the
//! `std` compiler flag, and never present in a wasm build.
//!
//! ```
//! // somewhere in your pallet. This is not a real pallet code.
//! mod pallet {
//! 	struct Pallet;
//! 	impl Pallet {
//! 		fn print() {
//! 			sp_std::if_std! {
//! 				println!("Hello, world!");
//! 			}
//! 		}
//! 	}
//! }
//! ```
//!
//! ## Using `log`
//!
//! First, ensure you are familiar with the [`log`] crate. In short, each log statement has:
//!
//! 1. `log-level`, signifying how important it is.
//! 2. `log-target`, signifying to which component it belongs.
//!
//! Add log statements to your pallet as such:
//!
//! You can add the log crate to the `Cargo.toml` of the pallet.
//!
//! ```text
//! #[dependencies]
//! log = { version = "x.y.z", default-features = false }
//!
//! #[features]
//! std = [
//! 	// snip -- other pallets
//! 	"log/std"
//! ]
//! ```
//!
//! More conveniently, the `frame` umbrella crate re-exports the log crate as [`frame::log`].
//!
//! Then, the pallet can use this crate to emit log statements. In this statement, we use the info
//! level, and the target is `pallet-example`.
//!
//! ```
//! mod pallet {
//! 	struct Pallet;
//!
//! 	impl Pallet {
//! 		fn logs() {
//! 			frame::log::info!(target: "pallet-example", "Hello, world!");
//! 		}
//! 	}
//! }
//! ```
//!
//! This will in itself just emit the log messages, **but unless if captured by a logger, they will
//! not go anywhere**. [`sp_api`] provides a handy function to enable the runtime logging:
//!
//! ```
//! // in your test
//! fn it_also_prints() {
//! 	sp_api::init_runtime_logger();
//! 	// call into your pallet, and now it will print `log` statements.
//! }
//! ```
//!
//! Alternatively, you can use [`sp_tracing::try_init_simple`].
//!
//! `info`, `error` and `warn` logs are printed by default, but if you want lower level logs to also
//! be printed, you must to add the following compiler flag:
//!
//! ```text
//! RUST_LOG=pallet-example=trace cargo test
//! ```
//!
//! ## Enabling Logs in Production
//!
//! All logs from the runtime are emitted by default, but there is a feature flag in [`sp_api`],
//! called `disable-logging`, that can be used to disable all logs in the runtime. This is useful
//! for production chains to reduce the size and overhead of the wasm runtime.
#![doc = docify::embed!("../../substrate/primitives/api/src/lib.rs", init_runtime_logger)]
//!
//! Similar to the above, the proper `RUST_LOG` must also be passed to your compiler flag when
//! compiling the runtime.
//!
//! ## Log Target Prefixing
//!
//! Many [`crate::polkadot_sdk::frame_runtime`] pallets emit logs with log target `runtime::<name of
//! pallet>`, for example `runtime::system`. This then allows one to run a node with a wasm blob
//! compiled with `LOG_TARGET=runtime=debug`, which enables the log target of all pallets who's log
//! target starts with `runtime`.
//!
//! ## Low Level Primitives
//!
//! Under the hood, logging is another instance of host functions under the hood (as defined in
//! [`crate::reference_docs::wasm_meta_protocol`]). The runtime uses a set of host functions under
//! [`sp_io::logging`] and [`sp_io::misc`] to emit all logs and prints. You typically do not need to
//! use these APIs directly.
//!
//! ## Using Logging in Production
//!
//! Note that within FRAME, reading storage values __only for the purpose of logging__ is dangerous,
//! and can lead to consensus issues. This is because with the introduction of
//! [`crate::guides::enable_pov_reclaim`], the node side code will track the storage changes, and
//! tries to update the onchain record of the `proof_size` weight used (stored in
//! [`frame_system::BlockWeight`]) after the block is executed.
//!
//! If one node has a different log level enabled than the rest of the network, and the extra logs
//! impose additional storage reads, then the amount of `proof_size` weight reclaimed into
//! [`frame_system::BlockWeight`] will be different, causing a state root mismatch, which is
//! typically a fatal error emitted from [`frame_executive`].
//!
//! This also can also happen in a parachain context, and cause discrepancies between the relay
//! chain and the parachain, when execution the Parachain Validation Function (PVF) on the relay
//! chain.
//!
//! **In summary, you should only used storage values in logging (especially for levels lower than
//! `info` which is typically enabled by all parties) that are already read from storage, and will
//! be part of the storage proof of execution in any case**.
//!
//! A typical faulty code would look like this:
//!
//! ```ignore
//! /// This function will have a different storage footprint depending on the log level
//! fn faulty_logging() {
//! 	log::debug!(
//! 		"what I am about to print is only read when `RUST_LOG=debug` {:?}",
//! 		StorageValue::<T>::get()
//!  	);
//! }
//! ```
//!
//! Please read [this issue](https://github.com/paritytech/polkadot-sdk/issues/8735) for one
//! instance of the consensus issues caused by this mistake.
