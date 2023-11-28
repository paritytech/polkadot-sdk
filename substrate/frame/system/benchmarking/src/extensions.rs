#![cfg_attr(not(feature = "std"), no_std)]
#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::{impl_benchmark_test_suite, v2::*, whitelisted_caller, BenchmarkError};
use frame_support::{
	dispatch::{DispatchClass, DispatchInfo, PostDispatchInfo},
	weights::Weight,
};
use frame_system::{
	pallet_prelude::*, CheckGenesis, CheckMortality, CheckNonZeroSender, CheckNonce,
	CheckSpecVersion, CheckTxVersion, CheckWeight, Pallet as System, RawOrigin,
};
use sp_runtime::{
	generic::Era,
	traits::{AsSystemOriginSigner, DispatchTransaction, Dispatchable, Get},
};
use sp_std::prelude::*;

pub struct Pallet<T: Config>(System<T>);
pub trait Config: frame_system::Config + Send + Sync {
	fn default_call() -> Self::RuntimeCall;

	fn dispatch_info(
		_weight: Weight,
		_class: DispatchClass,
	) -> <Self::RuntimeCall as Dispatchable>::Info;
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
		let len = 0_usize;
		let caller = whitelisted_caller();

		#[block]
		{
			CheckGenesis::<T>::new()
				.test_run(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len,
					|_| Ok(().into()),
				)
				.unwrap()
				.unwrap();
		}

		Ok(())
	}

	#[benchmark]
	fn check_mortality() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let ext = CheckMortality::<T>::from(Era::mortal(16, 256));
		let block_number: BlockNumberFor<T> = 17u32.into();
		System::<T>::set_block_number(block_number);
		let prev_block: BlockNumberFor<T> = 16u32.into();
		let default_hash: T::Hash = Default::default();
		frame_system::BlockHash::<T>::insert(prev_block, default_hash);
		let caller = whitelisted_caller();

		#[block]
		{
			ext.test_run(
				RawOrigin::Signed(caller).into(),
				&<T as Config>::default_call(),
				&<T as Config>::dispatch_info(Weight::from_parts(100, 0), DispatchClass::Normal),
				len,
				|_| Ok(().into()),
			)
			.unwrap()
			.unwrap();
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
			ext.test_run(
				RawOrigin::Signed(caller).into(),
				&<T as Config>::default_call(),
				&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
				len,
				|_| Ok(().into()),
			)
			.unwrap()
			.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_nonce() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let mut info = frame_system::AccountInfo::default();
		info.nonce = 1u32.into();
		info.providers = 1;
		let expected_nonce = info.nonce + 1u32.into();
		frame_system::Account::<T>::insert(caller.clone(), info);
		let len = 0_usize;
		let ext = CheckNonce::<T>::from(1u32.into());

		#[block]
		{
			ext.test_run(
				RawOrigin::Signed(caller.clone()).into(),
				&<T as Config>::default_call(),
				&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
				len,
				|_| Ok(().into()),
			)
			.unwrap()
			.unwrap();
		}

		let updated_info = frame_system::Account::<T>::get(caller.clone());
		assert_eq!(updated_info.nonce, expected_nonce);
		Ok(())
	}

	#[benchmark]
	fn check_spec_version() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let caller = whitelisted_caller();

		#[block]
		{
			CheckSpecVersion::<T>::new()
				.test_run(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len,
					|_| Ok(().into()),
				)
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_tx_version() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let caller = whitelisted_caller();

		#[block]
		{
			CheckTxVersion::<T>::new()
				.test_run(
					RawOrigin::Signed(caller).into(),
					&<T as Config>::default_call(),
					&<T as Config>::dispatch_info(Weight::zero(), Default::default()),
					len,
					|_| Ok(().into()),
				)
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_weight() -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		let base_extrinsic = <T as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.base_extrinsic;
		let info = <T as Config>::dispatch_info(
			Weight::from_parts(base_extrinsic.ref_time() * 5, 0),
			DispatchClass::Normal,
		);
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(base_extrinsic.ref_time() * 2, 0)),
			pays_fee: Default::default(),
		};
		let len = 0_usize;
		let base_extrinsic = <T as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.base_extrinsic;

		let ext = CheckWeight::<T>::new();

		let initial_block_weight = Weight::from_parts(base_extrinsic.ref_time() * 2, 0);
		frame_system::BlockWeight::<T>::mutate(|current_weight| {
			current_weight.set(Weight::zero(), DispatchClass::Mandatory);
			current_weight.set(initial_block_weight, DispatchClass::Normal);
		});

		#[block]
		{
			ext.test_run(
				RawOrigin::Signed(caller).into(),
				&<T as Config>::default_call(),
				&info,
				len,
				|_| Ok(post_info),
			)
			.unwrap()
			.unwrap();
		}

		assert_eq!(
			System::<T>::block_weight().total(),
			initial_block_weight + base_extrinsic + post_info.actual_weight.unwrap(),
		);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
