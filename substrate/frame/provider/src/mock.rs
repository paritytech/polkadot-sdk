#![cfg(test)]
use crate as pallet_provider;
use frame_support::{
    traits::{ConstU16, ConstU32, ConstU64},
    weights::Weight,
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};
use codec::Encode;

type Block = frame_system::mocking::MockBlock<Test>;

pub struct KingWeightInfo;
impl pallet_king::WeightInfo for KingWeightInfo {
    fn create_subnet() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn verify_provider() -> Weight {
        Weight::from_parts(10_000, 0)
    }
}

// Then subnet's weight info
pub struct SubnetWeightInfo;
impl pallet_subnet::WeightInfo for SubnetWeightInfo {
    fn create_subnet() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn add_provider() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn update_metrics() -> Weight {
        Weight::from_parts(10_000, 0)
    }
}

// And provider's weight info
pub struct ProviderWeightInfo;
impl pallet_provider::WeightInfo for ProviderWeightInfo {
    fn register_provider() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn update_resources() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn update_status() -> Weight {
        Weight::from_parts(10_000, 0)
    }
}


frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        King: pallet_king,
        Subnet: pallet_subnet,
        Provider: pallet_provider,
    }
);



impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeTask = RuntimeTask;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type Nonce = u64;
    type ExtensionsWeightInfo = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

impl pallet_king::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxTitleLength = ConstU32<100>;
    type MaxSubnetsPerKing = ConstU32<10>;
    type WeightInfo = KingWeightInfo;
}

impl pallet_subnet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxProvidersPerSubnet = ConstU32<100>;
    type WeightInfo = SubnetWeightInfo;
}

impl pallet_provider::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ProviderWeightInfo;
}




pub fn new_test_ext() -> sp_io::TestExternalities {
    frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap()
        .into()
}