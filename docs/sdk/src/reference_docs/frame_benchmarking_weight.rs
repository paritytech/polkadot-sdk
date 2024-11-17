//! # FRAME Benchmarking and Weights.
//!
//!
//!
//! a stack machine with more complex instruction set, and more unpredictable execution times. This
//!
//!
//!
//! expected.
//!
//!
//!
//!
//! operation. This is why FRAME has a toolkit for benchmarking pallets: So that this upper bound
//!
//!
//! the 20ms. In a benchmarked environment, it can examine the transactions for their upper bound,
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer)]
//!
//!
//!
//! into the pallet via a conventional `trait WeightInfo` on `Config`:
#![doc = docify::embed!("src/reference_docs/frame_benchmarking_weight.rs", WeightInfo)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer_2)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer_3)]
//!
//!
//!
//!    be using [`frame-omni-bencher`] CLI, which only relies on a runtime.
//!
//!
//!
//!
//!
//!
//!
//! Polkadot-SDK, rendering them not needed anymore once PolkaVM is fully integrated into
//!
//!
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

// Link References

// Link References




// [`frame_benchmarking`]: frame_benchmarking

// [`frame-omni-bencher`]: frame-omni-bencher
// [`frame_benchmarking`]: frame_benchmarking
