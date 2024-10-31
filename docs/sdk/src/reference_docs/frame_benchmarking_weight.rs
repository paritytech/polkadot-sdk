//! # FRAME Benchmarking and Weights.
//!
//! This reference doc explores the concept of weights within Polkadot-SDK runtimes, and more
//! specifically how FRAME-based runtimes handle it.
//!
//! ## Metering
//!
//! The existence of "weight" as a concept in Polkadot-SDK is a direct consequence of the usage of
//! WASM as a virtual machine. Unlike a metered virtual machine like EVM, where every instruction
//! can have a (fairly) deterministic "cost" (also known as "gas price") associated with it, WASM is
//! a stack machine with more complex instruction set, and more unpredictable execution times. This
//! means that unlike EVM, it is not possible to implement a "metering" system in WASM. A metering
//! system is one in which instructions are executed one by one, and the cost/gas is stored in an
//! accumulator. The execution may then halt once a gas limit is reached.
//!
//! In Polkadot-SDK, the WASM runtime is not assumed to be metered.
//!
//! ## Trusted Code
//!
//! Another important difference is that EVM is mostly used to express smart contracts, which are
//! foreign and untrusted codes from the perspective of the blockchain executing them. In such
//! cases, metering is crucial, in order to ensure a malicious code cannot consume more gas than
//! expected.
//!
//! This assumption does not hold about the runtime of Polkadot-SDK-based blockchains. The runtime
//! is trusted code, and it is assumed to be written by the same team/developers who are running the
//! blockchain itself. Therefore, this assumption of "untrusted foreign code" does not hold.
//!
//! This is why the runtime can opt for a more performant, more flexible virtual machine like WASM,
//! and get away without having metering.
//!
//! ## Benchmarking
//!
//! With the matter of untrusted code execution out of the way, the need for strict metering goes
//! out of the way. Yet, it would still be very beneficial for block producers to be able to know an
//! upper bound on how much resources a operation is going to consume before actually executing that
//! operation. This is why FRAME has a toolkit for benchmarking pallets: So that this upper bound
//! can be empirically determined.
//!
//! > Note: Benchmarking is a static analysis: It is all about knowing the upper bound of how much
//! > resources an operation takes statically, without actually executing it. In the context of
//! > FRAME extrinsics, this static-ness is expressed by the keyword "pre-dispatch".
//!
//! To understand why this upper bound is needed, consider the following: A block producer knows
//! they have 20ms left to finish producing their block, and wishes to include more transactions in
//! the block. Yet, in a metered environment, it would not know which transaction is likely to fit
//! the 20ms. In a benchmarked environment, it can examine the transactions for their upper bound,
//! and include the ones that are known to fit based on the worst case.
//!
//! The benchmarking code can be written as a part of FRAME pallet, using the macros provided in
//! [`frame_benchmarking`]. See any of the existing pallets in `polkadot-sdk`, or the pallets in our
//! [`crate::polkadot_sdk::templates`] for examples.
//!
//! ## Weight
//!
//! Finally, [`sp_weights::Weight`] is the output of the benchmarking process. It is a
//! two-dimensional data structure that demonstrates the resources consumed by a given block of
//! code (for example, a transaction). The two dimensions are:
//!
//! * reference time: The time consumed in pico-seconds, on a reference hardware.
//! * proof size: The amount of storage proof necessary to re-execute the block of code. This is
//!   mainly needed for parachain <> relay-chain verification.
//!
//! ## How To Write Benchmarks: Worst Case
//!
//! The most important detail about writing benchmarking code is that it must be written such that
//! it captures the worst case execution of any block of code.
//!
//! Consider:
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer)]
//!
//! If this block of code is to be benchmarked, then the benchmarking code must be written such that
//! it captures the worst case.
//!
//! ## Gluing Pallet Benchmarking with Runtime
//!
//! FRAME pallets are mandated to provide their own benchmarking code. Runtimes contain the
//! boilerplate needed to run these benchmarking (see [Running Benchmarks
//! below](#running-benchmarks)). The outcome of running these benchmarks are meant to be fed back
//! into the pallet via a conventional `trait WeightInfo` on `Config`:
#![doc = docify::embed!("src/reference_docs/frame_benchmarking_weight.rs", WeightInfo)]
//!
//! Then, individual functions of this trait are the final values that we assigned to the
//! [`frame::pallet_macros::weight`] attribute:
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer_2)]
//!
//! ## Manual Refund
//!
//! Back to the assumption of writing benchmarks for worst case: Sometimes, the pre-dispatch weight
//! significantly differ from the post-dispatch actual weight consumed. This can be expressed with
//! the following FRAME syntax:
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer_3)]
//!
//! ## Running Benchmarks
//!
//! Two ways exist to run the benchmarks of a runtime.
//!
//! 1. The old school way: Most Polkadot-SDK based nodes (such as the ones integrated in
//!    [`templates`]) have an a `benchmark` subcommand integrated into themselves.
//! 2. The more [`crate::reference_docs::omni_node`] compatible way of running the benchmarks would
//!    be using [`frame-omni-bencher`] CLI, which only relies on a runtime.
//!
//! Note that by convention, the runtime and pallets always have their benchmarking code feature
//! gated as behind `runtime-benchmarks`. So, the runtime should be compiled with `--features
//! runtime-benchmarks`.
//!
//! ## Automatic Refund of `proof_size`.
//!
//! A new feature in FRAME allows the runtime to be configured for "automatic refund" of the proof
//! size weight. This is very useful for maximizing the throughput of parachains. Please see:
//! [`crate::guides::enable_pov_reclaim`].
//!
//! ## Summary
//!
//! Polkadot-SDK runtimes use a more performant VM, namely WASM, which does not have metering. In
//! return they have to be benchmarked to provide an upper bound on the resources they consume. This
//! upper bound is represented as [`sp_weights::Weight`].
//!
//! ## Future: PolkaVM
//!
//! With the transition of Polkadot relay chain to [JAM], a set of new features are being
//! introduced, one of which being a new virtual machine named [PolkaVM] that is as flexible as
//! WASM, but also capable of metering. This might alter the future of benchmarking in FRAME and
//! Polkadot-SDK, rendering them not needed anymore once PolkaVM is fully integrated into
//! Polkadot-sdk. For a basic explanation of JAM and PolkaVM, see [here](https://blog.kianenigma.com/posts/tech/demystifying-jam/#pvm).
//!
//!
//! [`frame-omni-bencher`]: https://crates.io/crates/frame-omni-bencher
//! [`templates`]: crate::polkadot_sdk::templates
//! [PolkaVM]: https://github.com/koute/polkavm
//! [JAM]: https://graypaper.com

#[frame::pallet(dev_mode)]
#[allow(unused_variables, unreachable_code, unused, clippy::diverging_sub_expression)]
pub mod pallet {
	use frame::prelude::*;

	#[docify::export]
	pub trait WeightInfo {
		fn simple_transfer() -> Weight;
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[docify::export]
		#[pallet::weight(10_000)]
		pub fn simple_transfer(
			origin: OriginFor<T>,
			destination: T::AccountId,
			amount: u32,
		) -> DispatchResult {
			let destination_exists = todo!();
			if destination_exists {
				// simpler code path
			} else {
				// more complex code path
			}
			Ok(())
		}

		#[docify::export]
		#[pallet::weight(T::WeightInfo::simple_transfer())]
		pub fn simple_transfer_2(
			origin: OriginFor<T>,
			destination: T::AccountId,
			amount: u32,
		) -> DispatchResult {
			let destination_exists = todo!();
			if destination_exists {
				// simpler code path
			} else {
				// more complex code path
			}
			Ok(())
		}

		#[docify::export]
		// This is the worst-case, pre-dispatch weight.
		#[pallet::weight(T::WeightInfo::simple_transfer())]
		pub fn simple_transfer_3(
			origin: OriginFor<T>,
			destination: T::AccountId,
			amount: u32,
		) -> DispatchResultWithPostInfo {
			// ^^ Notice the new return type
			let destination_exists = todo!();
			if destination_exists {
				// simpler code path
				// Note that need for .into(), to convert `()` to `PostDispatchInfo`
				// See: https://paritytech.github.io/polkadot-sdk/master/frame_support/dispatch/struct.PostDispatchInfo.html#impl-From%3C()%3E-for-PostDispatchInfo
				Ok(().into())
			} else {
				// more complex code path
				let actual_weight =
					todo!("this can likely come from another benchmark that is NOT the worst case");
				let pays_fee = todo!("You can set this to `Pays::Yes` or `Pays::No` to change if this transaction should pay fees");
				Ok(frame::deps::frame_support::dispatch::PostDispatchInfo {
					actual_weight: Some(actual_weight),
					pays_fee,
				})
			}
		}
	}
}
