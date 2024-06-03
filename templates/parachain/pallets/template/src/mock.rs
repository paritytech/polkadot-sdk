use frame_support::{derive_impl, weights::constants::RocksDbWeight};
use frame_system::{mocking::MockBlock, GenesisConfig};
use sp_runtime::{traits::ConstU64, BuildStorage};

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		TemplateModule: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = MockBlock<Test>;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = RocksDbWeight;
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
