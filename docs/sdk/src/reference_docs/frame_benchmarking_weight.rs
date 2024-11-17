#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer)]
#![doc = docify::embed!("src/reference_docs/frame_benchmarking_weight.rs", WeightInfo)]
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer_2)]
#![doc = docify::embed!("./src/reference_docs/frame_benchmarking_weight.rs", simple_transfer_3)]

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




















// [``]:
