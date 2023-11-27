#![cfg_attr(not(feature = "std"), no_std)]
#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::{impl_benchmark_test_suite, v2::*, whitelisted_caller, BenchmarkError};
use frame_support::{
	dispatch::{DispatchClass, DispatchInfo, PostDispatchInfo},
	traits::Get,
	weights::Weight,
};
use frame_system::{
	CheckGenesis, CheckMortality, CheckNonZeroSender, CheckNonce, CheckSpecVersion, CheckTxVersion,
	CheckWeight, Pallet as System, RawOrigin,
};
use sp_runtime::traits::{AsSystemOriginSigner, DispatchTransaction, Dispatchable, One};
use sp_std::prelude::*;

pub struct Pallet<T: Config>(System<T>);
pub trait Config: frame_system::Config + Send + Sync {
	fn default_call() -> Self::RuntimeCall {
		todo!();
	}

	fn dispatch_info(
		weight: Weight,
		class: DispatchClass,
	) -> <Self::RuntimeCall as Dispatchable>::Info {
		todo!();
	}
}

#[benchmarks(where
    T: Config,
    T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone)
]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn check_genesis() -> Result<(), BenchmarkError> {
		let weights = T::BlockWeights::get();
		// let free = DispatchInfo { weight: Weight::zero(), ..Default::default() };
		let len = 0_usize;

		assert_eq!(System::<T>::block_weight().total(), weights.base_block);
		let caller = whitelisted_caller();

		#[block]
		{
			assert!(CheckGenesis::<T>::new()
				.validate_and_prepare(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len
				)
				.is_ok());
		}

		assert_eq!(
			System::<T>::block_weight().total(),
			weights.get(DispatchClass::Normal).base_extrinsic + weights.base_block
		);
		Ok(())
	}

	#[benchmark]
	fn check_mortality() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let ext = CheckMortality::<T>::from(sp_runtime::generic::Era::mortal(16, 256));
		let block_number: frame_system::pallet_prelude::BlockNumberFor<T> = One::one();
		let block_number: frame_system::pallet_prelude::BlockNumberFor<T> =
			block_number * 17u32.into();
		System::<T>::set_block_number(block_number);

		let caller = whitelisted_caller();

		#[block]
		{
			assert_eq!(
				ext.validate_only(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(
						Weight::from_parts(100, 0),
						DispatchClass::Normal,
					),
					len
				)
				.unwrap()
				.0
				.longevity,
				15
			);
		}
		Ok(())
	}

	#[benchmark]
	fn check_non_zero_sender() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let ext = CheckNonZeroSender::<T>::new();
		let caller = whitelisted_caller();

		#[block]
		{
			assert!(ext
				.validate_only(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len
				)
				.is_ok());
		}
		Ok(())
	}

	#[benchmark]
	fn check_nonce() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let info = frame_system::AccountInfo::default();
		frame_system::Account::<T>::insert(caller.clone(), info);
		let len = 0_usize;
		let ext = CheckNonce::<T>::from(One::one());

		#[block]
		{
			assert!(ext
				.validate_only(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len
				)
				.is_ok());
		}
		Ok(())
	}

	#[benchmark]
	fn check_spec_version() -> Result<(), BenchmarkError> {
		let weights = T::BlockWeights::get();
		let len = 0_usize;

		assert_eq!(System::<T>::block_weight().total(), weights.base_block);
		let caller = whitelisted_caller();

		#[block]
		{
			assert!(CheckSpecVersion::<T>::new()
				.validate_and_prepare(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len
				)
				.is_ok());
		}
		Ok(())
	}

	#[benchmark]
	fn check_tx_version() -> Result<(), BenchmarkError> {
		let weights = T::BlockWeights::get();
		let len = 0_usize;

		assert_eq!(System::<T>::block_weight().total(), weights.base_block);
		let caller = whitelisted_caller();

		#[block]
		{
			assert!(CheckTxVersion::<T>::new()
				.validate_and_prepare(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len
				)
				.is_ok());
		}
		Ok(())
	}

	#[benchmark]
	fn check_weight() -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		let info = <T as Config>::dispatch_info(Weight::from_parts(512, 0), Default::default());
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(700, 0)),
			pays_fee: Default::default(),
		};
		let len = 0_usize;

		let ext = CheckWeight::<T>::new();

		frame_system::BlockWeight::<T>::mutate(|current_weight| {
			current_weight.set(Weight::zero(), DispatchClass::Mandatory);
			current_weight.set(Weight::from_parts(128, 0), DispatchClass::Normal);
		});

		#[block]
		{
			let pre = ext
				.validate_and_prepare(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&info,
					len,
				)
				.unwrap()
				.0;
			assert_eq!(
				System::<T>::block_weight().total(),
				info.weight +
					Weight::from_parts(128, 0) +
					<T as frame_system::Config>::BlockWeights::get()
						.get(DispatchClass::Normal)
						.base_extrinsic,
			);

			// assert_ok!(ext::post_dispatch(pre, &info, &post_info, len, &Ok(()), &()));
		}

		assert_eq!(
			System::<T>::block_weight().total(),
			info.weight +
				Weight::from_parts(128, 0) +
				<T as frame_system::Config>::BlockWeights::get()
					.get(DispatchClass::Normal)
					.base_extrinsic,
		);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
