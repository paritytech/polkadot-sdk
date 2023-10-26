#![cfg(test)]

use frame_support::traits::EnsureOriginWithArg;
use frame_support::{
	construct_runtime,
	traits::{ConstU32, ConstU64, Everything},
};
use orml_traits::define_aggregrated_parameters;
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, BuildStorage};

use super::*;

use crate as parameters;

pub type AccountId = u128;

impl frame_system::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type BlockWeights = ();
	type BlockLength = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

pub mod pallet1 {
	orml_traits::define_parameters! {
		pub Parameters = {
			Key1: u64 = 0,
			Key2(u32): u32 = 1,
			Key3((u8, u8)): u128 = 2,
		}
	}
}
pub mod pallet2 {
	orml_traits::define_parameters! {
		pub Parameters = {
			Key1: u64 = 0,
			Key2(u32): u32 = 2,
			Key3((u8, u8)): u128 = 4,
		}
	}
}
define_aggregrated_parameters! {
	pub RuntimeParameters = {
		Pallet1: pallet1::Parameters = 0,
		Pallet2: pallet2::Parameters = 3,
	}
}

pub struct EnsureOriginImpl;

impl EnsureOriginWithArg<RuntimeOrigin, RuntimeParametersKey> for EnsureOriginImpl {
	type Success = ();

	fn try_origin(origin: RuntimeOrigin, key: &RuntimeParametersKey) -> Result<Self::Success, RuntimeOrigin> {
		match key {
			RuntimeParametersKey::Pallet1(_) => {
				ensure_root(origin.clone()).map_err(|_| origin)?;
				return Ok(());
			}
			RuntimeParametersKey::Pallet2(_) => {
				ensure_signed(origin.clone()).map_err(|_| origin)?;
				return Ok(());
			}
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(_key: &RuntimeParametersKey) -> Result<RuntimeOrigin, ()> {
		Err(())
	}
}

impl Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AggregratedKeyValue = RuntimeParameters;
	type AdminOrigin = EnsureOriginImpl;
	type WeightInfo = ();
}

type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		ModuleParameters: parameters,
	}
);

pub struct ExtBuilder;

impl ExtBuilder {
	pub fn new() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
