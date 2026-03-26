use crate::{
	assert_noop, assert_ok,
	dispatch::{DispatchExtension, DispatchResultWithPostInfo},
	parameter_types,
	traits::ExtendedDispatchable,
};
use sp_io::TestExternalities;
use sp_runtime::{generic, traits::BlakeTwo256, BuildStorage, DispatchError};

// -- Mock runtime ---------------------------------------------------------------

#[crate::pallet(dev_mode)]
mod frame_system {
	#[allow(unused)]
	use super::frame_system;
	pub use crate::dispatch::RawOrigin;
	use crate::{pallet_prelude::*, traits::DispatchExtension};
	use pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: 'static {
		type Block: Parameter + sp_runtime::traits::Block;
		type AccountId;
		type BaseCallFilter: crate::traits::Contains<Self::RuntimeCall>;
		type RuntimeOrigin;
		type RuntimeCall: sp_runtime::traits::Dispatchable;
		type RuntimeTask;
		type PalletInfo: crate::traits::PalletInfo;
		type DbWeight: Get<crate::weights::RuntimeDbWeight>;
		type DispatchExtension: DispatchExtension<Self::RuntimeCall>;
	}

	#[pallet::error]
	pub enum Error<T> {
		CallFiltered,
	}

	#[pallet::origin]
	pub type Origin<T> = RawOrigin<<T as Config>::AccountId>;

	pub mod pallet_prelude {
		pub type OriginFor<T> = <T as super::Config>::RuntimeOrigin;

		pub type HeaderFor<T> =
			<<T as super::Config>::Block as sp_runtime::traits::HeaderProvider>::HeaderT;

		pub type BlockNumberFor<T> = <HeaderFor<T> as sp_runtime::traits::Header>::Number;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A no-op call that accepts any origin.
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn noop(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

		/// A call that always fails.
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn always_fails(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			Err(sp_runtime::DispatchError::Other("call failed").into())
		}
	}
}

type Header = generic::Header<u32, BlakeTwo256>;
type UncheckedExtrinsic = generic::UncheckedExtrinsic<u64, RuntimeCall, (), ()>;
type Block = generic::Block<Header, UncheckedExtrinsic>;

#[crate::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Runtime;

	#[runtime::pallet_index(0)]
	pub type System = self::frame_system;
}

impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = u64;
	type BaseCallFilter = crate::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = ();
	type PalletInfo = PalletInfo;
	type DbWeight = ();
	type DispatchExtension = (DispatchExt1, DispatchExt2, DispatchExt3);
}

fn new_test_ext() -> TestExternalities {
	RuntimeGenesisConfig::default().build_storage().unwrap().into()
}

// -- Mock extensions ------------------------------------------------------------

parameter_types! {
	pub static DispatchExt1ShouldFail: bool = false;
	pub static DispatchExt1StorageWrite: bool = false;
	pub static DispatchExt2ShouldFail: bool = false;
	pub static DispatchExt3ShouldFail: bool = false;
	pub static PostDispatchCalled: bool = false;
}

pub struct DispatchExt1;
impl DispatchExtension<RuntimeCall> for DispatchExt1 {
	type Pre = ();

	fn weight(_call: &RuntimeCall) -> crate::weights::Weight {
		crate::weights::Weight::from_parts(100, 0)
	}

	fn pre_dispatch(
		_origin: &RuntimeOrigin,
		_call: &RuntimeCall,
	) -> Result<Self::Pre, crate::dispatch::DispatchErrorWithPostInfo> {
		if DispatchExt1StorageWrite::get() {
			crate::storage::unhashed::put_raw(b"dispatch_ext_write", b"written");
		}
		if DispatchExt1ShouldFail::get() {
			return Err(DispatchError::Other("first guard rejected").into());
		}
		Ok(())
	}

	fn post_dispatch(_pre: Self::Pre, _result: &DispatchResultWithPostInfo) {}
}

pub struct DispatchExt2;
impl DispatchExtension<RuntimeCall> for DispatchExt2 {
	type Pre = ();

	fn weight(_call: &RuntimeCall) -> crate::weights::Weight {
		crate::weights::Weight::from_parts(200, 0)
	}

	fn pre_dispatch(
		_origin: &RuntimeOrigin,
		_call: &RuntimeCall,
	) -> Result<Self::Pre, crate::dispatch::DispatchErrorWithPostInfo> {
		if DispatchExt2ShouldFail::get() {
			return Err(DispatchError::Other("second guard rejected").into());
		}
		Ok(())
	}

	fn post_dispatch(_pre: Self::Pre, _result: &DispatchResultWithPostInfo) {}
}

pub struct DispatchExt3;
impl DispatchExtension<RuntimeCall> for DispatchExt3 {
	type Pre = ();

	fn weight(_call: &RuntimeCall) -> crate::weights::Weight {
		crate::weights::Weight::from_parts(50, 0)
	}

	fn pre_dispatch(
		_origin: &RuntimeOrigin,
		_call: &RuntimeCall,
	) -> Result<Self::Pre, crate::dispatch::DispatchErrorWithPostInfo> {
		if DispatchExt3ShouldFail::get() {
			return Err(DispatchError::Other("third guard rejected").into());
		}
		Ok(())
	}

	fn post_dispatch(_pre: Self::Pre, _result: &DispatchResultWithPostInfo) {
		PostDispatchCalled::set(true);
	}
}

// -- Tests ----------------------------------------------------------------------

type TestDispatchExtension = <Runtime as frame_system::Config>::DispatchExtension;

const CALL: &RuntimeCall = &RuntimeCall::System(frame_system::Call::noop {});

#[test]
fn pre_dispatch_passes_when_all_allow() {
	new_test_ext().execute_with(|| {
		let origin = RuntimeOrigin::signed(1);
		assert_ok!(TestDispatchExtension::pre_dispatch(&origin, CALL));
	});
}

#[test]
fn pre_dispatch_first_rejects() {
	new_test_ext().execute_with(|| {
		DispatchExt1ShouldFail::set(true);
		let origin = RuntimeOrigin::signed(1);
		assert_noop!(
			TestDispatchExtension::pre_dispatch(&origin, CALL),
			DispatchError::Other("first guard rejected")
		);
	});
}

#[test]
fn pre_dispatch_none_origin_rejects() {
	new_test_ext().execute_with(|| {
		DispatchExt1ShouldFail::set(true);
		let origin = RuntimeOrigin::none();
		assert_noop!(
			TestDispatchExtension::pre_dispatch(&origin, CALL),
			DispatchError::Other("first guard rejected")
		);
	});
}

#[test]
fn rolls_back_storage_writes() {
	new_test_ext().execute_with(|| {
		DispatchExt1StorageWrite::set(true);
		let origin = RuntimeOrigin::signed(1);
		let _ =
			<TestDispatchExtension as ExtendedDispatchable<RuntimeCall>>::dispatch_with_extension(
				origin,
				CALL.clone(),
			);
		assert!(!crate::storage::unhashed::exists(b"dispatch_ext_write"));
	});
}

#[test]
fn rolls_back_storage_writes_on_failure() {
	new_test_ext().execute_with(|| {
		DispatchExt1StorageWrite::set(true);
		DispatchExt1ShouldFail::set(true);
		let origin = RuntimeOrigin::signed(1);
		let _ =
			<TestDispatchExtension as ExtendedDispatchable<RuntimeCall>>::dispatch_with_extension(
				origin,
				CALL.clone(),
			);
		assert!(!crate::storage::unhashed::exists(b"dispatch_ext_write"));
	});
}

#[test]
fn tuple_second_rejects() {
	new_test_ext().execute_with(|| {
		DispatchExt2ShouldFail::set(true);
		let origin = RuntimeOrigin::signed(1);
		assert_noop!(
			TestDispatchExtension::pre_dispatch(&origin, CALL),
			DispatchError::Other("second guard rejected")
		);
	});
}

#[test]
fn tuple_third_rejects() {
	new_test_ext().execute_with(|| {
		DispatchExt3ShouldFail::set(true);
		let origin = RuntimeOrigin::signed(1);
		assert_noop!(
			TestDispatchExtension::pre_dispatch(&origin, CALL),
			DispatchError::Other("third guard rejected")
		);
	});
}

#[test]
fn tuple_first_short_circuits() {
	new_test_ext().execute_with(|| {
		DispatchExt1ShouldFail::set(true);
		DispatchExt2ShouldFail::set(true);
		DispatchExt3ShouldFail::set(true);
		let origin = RuntimeOrigin::signed(1);
		assert_noop!(
			TestDispatchExtension::pre_dispatch(&origin, CALL),
			DispatchError::Other("first guard rejected")
		);
	});
}

#[test]
fn post_dispatch_runs() {
	new_test_ext().execute_with(|| {
		let origin = RuntimeOrigin::signed(1);
		let pre = TestDispatchExtension::pre_dispatch(&origin, CALL).unwrap();
		PostDispatchCalled::set(false);
		let result: DispatchResultWithPostInfo = Ok(().into());
		TestDispatchExtension::post_dispatch(pre, &result);
		assert!(PostDispatchCalled::get());
	});
}

#[test]
fn post_dispatch_runs_on_failed_dispatch() {
	new_test_ext().execute_with(|| {
		let origin = RuntimeOrigin::signed(1);
		let pre = TestDispatchExtension::pre_dispatch(&origin, CALL).unwrap();
		PostDispatchCalled::set(false);
		let result: DispatchResultWithPostInfo = Err(DispatchError::Other("call failed").into());
		TestDispatchExtension::post_dispatch(pre, &result);
		assert!(PostDispatchCalled::get());
	});
}

#[test]
fn dispatch_with_extension_runs_full_flow() {
	new_test_ext().execute_with(|| {
		PostDispatchCalled::set(false);
		let origin = RuntimeOrigin::signed(1);
		let result =
			<TestDispatchExtension as ExtendedDispatchable<RuntimeCall>>::dispatch_with_extension(
				origin,
				CALL.clone(),
			);
		assert_ok!(&result);
		assert!(PostDispatchCalled::get());
	});
}

#[test]
fn dispatch_with_extension_post_dispatch_runs_on_failed_call() {
	new_test_ext().execute_with(|| {
		PostDispatchCalled::set(false);
		let origin = RuntimeOrigin::signed(1);
		let bad_call = RuntimeCall::System(frame_system::Call::always_fails {});
		let result =
			<TestDispatchExtension as ExtendedDispatchable<RuntimeCall>>::dispatch_with_extension(
				origin, bad_call,
			);
		assert!(result.is_err());
		assert!(PostDispatchCalled::get());
	});
}

#[test]
fn get_dispatch_info_includes_extension_weight() {
	use crate::dispatch::GetDispatchInfo;

	let call = RuntimeCall::System(frame_system::Call::noop {});
	let info = call.get_dispatch_info();
	// noop has weight 0, extensions add 100 + 200 + 50 = 350
	assert_eq!(info.call_weight, crate::weights::Weight::from_parts(350, 0));
}
