use crate::{CollectionSelector, CreditHash, Selection, VoucherPublic, VoucherSignature};
use frame_support::{
	derive_impl, parameter_types,
	traits::{fungible::HoldConsideration, ConstU32, ConstU64, LinearStoragePrice, UnixTime},
	weights::{constants::RocksDbWeight, Weight},
	BoundedVec,
};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
	traits::{Identity, IdentityLookup},
	BuildStorage, DispatchError,
};
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Scarcity: pallet_scarcity,
		ScarcityClaims: crate,
	}
);

pub const OWNER: u64 = 1;
pub const RELAYER: u64 = 2;
pub const DESTINATION: u64 = 3;
pub const OTHER: u64 = 4;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = Block;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = RocksDbWeight;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u64;
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	pub static MockNow: u64 = 0;
}

pub struct MockUnixTime;
impl UnixTime for MockUnixTime {
	fn now() -> core::time::Duration {
		core::time::Duration::from_secs(MockNow::get())
	}
}

type TestStoragePrice = LinearStoragePrice<ConstU64<1>, ConstU64<1>, u64>;

parameter_types! {
	pub const ScarcityHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Scarcity(pallet_scarcity::HoldReason::StorageDeposit);
}

type TestConsideration = HoldConsideration<u64, Balances, ScarcityHoldReason, Identity, u64>;

impl pallet_scarcity::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type UnixTime = MockUnixTime;
	type Balance = u64;
	type Consideration = TestConsideration;
	type CollectionDeposit = TestStoragePrice;
	type ItemDeposit = TestStoragePrice;
	type InstanceDeposit = TestStoragePrice;
	type MetadataDeposit = TestStoragePrice;
	type MaxKeyLen = ConstU32<32>;
	type MaxValueLen = ConstU32<256>;
	type MaxInstanceMetadata = ConstU32<3>;
	type LockPeriod = ConstU64<60>;
	type MaxTransferPriority = ConstU64<1_000_000>;
}

thread_local! {
	static SELECTOR_ITEM: RefCell<u32> = const { RefCell::new(0) };
	static SELECTOR_OWNER: RefCell<Option<u64>> = const { RefCell::new(None) };
	static SELECTOR_FAILS: RefCell<bool> = const { RefCell::new(false) };
	static SELECTIONS: RefCell<Vec<(u64, u32, CreditHash)>> = const { RefCell::new(Vec::new()) };
	static REENTRANT_CLAIM: RefCell<Option<ReentrantClaim>> = const { RefCell::new(None) };
}

pub struct ReentrantClaim {
	pub root_id: u32,
	pub voucher: VoucherPublic,
	pub credit_hash: CreditHash,
	pub timestamp: u32,
	pub proof: BoundedVec<u8, <Test as crate::Config>::MaxProofLen>,
	pub collection: u32,
	pub destination: u64,
	pub signature: VoucherSignature,
}

pub struct MockSelector;
impl CollectionSelector<u64> for MockSelector {
	fn max_weight() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn select(
		collection_owner: &u64,
		collection: pallet_scarcity::CollectionId,
		entropy: CreditHash,
	) -> Result<Selection, DispatchError> {
		let expected_owner = SELECTOR_OWNER.with(|value| *value.borrow());
		if expected_owner.is_some_and(|expected| *collection_owner != expected) {
			return Err(DispatchError::Other("collection owner is not the mock contract"));
		}
		if SELECTOR_FAILS.with(|value| *value.borrow()) {
			return Err(DispatchError::Other("mock selector failed"));
		}
		if let Some(claim) = REENTRANT_CLAIM.with(|queued| queued.borrow_mut().take()) {
			if let Err(error) = ScarcityClaims::claim(
				RuntimeOrigin::signed(OTHER),
				claim.root_id,
				claim.voucher,
				claim.credit_hash,
				claim.timestamp,
				claim.proof,
				claim.collection,
				claim.destination,
				claim.signature,
			) {
				return Err(error.error);
			}
		}
		SELECTIONS.with(|calls| calls.borrow_mut().push((*collection_owner, collection, entropy)));
		let item = SELECTOR_ITEM.with(|value| *value.borrow());
		Ok(Selection { item, weight_consumed: Weight::from_parts(7, 0) })
	}
}

impl crate::Config for Test {
	type RootOrigin = frame_system::EnsureRoot<u64>;
	type CollectionSelector = MockSelector;
	type MaxProofLen = ConstU32<2048>;
	type MaxProofDepth = ConstU32<32>;
	type WeightInfo = ();
}

pub fn set_selector_item(item: u32) {
	SELECTOR_ITEM.with(|value| *value.borrow_mut() = item);
}

pub fn set_selector_owner(owner: u64) {
	SELECTOR_OWNER.with(|value| *value.borrow_mut() = Some(owner));
}

pub fn set_selector_fails(fails: bool) {
	SELECTOR_FAILS.with(|value| *value.borrow_mut() = fails);
}

pub fn set_reentrant_claim(claim: ReentrantClaim) {
	REENTRANT_CLAIM.with(|queued| *queued.borrow_mut() = Some(claim));
}

pub fn selections() -> Vec<(u64, u32, CreditHash)> {
	SELECTIONS.with(|calls| calls.borrow().clone())
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(OWNER, 1_000_000),
			(RELAYER, 1_000_000),
			(DESTINATION, 1_000_000),
			(OTHER, 1_000_000),
		],
		dev_accounts: None,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(storage);
	ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
	ext.execute_with(|| {
		System::set_block_number(1);
		MockNow::set(1_000);
		set_selector_item(0);
		SELECTOR_OWNER.with(|value| *value.borrow_mut() = None);
		set_selector_fails(false);
		SELECTIONS.with(|calls| calls.borrow_mut().clear());
		REENTRANT_CLAIM.with(|queued| *queued.borrow_mut() = None);
	});
	ext
}
