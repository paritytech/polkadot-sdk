// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod pallet_dummy;
mod test_debug;

use self::{
	test_debug::TestDebug,
	test_utils::{ensure_stored, expected_deposit},
};
use crate::{
	self as pallet_contracts,
	chain_extension::{
		ChainExtension, Environment, Ext, InitState, RegisteredChainExtension,
		Result as ExtensionResult, RetVal, ReturnFlags,
	},
	exec::{Frame, Key},
	migration::codegen::LATEST_MIGRATION_VERSION,
	primitives::CodeUploadReturnValue,
	storage::DeletionQueueManager,
	tests::test_utils::{get_contract, get_contract_checked},
	wasm::{Determinism, LoadingMode, ReturnErrorCode as RuntimeReturnCode},
	weights::WeightInfo,
	Array, BalanceOf, Code, CodeHash, CodeInfoOf, CollectEvents, Config, ContractInfo,
	ContractInfoOf, DebugInfo, DefaultAddressGenerator, DeletionQueueCounter, Error, HoldReason,
	MigrationInProgress, Origin, Pallet, PristineCode, Schedule,
};
use assert_matches::assert_matches;
use codec::{Decode, Encode};
use frame_support::{
	assert_err, assert_err_ignore_postinfo, assert_err_with_weight, assert_noop, assert_ok,
	derive_impl,
	dispatch::{DispatchErrorWithPostInfo, PostDispatchInfo},
	pallet_prelude::EnsureOrigin,
	parameter_types,
	storage::child,
	traits::{
		fungible::{BalancedHold, Inspect, Mutate, MutateHold},
		tokens::Preservation,
		ConstU32, ConstU64, Contains, OnIdle, OnInitialize, StorageVersion,
	},
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight, WeightMeter},
};
use frame_system::{EventRecord, Phase};
use pallet_contracts_fixtures::compile_module;
use pretty_assertions::{assert_eq, assert_ne};
use sp_core::ByteArray;
use sp_io::hashing::blake2_256;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
	testing::H256,
	traits::{BlakeTwo256, Convert, IdentityLookup},
	AccountId32, BuildStorage, DispatchError, Perbill, TokenError,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,
		Randomness: pallet_insecure_randomness_collective_flip,
		Utility: pallet_utility,
		Contracts: pallet_contracts,
		Proxy: pallet_proxy,
		Dummy: pallet_dummy
	}
);

macro_rules! assert_return_code {
	( $x:expr , $y:expr $(,)? ) => {{
		assert_eq!(u32::from_le_bytes($x.data[..].try_into().unwrap()), $y as u32);
	}};
}

macro_rules! assert_refcount {
	( $code_hash:expr , $should:expr $(,)? ) => {{
		let is = crate::CodeInfoOf::<Test>::get($code_hash).map(|m| m.refcount()).unwrap();
		assert_eq!(is, $should);
	}};
}

pub mod test_utils {
	use super::{Contracts, DepositPerByte, DepositPerItem, Test};
	use crate::{
		exec::AccountIdOf, BalanceOf, CodeHash, CodeInfo, CodeInfoOf, Config, ContractInfo,
		ContractInfoOf, Nonce, PristineCode,
	};
	use codec::{Encode, MaxEncodedLen};
	use frame_support::traits::fungible::{InspectHold, Mutate};

	pub fn place_contract(address: &AccountIdOf<Test>, code_hash: CodeHash<Test>) {
		let nonce = <Nonce<Test>>::mutate(|counter| {
			*counter += 1;
			*counter
		});
		set_balance(address, Contracts::min_balance() * 10);
		<CodeInfoOf<Test>>::insert(code_hash, CodeInfo::new(address.clone()));
		let contract = <ContractInfo<Test>>::new(&address, nonce, code_hash).unwrap();
		<ContractInfoOf<Test>>::insert(address, contract);
	}
	pub fn set_balance(who: &AccountIdOf<Test>, amount: u64) {
		let _ = <Test as Config>::Currency::set_balance(who, amount);
	}
	pub fn get_balance(who: &AccountIdOf<Test>) -> u64 {
		<Test as Config>::Currency::free_balance(who)
	}
	pub fn get_balance_on_hold(
		reason: &<Test as Config>::RuntimeHoldReason,
		who: &AccountIdOf<Test>,
	) -> u64 {
		<Test as Config>::Currency::balance_on_hold(reason.into(), who)
	}
	pub fn get_contract(addr: &AccountIdOf<Test>) -> ContractInfo<Test> {
		get_contract_checked(addr).unwrap()
	}
	pub fn get_contract_checked(addr: &AccountIdOf<Test>) -> Option<ContractInfo<Test>> {
		ContractInfoOf::<Test>::get(addr)
	}
	pub fn get_code_deposit(code_hash: &CodeHash<Test>) -> BalanceOf<Test> {
		crate::CodeInfoOf::<Test>::get(code_hash).unwrap().deposit()
	}
	pub fn contract_info_storage_deposit(
		addr: &<Test as frame_system::Config>::AccountId,
	) -> BalanceOf<Test> {
		let contract_info = self::get_contract(&addr);
		let info_size = contract_info.encoded_size() as u64;
		DepositPerByte::get()
			.saturating_mul(info_size)
			.saturating_add(DepositPerItem::get())
	}
	pub fn expected_deposit(code_len: usize) -> u64 {
		// For code_info, the deposit for max_encoded_len is taken.
		let code_info_len = CodeInfo::<Test>::max_encoded_len() as u64;
		// Calculate deposit to be reserved.
		// We add 2 storage items: one for code, other for code_info
		DepositPerByte::get().saturating_mul(code_len as u64 + code_info_len) +
			DepositPerItem::get().saturating_mul(2)
	}
	pub fn ensure_stored(code_hash: CodeHash<Test>) -> usize {
		// Assert that code_info is stored
		assert!(CodeInfoOf::<Test>::contains_key(&code_hash));
		// Assert that contract code is stored, and get its size.
		PristineCode::<Test>::try_get(&code_hash).unwrap().len()
	}
}

mod builder {
	use super::Test;
	use crate::{
		test_utils::{builder::*, AccountId32, ALICE},
		tests::RuntimeOrigin,
		AccountIdLookupOf, Code, CodeHash,
	};

	pub fn bare_instantiate(code: Code<CodeHash<Test>>) -> BareInstantiateBuilder<Test> {
		BareInstantiateBuilder::<Test>::bare_instantiate(ALICE, code)
	}

	pub fn bare_call(dest: AccountId32) -> BareCallBuilder<Test> {
		BareCallBuilder::<Test>::bare_call(ALICE, dest)
	}

	pub fn instantiate_with_code(code: Vec<u8>) -> InstantiateWithCodeBuilder<Test> {
		InstantiateWithCodeBuilder::<Test>::instantiate_with_code(
			RuntimeOrigin::signed(ALICE),
			code,
		)
	}

	pub fn instantiate(code_hash: CodeHash<Test>) -> InstantiateBuilder<Test> {
		InstantiateBuilder::<Test>::instantiate(RuntimeOrigin::signed(ALICE), code_hash)
	}

	pub fn call(dest: AccountIdLookupOf<Test>) -> CallBuilder<Test> {
		CallBuilder::<Test>::call(RuntimeOrigin::signed(ALICE), dest)
	}
}

impl Test {
	pub fn set_unstable_interface(unstable_interface: bool) {
		UNSTABLE_INTERFACE.with(|v| *v.borrow_mut() = unstable_interface);
	}
}

parameter_types! {
	static TestExtensionTestValue: TestExtension = Default::default();
}

#[derive(Clone)]
pub struct TestExtension {
	enabled: bool,
	last_seen_buffer: Vec<u8>,
	last_seen_input_len: u32,
}

#[derive(Default)]
pub struct RevertingExtension;

#[derive(Default)]
pub struct DisabledExtension;

#[derive(Default)]
pub struct TempStorageExtension {
	storage: u32,
}

impl TestExtension {
	fn disable() {
		TestExtensionTestValue::mutate(|e| e.enabled = false)
	}

	fn last_seen_buffer() -> Vec<u8> {
		TestExtensionTestValue::get().last_seen_buffer.clone()
	}

	fn last_seen_input_len() -> u32 {
		TestExtensionTestValue::get().last_seen_input_len
	}
}

impl Default for TestExtension {
	fn default() -> Self {
		Self { enabled: true, last_seen_buffer: vec![], last_seen_input_len: 0 }
	}
}

impl ChainExtension<Test> for TestExtension {
	fn call<E>(&mut self, env: Environment<E, InitState>) -> ExtensionResult<RetVal>
	where
		E: Ext<T = Test>,
	{
		let func_id = env.func_id();
		let id = env.ext_id() as u32 | func_id as u32;
		match func_id {
			0 => {
				let mut env = env.buf_in_buf_out();
				let input = env.read(8)?;
				env.write(&input, false, None)?;
				TestExtensionTestValue::mutate(|e| e.last_seen_buffer = input);
				Ok(RetVal::Converging(id))
			},
			1 => {
				let env = env.only_in();
				TestExtensionTestValue::mutate(|e| e.last_seen_input_len = env.val1());
				Ok(RetVal::Converging(id))
			},
			2 => {
				let mut env = env.buf_in_buf_out();
				let mut enc = &env.read(9)?[4..8];
				let weight = Weight::from_parts(
					u32::decode(&mut enc).map_err(|_| Error::<Test>::ContractTrapped)?.into(),
					0,
				);
				env.charge_weight(weight)?;
				Ok(RetVal::Converging(id))
			},
			3 => Ok(RetVal::Diverging { flags: ReturnFlags::REVERT, data: vec![42, 99] }),
			_ => {
				panic!("Passed unknown id to test chain extension: {}", func_id);
			},
		}
	}

	fn enabled() -> bool {
		TestExtensionTestValue::get().enabled
	}
}

impl RegisteredChainExtension<Test> for TestExtension {
	const ID: u16 = 0;
}

impl ChainExtension<Test> for RevertingExtension {
	fn call<E>(&mut self, _env: Environment<E, InitState>) -> ExtensionResult<RetVal>
	where
		E: Ext<T = Test>,
	{
		Ok(RetVal::Diverging { flags: ReturnFlags::REVERT, data: vec![0x4B, 0x1D] })
	}

	fn enabled() -> bool {
		TestExtensionTestValue::get().enabled
	}
}

impl RegisteredChainExtension<Test> for RevertingExtension {
	const ID: u16 = 1;
}

impl ChainExtension<Test> for DisabledExtension {
	fn call<E>(&mut self, _env: Environment<E, InitState>) -> ExtensionResult<RetVal>
	where
		E: Ext<T = Test>,
	{
		panic!("Disabled chain extensions are never called")
	}

	fn enabled() -> bool {
		false
	}
}

impl RegisteredChainExtension<Test> for DisabledExtension {
	const ID: u16 = 2;
}

impl ChainExtension<Test> for TempStorageExtension {
	fn call<E>(&mut self, env: Environment<E, InitState>) -> ExtensionResult<RetVal>
	where
		E: Ext<T = Test>,
	{
		let func_id = env.func_id();
		match func_id {
			0 => self.storage = 42,
			1 => assert_eq!(self.storage, 42, "Storage is preserved inside the same call."),
			2 => {
				assert_eq!(self.storage, 0, "Storage is different for different calls.");
				self.storage = 99;
			},
			3 => assert_eq!(self.storage, 99, "Storage is preserved inside the same call."),
			_ => {
				panic!("Passed unknown id to test chain extension: {}", func_id);
			},
		}
		Ok(RetVal::Converging(0))
	}

	fn enabled() -> bool {
		TestExtensionTestValue::get().enabled
	}
}

impl RegisteredChainExtension<Test> for TempStorageExtension {
	const ID: u16 = 3;
}

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(
			Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		);
	pub static ExistentialDeposit: u64 = 1;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<u64>;
}

impl pallet_insecure_randomness_collective_flip::Config for Test {}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ExistentialDeposit = ExistentialDeposit;
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

impl pallet_proxy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ProxyType = ();
	type ProxyDepositBase = ConstU64<1>;
	type ProxyDepositFactor = ConstU64<1>;
	type MaxProxies = ConstU32<32>;
	type WeightInfo = ();
	type MaxPending = ConstU32<32>;
	type CallHasher = BlakeTwo256;
	type AnnouncementDepositBase = ConstU64<1>;
	type AnnouncementDepositFactor = ConstU64<1>;
}

impl pallet_dummy::Config for Test {}

parameter_types! {
	pub MySchedule: Schedule<Test> = {
		let schedule = <Schedule<Test>>::default();
		schedule
	};
	pub static DepositPerByte: BalanceOf<Test> = 1;
	pub const DepositPerItem: BalanceOf<Test> = 2;
	pub static MaxDelegateDependencies: u32 = 32;

	pub static CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(0);
	// We need this one set high enough for running benchmarks.
	pub static DefaultDepositLimit: BalanceOf<Test> = 10_000_000;
}

impl Convert<Weight, BalanceOf<Self>> for Test {
	fn convert(w: Weight) -> BalanceOf<Self> {
		w.ref_time()
	}
}

/// A filter whose filter function can be swapped at runtime.
pub struct TestFilter;

#[derive(Clone)]
pub struct Filters {
	filter: fn(&RuntimeCall) -> bool,
}

impl Default for Filters {
	fn default() -> Self {
		Filters { filter: (|_| true) }
	}
}

parameter_types! {
	static CallFilter: Filters = Default::default();
}

impl TestFilter {
	pub fn set_filter(filter: fn(&RuntimeCall) -> bool) {
		CallFilter::mutate(|fltr| fltr.filter = filter);
	}
}

impl Contains<RuntimeCall> for TestFilter {
	fn contains(call: &RuntimeCall) -> bool {
		(CallFilter::get().filter)(call)
	}
}

parameter_types! {
	pub static UploadAccount: Option<<Test as frame_system::Config>::AccountId> = None;
	pub static InstantiateAccount: Option<<Test as frame_system::Config>::AccountId> = None;
}

pub struct EnsureAccount<T, A>(sp_std::marker::PhantomData<(T, A)>);
impl<T: Config, A: sp_core::Get<Option<crate::AccountIdOf<T>>>>
	EnsureOrigin<<T as frame_system::Config>::RuntimeOrigin> for EnsureAccount<T, A>
where
	<T as frame_system::Config>::AccountId: From<AccountId32>,
{
	type Success = T::AccountId;

	fn try_origin(o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
		let who = <frame_system::EnsureSigned<_> as EnsureOrigin<_>>::try_origin(o.clone())?;
		if matches!(A::get(), Some(a) if who != a) {
			return Err(o)
		}

		Ok(who)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<T::RuntimeOrigin, ()> {
		Err(())
	}
}
parameter_types! {
	pub static UnstableInterface: bool = true;
}

#[derive_impl(crate::config_preludes::TestDefaultConfig)]
impl Config for Test {
	type Time = Timestamp;
	type Randomness = Randomness;
	type Currency = Balances;
	type CallFilter = TestFilter;
	type CallStack = [Frame<Self>; 5];
	type ChainExtension =
		(TestExtension, DisabledExtension, RevertingExtension, TempStorageExtension);
	type Schedule = MySchedule;
	type DepositPerByte = DepositPerByte;
	type DepositPerItem = DepositPerItem;
	type DefaultDepositLimit = DefaultDepositLimit;
	type AddressGenerator = DefaultAddressGenerator;
	type UnsafeUnstableInterface = UnstableInterface;
	type UploadOrigin = EnsureAccount<Self, UploadAccount>;
	type InstantiateOrigin = EnsureAccount<Self, InstantiateAccount>;
	type Migrations = crate::migration::codegen::BenchMigrations;
	type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
	type MaxDelegateDependencies = MaxDelegateDependencies;
	type Debug = TestDebug;
}

pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const CHARLIE: AccountId32 = AccountId32::new([3u8; 32]);
pub const DJANGO: AccountId32 = AccountId32::new([4u8; 32]);

pub const GAS_LIMIT: Weight = Weight::from_parts(100_000_000_000, 3 * 1024 * 1024);

pub struct ExtBuilder {
	existential_deposit: u64,
	storage_version: Option<StorageVersion>,
	code_hashes: Vec<CodeHash<Test>>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			existential_deposit: ExistentialDeposit::get(),
			storage_version: None,
			code_hashes: vec![],
		}
	}
}

impl ExtBuilder {
	pub fn existential_deposit(mut self, existential_deposit: u64) -> Self {
		self.existential_deposit = existential_deposit;
		self
	}
	pub fn with_code_hashes(mut self, code_hashes: Vec<CodeHash<Test>>) -> Self {
		self.code_hashes = code_hashes;
		self
	}
	pub fn set_associated_consts(&self) {
		EXISTENTIAL_DEPOSIT.with(|v| *v.borrow_mut() = self.existential_deposit);
	}
	pub fn set_storage_version(mut self, version: u16) -> Self {
		self.storage_version = Some(StorageVersion::new(version));
		self
	}
	pub fn build(self) -> sp_io::TestExternalities {
		use env_logger::{Builder, Env};
		let env = Env::new().default_filter_or("runtime=debug");
		let _ = Builder::from_env(env).is_test(true).try_init();
		self.set_associated_consts();
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Test> { balances: vec![] }
			.assimilate_storage(&mut t)
			.unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
		ext.execute_with(|| {
			use frame_support::traits::OnGenesis;

			Pallet::<Test>::on_genesis();
			if let Some(storage_version) = self.storage_version {
				storage_version.put::<Pallet<Test>>();
			}
			System::set_block_number(1)
		});
		ext.execute_with(|| {
			for code_hash in self.code_hashes {
				CodeInfoOf::<Test>::insert(code_hash, crate::CodeInfo::new(ALICE));
			}
		});
		ext
	}
}

fn initialize_block(number: u64) {
	System::reset_events();
	System::initialize(&number, &[0u8; 32].into(), &Default::default());
}

struct ExtensionInput<'a> {
	extension_id: u16,
	func_id: u16,
	extra: &'a [u8],
}

impl<'a> ExtensionInput<'a> {
	fn to_vec(&self) -> Vec<u8> {
		((self.extension_id as u32) << 16 | (self.func_id as u32))
			.to_le_bytes()
			.iter()
			.chain(self.extra)
			.cloned()
			.collect()
	}
}

impl<'a> From<ExtensionInput<'a>> for Vec<u8> {
	fn from(input: ExtensionInput) -> Vec<u8> {
		input.to_vec()
	}
}

impl Default for Origin<Test> {
	fn default() -> Self {
		Self::Signed(ALICE)
	}
}
// Perform a call to a plain account.
// The actual transfer fails because we can only call contracts.
// Then we check that at least the base costs where charged (no runtime gas costs.)
#[test]
fn calling_plain_account_fails() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000);
		let base_cost = <<Test as Config>::WeightInfo as WeightInfo>::call();

		assert_eq!(
			builder::call(BOB).build(),
			Err(DispatchErrorWithPostInfo {
				error: Error::<Test>::ContractNotFound.into(),
				post_info: PostDispatchInfo {
					actual_weight: Some(base_cost),
					pays_fee: Default::default(),
				},
			})
		);
	});
}

#[test]
fn migration_on_idle_hooks_works() {
	// Defines expectations of how many migration steps can be done given the weight limit.
	let tests = [
		(Weight::zero(), LATEST_MIGRATION_VERSION - 2),
		(<Test as Config>::WeightInfo::migrate() + 1.into(), LATEST_MIGRATION_VERSION - 1),
		(Weight::MAX, LATEST_MIGRATION_VERSION),
	];

	for (weight, expected_version) in tests {
		ExtBuilder::default()
			.set_storage_version(LATEST_MIGRATION_VERSION - 2)
			.build()
			.execute_with(|| {
				MigrationInProgress::<Test>::set(Some(Default::default()));
				Contracts::on_idle(System::block_number(), weight);
				assert_eq!(StorageVersion::get::<Pallet<Test>>(), expected_version);
			});
	}
}

#[test]
fn migration_in_progress_works() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		MigrationInProgress::<Test>::set(Some(Default::default()));

		assert_err!(
			Contracts::upload_code(
				RuntimeOrigin::signed(ALICE),
				vec![],
				None,
				Determinism::Enforced
			),
			Error::<Test>::MigrationInProgress,
		);
		assert_err!(
			Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash),
			Error::<Test>::MigrationInProgress,
		);
		assert_err!(
			Contracts::set_code(RuntimeOrigin::signed(ALICE), BOB.clone(), code_hash),
			Error::<Test>::MigrationInProgress,
		);
		assert_err_ignore_postinfo!(builder::call(BOB).build(), Error::<Test>::MigrationInProgress);
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(wasm).value(100_000).build(),
			Error::<Test>::MigrationInProgress,
		);
		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).value(100_000).build(),
			Error::<Test>::MigrationInProgress,
		);
	});
}

#[test]
fn instantiate_and_call_and_deposit_event() {
	let (wasm, code_hash) = compile_module::<Test>("event_and_return_on_deploy").unwrap();

	ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();
		let value = 100;

		// We determine the storage deposit limit after uploading because it depends on ALICEs free
		// balance which is changed by uploading a module.
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			None,
			Determinism::Enforced
		));

		// Drop previous events
		initialize_block(2);

		// Check at the end to get hash on error easily
		let addr = builder::bare_instantiate(Code::Existing(code_hash))
			.value(value)
			.build_and_unwrap_account_id();
		assert!(ContractInfoOf::<Test>::contains_key(&addr));

		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::System(frame_system::Event::NewAccount {
						account: addr.clone()
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Endowed {
						account: addr.clone(),
						free_balance: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: addr.clone(),
						amount: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: addr.clone(),
						amount: value,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::ContractEmitted {
						contract: addr.clone(),
						data: vec![1, 2, 3, 4]
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Instantiated {
						deployer: ALICE,
						contract: addr.clone()
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndHeld {
							from: ALICE,
							to: addr.clone(),
							amount: test_utils::contract_info_storage_deposit(&addr),
						}
					),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn deposit_event_max_value_limit() {
	let (wasm, _code_hash) = compile_module::<Test>("event_size").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		// Create
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(30_000)
			.build_and_unwrap_account_id();

		// Call contract with allowed storage value.
		assert_ok!(builder::call(addr.clone())
			.gas_limit(GAS_LIMIT.set_ref_time(GAS_LIMIT.ref_time() * 2)) // we are copying a huge buffer,
			.data(<Test as Config>::Schedule::get().limits.payload_len.encode())
			.build());

		// Call contract with too large a storage value.
		assert_err_ignore_postinfo!(
			builder::call(addr)
				.data((<Test as Config>::Schedule::get().limits.payload_len + 1).encode())
				.build(),
			Error::<Test>::ValueTooLarge,
		);
	});
}

// Fail out of fuel (ref_time weight) in the engine.
#[test]
fn run_out_of_fuel_engine() {
	let (wasm, _code_hash) = compile_module::<Test>("run_out_of_gas").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(100 * min_balance)
			.build_and_unwrap_account_id();

		// Call the contract with a fixed gas limit. It must run out of gas because it just
		// loops forever.
		assert_err_ignore_postinfo!(
			builder::call(addr)
				.gas_limit(Weight::from_parts(1_000_000_000_000, u64::MAX))
				.build(),
			Error::<Test>::OutOfGas,
		);
	});
}

// Fail out of fuel (ref_time weight) in the host.
#[test]
fn run_out_of_fuel_host() {
	let (code, _hash) = compile_module::<Test>("chain_extension").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let gas_limit = Weight::from_parts(u32::MAX as u64, GAS_LIMIT.proof_size());

		// Use chain extension to charge more ref_time than it is available.
		let result = builder::bare_call(addr.clone())
			.gas_limit(gas_limit)
			.data(ExtensionInput { extension_id: 0, func_id: 2, extra: &u32::MAX.encode() }.into())
			.build()
			.result;
		assert_err!(result, <Error<Test>>::OutOfGas);
	});
}

#[test]
fn gas_syncs_work() {
	let (code, _code_hash) = compile_module::<Test>("caller_is_origin_n").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let addr = builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_account_id();

		let result = builder::bare_call(addr.clone()).data(0u32.encode()).build();
		assert_ok!(result.result);
		let engine_consumed_noop = result.gas_consumed.ref_time();

		let result = builder::bare_call(addr.clone()).data(1u32.encode()).build();
		assert_ok!(result.result);
		let gas_consumed_once = result.gas_consumed.ref_time();
		let host_consumed_once = <Test as Config>::WeightInfo::seal_caller_is_origin().ref_time();
		let engine_consumed_once = gas_consumed_once - host_consumed_once - engine_consumed_noop;

		let result = builder::bare_call(addr).data(2u32.encode()).build();
		assert_ok!(result.result);
		let gas_consumed_twice = result.gas_consumed.ref_time();
		let host_consumed_twice = host_consumed_once * 2;
		let engine_consumed_twice = gas_consumed_twice - host_consumed_twice - engine_consumed_noop;

		// Second contract just repeats first contract's instructions twice.
		// If runtime syncs gas with the engine properly, this should pass.
		assert_eq!(engine_consumed_twice, engine_consumed_once * 2);
	});
}

/// Check that contracts with the same account id have different trie ids.
/// Check the `Nonce` storage item for more information.
#[test]
fn instantiate_unique_trie_id() {
	let (wasm, code_hash) = compile_module::<Test>("self_destruct").unwrap();

	ExtBuilder::default().existential_deposit(500).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		Contracts::upload_code(RuntimeOrigin::signed(ALICE), wasm, None, Determinism::Enforced)
			.unwrap();

		// Instantiate the contract and store its trie id for later comparison.
		let addr =
			builder::bare_instantiate(Code::Existing(code_hash)).build_and_unwrap_account_id();
		let trie_id = get_contract(&addr).trie_id;

		// Try to instantiate it again without termination should yield an error.
		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).build(),
			<Error<Test>>::DuplicateContract,
		);

		// Terminate the contract.
		assert_ok!(builder::call(addr.clone()).build());

		// Re-Instantiate after termination.
		assert_ok!(builder::instantiate(code_hash).build());

		// Trie ids shouldn't match or we might have a collision
		assert_ne!(trie_id, get_contract(&addr).trie_id);
	});
}

#[test]
fn storage_work() {
	let (code, _code_hash) = compile_module::<Test>("storage").unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();
		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		builder::bare_call(addr).build_and_unwrap_result();
	});
}

#[test]
fn storage_max_value_limit() {
	let (wasm, _code_hash) = compile_module::<Test>("storage_size").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		// Create
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(30_000)
			.build_and_unwrap_account_id();
		get_contract(&addr);

		// Call contract with allowed storage value.
		assert_ok!(builder::call(addr.clone())
			.gas_limit(GAS_LIMIT.set_ref_time(GAS_LIMIT.ref_time() * 2)) // we are copying a huge buffer
			.data(<Test as Config>::Schedule::get().limits.payload_len.encode())
			.build());

		// Call contract with too large a storage value.
		assert_err_ignore_postinfo!(
			builder::call(addr)
				.data((<Test as Config>::Schedule::get().limits.payload_len + 1).encode())
				.build(),
			Error::<Test>::ValueTooLarge,
		);
	});
}

#[test]
fn deploy_and_call_other_contract() {
	let (caller_wasm, _caller_code_hash) = compile_module::<Test>("caller_contract").unwrap();
	let (callee_wasm, callee_code_hash) = compile_module::<Test>("return_with_data").unwrap();

	ExtBuilder::default().existential_deposit(1).build().execute_with(|| {
		let min_balance = Contracts::min_balance();

		// Create
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let caller_addr = builder::bare_instantiate(Code::Upload(caller_wasm))
			.value(100_000)
			.build_and_unwrap_account_id();
		Contracts::bare_upload_code(ALICE, callee_wasm, None, Determinism::Enforced).unwrap();

		let callee_addr = Contracts::contract_address(
			&caller_addr,
			&callee_code_hash,
			&[0, 1, 34, 51, 68, 85, 102, 119], // hard coded in wasm
			&[],
		);

		// Drop previous events
		initialize_block(2);

		// Call BOB contract, which attempts to instantiate and call the callee contract and
		// makes various assertions on the results from those calls.
		assert_ok!(builder::call(caller_addr.clone())
			.data(callee_code_hash.as_ref().to_vec())
			.build());

		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::System(frame_system::Event::NewAccount {
						account: callee_addr.clone()
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Endowed {
						account: callee_addr.clone(),
						free_balance: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: callee_addr.clone(),
						amount: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: caller_addr.clone(),
						to: callee_addr.clone(),
						amount: 32768 // hardcoded in wasm
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Instantiated {
						deployer: caller_addr.clone(),
						contract: callee_addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: caller_addr.clone(),
						to: callee_addr.clone(),
						amount: 32768,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(caller_addr.clone()),
						contract: callee_addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: caller_addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndHeld {
							from: ALICE,
							to: callee_addr.clone(),
							amount: test_utils::contract_info_storage_deposit(&callee_addr),
						}
					),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn delegate_call() {
	let (caller_wasm, _caller_code_hash) = compile_module::<Test>("delegate_call").unwrap();
	let (callee_wasm, callee_code_hash) = compile_module::<Test>("delegate_call_lib").unwrap();

	ExtBuilder::default().existential_deposit(500).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the 'caller'
		let caller_addr = builder::bare_instantiate(Code::Upload(caller_wasm))
			.value(300_000)
			.build_and_unwrap_account_id();
		// Only upload 'callee' code
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			callee_wasm,
			Some(codec::Compact(100_000)),
			Determinism::Enforced,
		));

		assert_ok!(builder::call(caller_addr.clone())
			.value(1337)
			.data(callee_code_hash.as_ref().to_vec())
			.build());
	});
}

#[test]
fn track_check_uncheck_module_call() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		Contracts::bare_upload_code(ALICE, wasm, None, Determinism::Enforced).unwrap();
		builder::bare_instantiate(Code::Existing(code_hash)).build_and_unwrap_result();
	});

	// It should have recorded 1 `Checked` for the upload and 1 `Unchecked` for the instantiate.
	let record = crate::wasm::tracker::LOADED_MODULE.with(|stack| stack.borrow().clone());
	assert_eq!(record, vec![LoadingMode::Checked, LoadingMode::Unchecked]);
}

#[test]
fn transfer_expendable_cannot_kill_account() {
	let (wasm, _code_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(1_000)
			.build_and_unwrap_account_id();

		// Check that the BOB contract has been instantiated.
		get_contract(&addr);

		let total_balance = <Test as Config>::Currency::total_balance(&addr);

		assert_eq!(
			test_utils::get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &addr),
			test_utils::contract_info_storage_deposit(&addr)
		);

		// Some ot the total balance is held, so it can't be transferred.
		assert_err!(
			<<Test as Config>::Currency as Mutate<AccountId32>>::transfer(
				&addr,
				&ALICE,
				total_balance,
				Preservation::Expendable,
			),
			TokenError::FundsUnavailable,
		);

		assert_eq!(<Test as Config>::Currency::total_balance(&addr), total_balance);
	});
}

#[test]
fn cannot_self_destruct_through_draining() {
	let (wasm, _code_hash) = compile_module::<Test>("drain").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let value = 1_000;
		let min_balance = Contracts::min_balance();

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(value)
			.build_and_unwrap_account_id();

		// Check that the BOB contract has been instantiated.
		get_contract(&addr);

		// Call BOB which makes it send all funds to the zero address
		// The contract code asserts that the transfer fails with the correct error code
		assert_ok!(builder::call(addr.clone()).build());

		// Make sure the account wasn't remove by sending all free balance away.
		assert_eq!(
			<Test as Config>::Currency::total_balance(&addr),
			value + test_utils::contract_info_storage_deposit(&addr) + min_balance,
		);
	});
}

#[test]
fn cannot_self_destruct_through_storage_refund_after_price_change() {
	let (wasm, _code_hash) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);

		// Check that the contract has been instantiated and has the minimum balance
		assert_eq!(get_contract(&addr).total_deposit(), info_deposit);
		assert_eq!(get_contract(&addr).extra_deposit(), 0);
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), info_deposit + min_balance);

		// Create 100 bytes of storage with a price of per byte and a single storage item of price 2
		assert_ok!(builder::call(addr.clone()).data(100u32.to_le_bytes().to_vec()).build());
		assert_eq!(get_contract(&addr).total_deposit(), info_deposit + 102);

		// Increase the byte price and trigger a refund. This should not have any influence because
		// the removal is pro rata and exactly those 100 bytes should have been removed.
		DEPOSIT_PER_BYTE.with(|c| *c.borrow_mut() = 500);
		assert_ok!(builder::call(addr.clone()).data(0u32.to_le_bytes().to_vec()).build());

		// Make sure the account wasn't removed by the refund
		assert_eq!(
			<Test as Config>::Currency::total_balance(&addr),
			get_contract(&addr).total_deposit() + min_balance,
		);
		assert_eq!(get_contract(&addr).extra_deposit(), 2);
	});
}

#[test]
fn cannot_self_destruct_while_live() {
	let (wasm, _code_hash) = compile_module::<Test>("self_destruct").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(100_000)
			.build_and_unwrap_account_id();

		// Check that the BOB contract has been instantiated.
		get_contract(&addr);

		// Call BOB with input data, forcing it make a recursive call to itself to
		// self-destruct, resulting in a trap.
		assert_err_ignore_postinfo!(
			builder::call(addr.clone()).data(vec![0]).build(),
			Error::<Test>::ContractTrapped,
		);

		// Check that BOB is still there.
		get_contract(&addr);
	});
}

#[test]
fn self_destruct_works() {
	let (wasm, code_hash) = compile_module::<Test>("self_destruct").unwrap();
	ExtBuilder::default().existential_deposit(1_000).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let _ = <Test as Config>::Currency::set_balance(&DJANGO, 1_000_000);
		let min_balance = Contracts::min_balance();

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(100_000)
			.build_and_unwrap_account_id();

		// Check that the BOB contract has been instantiated.
		let _ = get_contract(&addr);

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);

		// Drop all previous events
		initialize_block(2);

		// Call BOB without input data which triggers termination.
		assert_matches!(builder::call(addr.clone()).build(), Ok(_));

		// Check that code is still there but refcount dropped to zero.
		assert_refcount!(&code_hash, 0);

		// Check that account is gone
		assert!(get_contract_checked(&addr).is_none());
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), 0);

		// Check that the beneficiary (django) got remaining balance.
		assert_eq!(
			<Test as Config>::Currency::free_balance(DJANGO),
			1_000_000 + 100_000 + min_balance
		);

		// Check that the Alice is missing Django's benefit. Within ALICE's total balance there's
		// also the code upload deposit held.
		assert_eq!(
			<Test as Config>::Currency::total_balance(&ALICE),
			1_000_000 - (100_000 + min_balance)
		);

		pretty_assertions::assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Terminated {
						contract: addr.clone(),
						beneficiary: DJANGO
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndReleased {
							from: addr.clone(),
							to: ALICE,
							amount: info_deposit,
						}
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::System(frame_system::Event::KilledAccount {
						account: addr.clone()
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: addr.clone(),
						to: DJANGO,
						amount: 100_000 + min_balance,
					}),
					topics: vec![],
				},
			],
		);
	});
}

// This tests that one contract cannot prevent another from self-destructing by sending it
// additional funds after it has been drained.
#[test]
fn destroy_contract_and_transfer_funds() {
	let (callee_wasm, callee_code_hash) = compile_module::<Test>("self_destruct").unwrap();
	let (caller_wasm, _caller_code_hash) = compile_module::<Test>("destroy_and_transfer").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		// Create code hash for bob to instantiate
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		Contracts::bare_upload_code(ALICE, callee_wasm, None, Determinism::Enforced).unwrap();

		// This deploys the BOB contract, which in turn deploys the CHARLIE contract during
		// construction.
		let addr_bob = builder::bare_instantiate(Code::Upload(caller_wasm))
			.value(200_000)
			.data(callee_code_hash.as_ref().to_vec())
			.build_and_unwrap_account_id();

		// Check that the CHARLIE contract has been instantiated.
		let addr_charlie =
			Contracts::contract_address(&addr_bob, &callee_code_hash, &[], &[0x47, 0x11]);
		get_contract(&addr_charlie);

		// Call BOB, which calls CHARLIE, forcing CHARLIE to self-destruct.
		assert_ok!(builder::call(addr_bob).data(addr_charlie.encode()).build());

		// Check that CHARLIE has moved on to the great beyond (ie. died).
		assert!(get_contract_checked(&addr_charlie).is_none());
	});
}

#[test]
fn cannot_self_destruct_in_constructor() {
	let (wasm, _) = compile_module::<Test>("self_destructing_constructor").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Fail to instantiate the BOB because the constructor calls seal_terminate.
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(wasm).value(100_000).build(),
			Error::<Test>::TerminatedInConstructor,
		);
	});
}

#[test]
fn crypto_hashes() {
	let (wasm, _code_hash) = compile_module::<Test>("crypto_hashes").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the CRYPTO_HASHES contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(100_000)
			.build_and_unwrap_account_id();
		// Perform the call.
		let input = b"_DEAD_BEEF";
		use sp_io::hashing::*;
		// Wraps a hash function into a more dynamic form usable for testing.
		macro_rules! dyn_hash_fn {
			($name:ident) => {
				Box::new(|input| $name(input).as_ref().to_vec().into_boxed_slice())
			};
		}
		// All hash functions and their associated output byte lengths.
		let test_cases: &[(Box<dyn Fn(&[u8]) -> Box<[u8]>>, usize)] = &[
			(dyn_hash_fn!(sha2_256), 32),
			(dyn_hash_fn!(keccak_256), 32),
			(dyn_hash_fn!(blake2_256), 32),
			(dyn_hash_fn!(blake2_128), 16),
		];
		// Test the given hash functions for the input: "_DEAD_BEEF"
		for (n, (hash_fn, expected_size)) in test_cases.iter().enumerate() {
			// We offset data in the contract tables by 1.
			let mut params = vec![(n + 1) as u8];
			params.extend_from_slice(input);
			let result = builder::bare_call(addr.clone()).data(params).build_and_unwrap_result();
			assert!(!result.did_revert());
			let expected = hash_fn(input.as_ref());
			assert_eq!(&result.data[..*expected_size], &*expected);
		}
	})
}

#[test]
fn transfer_return_code() {
	let (wasm, _code_hash) = compile_module::<Test>("transfer_return_code").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		// Contract has only the minimal balance so any transfer will fail.
		<Test as Config>::Currency::set_balance(&addr, min_balance);
		let result = builder::bare_call(addr.clone()).build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::TransferFailed);
	});
}

#[test]
fn call_return_code() {
	let (caller_code, _caller_hash) = compile_module::<Test>("call_return_code").unwrap();
	let (callee_code, _callee_hash) = compile_module::<Test>("ok_trap_revert").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let _ = <Test as Config>::Currency::set_balance(&CHARLIE, 1000 * min_balance);

		let addr_bob = builder::bare_instantiate(Code::Upload(caller_code))
			.value(min_balance * 100)
			.data(vec![0])
			.build_and_unwrap_account_id();
		<Test as Config>::Currency::set_balance(&addr_bob, min_balance);

		// Contract calls into Django which is no valid contract
		let result = builder::bare_call(addr_bob.clone())
			.data(AsRef::<[u8]>::as_ref(&DJANGO).to_vec())
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::NotCallable);

		let addr_django = builder::bare_instantiate(Code::Upload(callee_code))
			.origin(CHARLIE)
			.value(min_balance * 100)
			.data(vec![0])
			.build_and_unwrap_account_id();
		<Test as Config>::Currency::set_balance(&addr_django, min_balance);

		// Contract has only the minimal balance so any transfer will fail.
		let result = builder::bare_call(addr_bob.clone())
			.data(
				AsRef::<[u8]>::as_ref(&addr_django)
					.iter()
					.chain(&0u32.to_le_bytes())
					.cloned()
					.collect(),
			)
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::TransferFailed);

		// Contract has enough balance but callee reverts because "1" is passed.
		<Test as Config>::Currency::set_balance(&addr_bob, min_balance + 1000);
		let result = builder::bare_call(addr_bob.clone())
			.data(
				AsRef::<[u8]>::as_ref(&addr_django)
					.iter()
					.chain(&1u32.to_le_bytes())
					.cloned()
					.collect(),
			)
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::CalleeReverted);

		// Contract has enough balance but callee traps because "2" is passed.
		let result = builder::bare_call(addr_bob)
			.data(
				AsRef::<[u8]>::as_ref(&addr_django)
					.iter()
					.chain(&2u32.to_le_bytes())
					.cloned()
					.collect(),
			)
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::CalleeTrapped);
	});
}

#[test]
fn instantiate_return_code() {
	let (caller_code, _caller_hash) = compile_module::<Test>("instantiate_return_code").unwrap();
	let (callee_code, callee_hash) = compile_module::<Test>("ok_trap_revert").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let _ = <Test as Config>::Currency::set_balance(&CHARLIE, 1000 * min_balance);
		let callee_hash = callee_hash.as_ref().to_vec();

		assert_ok!(builder::instantiate_with_code(callee_code).value(min_balance * 100).build());

		let addr = builder::bare_instantiate(Code::Upload(caller_code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		// Contract has only the minimal balance so any transfer will fail.
		<Test as Config>::Currency::set_balance(&addr, min_balance);
		let result = builder::bare_call(addr.clone())
			.data(callee_hash.clone())
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::TransferFailed);

		// Contract has enough balance but the passed code hash is invalid
		<Test as Config>::Currency::set_balance(&addr, min_balance + 10_000);
		let result = builder::bare_call(addr.clone()).data(vec![0; 33]).build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::CodeNotFound);

		// Contract has enough balance but callee reverts because "1" is passed.
		let result = builder::bare_call(addr.clone())
			.data(callee_hash.iter().chain(&1u32.to_le_bytes()).cloned().collect())
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::CalleeReverted);

		// Contract has enough balance but callee traps because "2" is passed.
		let result = builder::bare_call(addr)
			.data(callee_hash.iter().chain(&2u32.to_le_bytes()).cloned().collect())
			.build_and_unwrap_result();
		assert_return_code!(result, RuntimeReturnCode::CalleeTrapped);
	});
}

#[test]
fn disabled_chain_extension_wont_deploy() {
	let (code, _hash) = compile_module::<Test>("chain_extension").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		TestExtension::disable();
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(code).value(3 * min_balance).build(),
			<Error<Test>>::CodeRejected,
		);
	});
}

#[test]
fn disabled_chain_extension_errors_on_call() {
	let (code, _hash) = compile_module::<Test>("chain_extension").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();
		TestExtension::disable();
		assert_err_ignore_postinfo!(
			builder::call(addr.clone()).build(),
			Error::<Test>::CodeRejected,
		);
	});
}

#[test]
fn chain_extension_works() {
	let (code, _hash) = compile_module::<Test>("chain_extension").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		// 0 = read input buffer and pass it through as output
		let input: Vec<u8> = ExtensionInput { extension_id: 0, func_id: 0, extra: &[99] }.into();
		let result = builder::bare_call(addr.clone()).data(input.clone()).build();
		assert_eq!(TestExtension::last_seen_buffer(), input);
		assert_eq!(result.result.unwrap().data, input);

		// 1 = treat inputs as integer primitives and store the supplied integers
		builder::bare_call(addr.clone())
			.data(ExtensionInput { extension_id: 0, func_id: 1, extra: &[] }.into())
			.build_and_unwrap_result();
		assert_eq!(TestExtension::last_seen_input_len(), 4);

		// 2 = charge some extra weight (amount supplied in the fifth byte)
		let result = builder::bare_call(addr.clone())
			.data(ExtensionInput { extension_id: 0, func_id: 2, extra: &0u32.encode() }.into())
			.build();
		assert_ok!(result.result);
		let gas_consumed = result.gas_consumed;
		let result = builder::bare_call(addr.clone())
			.data(ExtensionInput { extension_id: 0, func_id: 2, extra: &42u32.encode() }.into())
			.build();
		assert_ok!(result.result);
		assert_eq!(result.gas_consumed.ref_time(), gas_consumed.ref_time() + 42);
		let result = builder::bare_call(addr.clone())
			.data(ExtensionInput { extension_id: 0, func_id: 2, extra: &95u32.encode() }.into())
			.build();
		assert_ok!(result.result);
		assert_eq!(result.gas_consumed.ref_time(), gas_consumed.ref_time() + 95);

		// 3 = diverging chain extension call that sets flags to 0x1 and returns a fixed buffer
		let result = builder::bare_call(addr.clone())
			.data(ExtensionInput { extension_id: 0, func_id: 3, extra: &[] }.into())
			.build_and_unwrap_result();
		assert_eq!(result.flags, ReturnFlags::REVERT);
		assert_eq!(result.data, vec![42, 99]);

		// diverging to second chain extension that sets flags to 0x1 and returns a fixed buffer
		// We set the MSB part to 1 (instead of 0) which routes the request into the second
		// extension
		let result = builder::bare_call(addr.clone())
			.data(ExtensionInput { extension_id: 1, func_id: 0, extra: &[] }.into())
			.build_and_unwrap_result();
		assert_eq!(result.flags, ReturnFlags::REVERT);
		assert_eq!(result.data, vec![0x4B, 0x1D]);

		// Diverging to third chain extension that is disabled
		// We set the MSB part to 2 (instead of 0) which routes the request into the third extension
		assert_err_ignore_postinfo!(
			builder::call(addr.clone())
				.data(ExtensionInput { extension_id: 2, func_id: 0, extra: &[] }.into())
				.build(),
			Error::<Test>::NoChainExtension,
		);
	});
}

#[test]
fn chain_extension_temp_storage_works() {
	let (code, _hash) = compile_module::<Test>("chain_extension_temp_storage").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		// Call func 0 and func 1 back to back.
		let stop_recursion = 0u8;
		let mut input: Vec<u8> = ExtensionInput { extension_id: 3, func_id: 0, extra: &[] }.into();
		input.extend_from_slice(
			ExtensionInput { extension_id: 3, func_id: 1, extra: &[stop_recursion] }
				.to_vec()
				.as_ref(),
		);

		assert_ok!(builder::bare_call(addr.clone()).data(input.clone()).build().result);
	})
}

#[test]
fn lazy_removal_works() {
	let (code, _hash) = compile_module::<Test>("self_destruct").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let info = get_contract(&addr);
		let trie = &info.child_trie_info();

		// Put value into the contracts child trie
		child::put(trie, &[99], &42);

		// Terminate the contract
		assert_ok!(builder::call(addr.clone()).build());

		// Contract info should be gone
		assert!(!<ContractInfoOf::<Test>>::contains_key(&addr));

		// But value should be still there as the lazy removal did not run, yet.
		assert_matches!(child::get(trie, &[99]), Some(42));

		// Run the lazy removal
		Contracts::on_idle(System::block_number(), Weight::MAX);

		// Value should be gone now
		assert_matches!(child::get::<i32>(trie, &[99]), None);
	});
}

#[test]
fn lazy_batch_removal_works() {
	let (code, _hash) = compile_module::<Test>("self_destruct").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let mut tries: Vec<child::ChildInfo> = vec![];

		for i in 0..3u8 {
			let addr = builder::bare_instantiate(Code::Upload(code.clone()))
				.value(min_balance * 100)
				.salt(vec![i])
				.build_and_unwrap_account_id();

			let info = get_contract(&addr);
			let trie = &info.child_trie_info();

			// Put value into the contracts child trie
			child::put(trie, &[99], &42);

			// Terminate the contract. Contract info should be gone, but value should be still there
			// as the lazy removal did not run, yet.
			assert_ok!(builder::call(addr.clone()).build());

			assert!(!<ContractInfoOf::<Test>>::contains_key(&addr));
			assert_matches!(child::get(trie, &[99]), Some(42));

			tries.push(trie.clone())
		}

		// Run single lazy removal
		Contracts::on_idle(System::block_number(), Weight::MAX);

		// The single lazy removal should have removed all queued tries
		for trie in tries.iter() {
			assert_matches!(child::get::<i32>(trie, &[99]), None);
		}
	});
}

#[test]
fn lazy_removal_partial_remove_works() {
	let (code, _hash) = compile_module::<Test>("self_destruct").unwrap();

	// We create a contract with some extra keys above the weight limit
	let extra_keys = 7u32;
	let mut meter = WeightMeter::with_limit(Weight::from_parts(5_000_000_000, 100 * 1024));
	let (weight_per_key, max_keys) = ContractInfo::<Test>::deletion_budget(&meter);
	let vals: Vec<_> = (0..max_keys + extra_keys)
		.map(|i| (blake2_256(&i.encode()), (i as u32), (i as u32).encode()))
		.collect();

	let mut ext = ExtBuilder::default().existential_deposit(50).build();

	let trie = ext.execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let info = get_contract(&addr);

		// Put value into the contracts child trie
		for val in &vals {
			info.write(&Key::Fix(val.0), Some(val.2.clone()), None, false).unwrap();
		}
		<ContractInfoOf<Test>>::insert(&addr, info.clone());

		// Terminate the contract
		assert_ok!(builder::call(addr.clone()).build());

		// Contract info should be gone
		assert!(!<ContractInfoOf::<Test>>::contains_key(&addr));

		let trie = info.child_trie_info();

		// But value should be still there as the lazy removal did not run, yet.
		for val in &vals {
			assert_eq!(child::get::<u32>(&trie, &blake2_256(&val.0)), Some(val.1));
		}

		trie.clone()
	});

	// The lazy removal limit only applies to the backend but not to the overlay.
	// This commits all keys from the overlay to the backend.
	ext.commit_all().unwrap();

	ext.execute_with(|| {
		// Run the lazy removal
		ContractInfo::<Test>::process_deletion_queue_batch(&mut meter);

		// Weight should be exhausted because we could not even delete all keys
		assert!(!meter.can_consume(weight_per_key));

		let mut num_deleted = 0u32;
		let mut num_remaining = 0u32;

		for val in &vals {
			match child::get::<u32>(&trie, &blake2_256(&val.0)) {
				None => num_deleted += 1,
				Some(x) if x == val.1 => num_remaining += 1,
				Some(_) => panic!("Unexpected value in contract storage"),
			}
		}

		// All but one key is removed
		assert_eq!(num_deleted + num_remaining, vals.len() as u32);
		assert_eq!(num_deleted, max_keys);
		assert_eq!(num_remaining, extra_keys);
	});
}

#[test]
fn lazy_removal_does_no_run_on_low_remaining_weight() {
	let (code, _hash) = compile_module::<Test>("self_destruct").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let info = get_contract(&addr);
		let trie = &info.child_trie_info();

		// Put value into the contracts child trie
		child::put(trie, &[99], &42);

		// Terminate the contract
		assert_ok!(builder::call(addr.clone()).build());

		// Contract info should be gone
		assert!(!<ContractInfoOf::<Test>>::contains_key(&addr));

		// But value should be still there as the lazy removal did not run, yet.
		assert_matches!(child::get(trie, &[99]), Some(42));

		// Assign a remaining weight which is too low for a successful deletion of the contract
		let low_remaining_weight =
			<<Test as Config>::WeightInfo as WeightInfo>::on_process_deletion_queue_batch();

		// Run the lazy removal
		Contracts::on_idle(System::block_number(), low_remaining_weight);

		// Value should still be there, since remaining weight was too low for removal
		assert_matches!(child::get::<i32>(trie, &[99]), Some(42));

		// Run the lazy removal while deletion_queue is not full
		Contracts::on_initialize(System::block_number());

		// Value should still be there, since deletion_queue was not full
		assert_matches!(child::get::<i32>(trie, &[99]), Some(42));

		// Run on_idle with max remaining weight, this should remove the value
		Contracts::on_idle(System::block_number(), Weight::MAX);

		// Value should be gone
		assert_matches!(child::get::<i32>(trie, &[99]), None);
	});
}

#[test]
fn lazy_removal_does_not_use_all_weight() {
	let (code, _hash) = compile_module::<Test>("self_destruct").unwrap();

	let mut meter = WeightMeter::with_limit(Weight::from_parts(5_000_000_000, 100 * 1024));
	let mut ext = ExtBuilder::default().existential_deposit(50).build();

	let (trie, vals, weight_per_key) = ext.execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let info = get_contract(&addr);
		let (weight_per_key, max_keys) = ContractInfo::<Test>::deletion_budget(&meter);
		assert!(max_keys > 0);

		// We create a contract with one less storage item than we can remove within the limit
		let vals: Vec<_> = (0..max_keys - 1)
			.map(|i| (blake2_256(&i.encode()), (i as u32), (i as u32).encode()))
			.collect();

		// Put value into the contracts child trie
		for val in &vals {
			info.write(&Key::Fix(val.0), Some(val.2.clone()), None, false).unwrap();
		}
		<ContractInfoOf<Test>>::insert(&addr, info.clone());

		// Terminate the contract
		assert_ok!(builder::call(addr.clone()).build());

		// Contract info should be gone
		assert!(!<ContractInfoOf::<Test>>::contains_key(&addr));

		let trie = info.child_trie_info();

		// But value should be still there as the lazy removal did not run, yet.
		for val in &vals {
			assert_eq!(child::get::<u32>(&trie, &blake2_256(&val.0)), Some(val.1));
		}

		(trie, vals, weight_per_key)
	});

	// The lazy removal limit only applies to the backend but not to the overlay.
	// This commits all keys from the overlay to the backend.
	ext.commit_all().unwrap();

	ext.execute_with(|| {
		// Run the lazy removal
		ContractInfo::<Test>::process_deletion_queue_batch(&mut meter);
		let base_weight =
			<<Test as Config>::WeightInfo as WeightInfo>::on_process_deletion_queue_batch();
		assert_eq!(meter.consumed(), weight_per_key.mul(vals.len() as _) + base_weight);

		// All the keys are removed
		for val in vals {
			assert_eq!(child::get::<u32>(&trie, &blake2_256(&val.0)), None);
		}
	});
}

#[test]
fn deletion_queue_ring_buffer_overflow() {
	let (code, _hash) = compile_module::<Test>("self_destruct").unwrap();
	let mut ext = ExtBuilder::default().existential_deposit(50).build();

	// setup the deletion queue with custom counters
	ext.execute_with(|| {
		let queue = DeletionQueueManager::from_test_values(u32::MAX - 1, u32::MAX - 1);
		<DeletionQueueCounter<Test>>::set(queue);
	});

	// commit the changes to the storage
	ext.commit_all().unwrap();

	ext.execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let mut tries: Vec<child::ChildInfo> = vec![];

		// add 3 contracts to the deletion queue
		for i in 0..3u8 {
			let addr = builder::bare_instantiate(Code::Upload(code.clone()))
				.value(min_balance * 100)
				.salt(vec![i])
				.build_and_unwrap_account_id();

			let info = get_contract(&addr);
			let trie = &info.child_trie_info();

			// Put value into the contracts child trie
			child::put(trie, &[99], &42);

			// Terminate the contract. Contract info should be gone, but value should be still
			// there as the lazy removal did not run, yet.
			assert_ok!(builder::call(addr.clone()).build());

			assert!(!<ContractInfoOf::<Test>>::contains_key(&addr));
			assert_matches!(child::get(trie, &[99]), Some(42));

			tries.push(trie.clone())
		}

		// Run single lazy removal
		Contracts::on_idle(System::block_number(), Weight::MAX);

		// The single lazy removal should have removed all queued tries
		for trie in tries.iter() {
			assert_matches!(child::get::<i32>(trie, &[99]), None);
		}

		// insert and delete counter values should go from u32::MAX - 1 to 1
		assert_eq!(<DeletionQueueCounter<Test>>::get().as_test_tuple(), (1, 1));
	})
}
#[test]
fn refcounter() {
	let (wasm, code_hash) = compile_module::<Test>("self_destruct").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();

		// Create two contracts with the same code and check that they do in fact share it.
		let addr0 = builder::bare_instantiate(Code::Upload(wasm.clone()))
			.value(min_balance * 100)
			.salt(vec![0])
			.build_and_unwrap_account_id();
		let addr1 = builder::bare_instantiate(Code::Upload(wasm.clone()))
			.value(min_balance * 100)
			.salt(vec![1])
			.build_and_unwrap_account_id();
		assert_refcount!(code_hash, 2);

		// Sharing should also work with the usual instantiate call
		let addr2 = builder::bare_instantiate(Code::Existing(code_hash))
			.value(min_balance * 100)
			.salt(vec![2])
			.build_and_unwrap_account_id();
		assert_refcount!(code_hash, 3);

		// Terminating one contract should decrement the refcount
		assert_ok!(builder::call(addr0).build());
		assert_refcount!(code_hash, 2);

		// remove another one
		assert_ok!(builder::call(addr1).build());
		assert_refcount!(code_hash, 1);

		// Pristine code should still be there
		PristineCode::<Test>::get(code_hash).unwrap();

		// remove the last contract
		assert_ok!(builder::call(addr2).build());
		assert_refcount!(code_hash, 0);

		// refcount is `0` but code should still exists because it needs to be removed manually
		assert!(crate::PristineCode::<Test>::contains_key(&code_hash));
	});
}

#[test]
fn debug_message_works() {
	let (wasm, _code_hash) = compile_module::<Test>("debug_message_works").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(30_000)
			.build_and_unwrap_account_id();
		let result = builder::bare_call(addr).debug(DebugInfo::UnsafeDebug).build();

		assert_matches!(result.result, Ok(_));
		assert_eq!(std::str::from_utf8(&result.debug_message).unwrap(), "Hello World!");
	});
}

#[test]
fn debug_message_logging_disabled() {
	let (wasm, _code_hash) = compile_module::<Test>("debug_message_logging_disabled").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(30_000)
			.build_and_unwrap_account_id();
		// disable logging by passing `false`
		let result = builder::bare_call(addr.clone()).build();
		assert_matches!(result.result, Ok(_));
		// the dispatchables always run without debugging
		assert_ok!(Contracts::call(RuntimeOrigin::signed(ALICE), addr, 0, GAS_LIMIT, None, vec![]));
		assert!(result.debug_message.is_empty());
	});
}

#[test]
fn debug_message_invalid_utf8() {
	let (wasm, _code_hash) = compile_module::<Test>("debug_message_invalid_utf8").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(30_000)
			.build_and_unwrap_account_id();
		let result = builder::bare_call(addr).debug(DebugInfo::UnsafeDebug).build();
		assert_ok!(result.result);
		assert!(result.debug_message.is_empty());
	});
}

#[test]
fn gas_estimation_for_subcalls() {
	let (caller_code, _caller_hash) = compile_module::<Test>("call_with_limit").unwrap();
	let (call_runtime_code, _caller_hash) = compile_module::<Test>("call_runtime").unwrap();
	let (dummy_code, _callee_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 2_000 * min_balance);

		let addr_caller = builder::bare_instantiate(Code::Upload(caller_code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let addr_dummy = builder::bare_instantiate(Code::Upload(dummy_code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let addr_call_runtime = builder::bare_instantiate(Code::Upload(call_runtime_code))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		// Run the test for all of those weight limits for the subcall
		let weights = [
			Weight::zero(),
			GAS_LIMIT,
			GAS_LIMIT * 2,
			GAS_LIMIT / 5,
			Weight::from_parts(0, GAS_LIMIT.proof_size()),
			Weight::from_parts(GAS_LIMIT.ref_time(), 0),
		];

		// This call is passed to the sub call in order to create a large `required_weight`
		let runtime_call = RuntimeCall::Dummy(pallet_dummy::Call::overestimate_pre_charge {
			pre_charge: Weight::from_parts(10_000_000_000, 512 * 1024),
			actual_weight: Weight::from_parts(1, 1),
		})
		.encode();

		// Encodes which contract should be sub called with which input
		let sub_calls: [(&[u8], Vec<_>, bool); 2] = [
			(addr_dummy.as_ref(), vec![], false),
			(addr_call_runtime.as_ref(), runtime_call, true),
		];

		for weight in weights {
			for (sub_addr, sub_input, out_of_gas_in_subcall) in &sub_calls {
				let input: Vec<u8> = sub_addr
					.iter()
					.cloned()
					.chain(weight.ref_time().to_le_bytes())
					.chain(weight.proof_size().to_le_bytes())
					.chain(sub_input.clone())
					.collect();

				// Call in order to determine the gas that is required for this call
				let result = builder::bare_call(addr_caller.clone()).data(input.clone()).build();
				assert_ok!(&result.result);

				// If the out of gas happens in the subcall the caller contract
				// will just trap. Otherwise we would need to forward an error
				// code to signal that the sub contract ran out of gas.
				let error: DispatchError = if *out_of_gas_in_subcall {
					assert!(result.gas_required.all_gt(result.gas_consumed));
					<Error<Test>>::ContractTrapped.into()
				} else {
					assert_eq!(result.gas_required, result.gas_consumed);
					<Error<Test>>::OutOfGas.into()
				};

				// Make the same call using the estimated gas. Should succeed.
				assert_ok!(
					builder::bare_call(addr_caller.clone())
						.gas_limit(result.gas_required)
						.storage_deposit_limit(Some(result.storage_deposit.charge_or_zero()))
						.data(input.clone())
						.build()
						.result
				);

				// Check that it fails with too little ref_time
				assert_err!(
					builder::bare_call(addr_caller.clone())
						.gas_limit(result.gas_required.sub_ref_time(1))
						.storage_deposit_limit(Some(result.storage_deposit.charge_or_zero()))
						.data(input.clone())
						.build()
						.result,
					error,
				);

				// Check that it fails with too little proof_size
				assert_err!(
					builder::bare_call(addr_caller.clone())
						.gas_limit(result.gas_required.sub_proof_size(1))
						.storage_deposit_limit(Some(result.storage_deposit.charge_or_zero()))
						.data(input)
						.build()
						.result,
					error,
				);
			}
		}
	});
}

#[test]
fn gas_estimation_call_runtime() {
	let (caller_code, _caller_hash) = compile_module::<Test>("call_runtime").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let _ = <Test as Config>::Currency::set_balance(&CHARLIE, 1000 * min_balance);

		let addr_caller = builder::bare_instantiate(Code::Upload(caller_code))
			.value(min_balance * 100)
			.salt(vec![0])
			.build_and_unwrap_account_id();

		// Call something trivial with a huge gas limit so that we can observe the effects
		// of pre-charging. This should create a difference between consumed and required.
		let call = RuntimeCall::Dummy(pallet_dummy::Call::overestimate_pre_charge {
			pre_charge: Weight::from_parts(10_000_000, 1_000),
			actual_weight: Weight::from_parts(100, 100),
		});
		let result = builder::bare_call(addr_caller.clone()).data(call.encode()).build();
		// contract encodes the result of the dispatch runtime
		let outcome = u32::decode(&mut result.result.unwrap().data.as_ref()).unwrap();
		assert_eq!(outcome, 0);
		assert!(result.gas_required.all_gt(result.gas_consumed));

		// Make the same call using the required gas. Should succeed.
		assert_ok!(
			builder::bare_call(addr_caller)
				.gas_limit(result.gas_required)
				.data(call.encode())
				.build()
				.result
		);
	});
}

#[test]
fn call_runtime_reentrancy_guarded() {
	let (caller_code, _caller_hash) = compile_module::<Test>("call_runtime").unwrap();
	let (callee_code, _callee_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);
		let _ = <Test as Config>::Currency::set_balance(&CHARLIE, 1000 * min_balance);

		let addr_caller = builder::bare_instantiate(Code::Upload(caller_code))
			.value(min_balance * 100)
			.salt(vec![0])
			.build_and_unwrap_account_id();

		let addr_callee = builder::bare_instantiate(Code::Upload(callee_code))
			.value(min_balance * 100)
			.salt(vec![1])
			.build_and_unwrap_account_id();

		// Call pallet_contracts call() dispatchable
		let call = RuntimeCall::Contracts(crate::Call::call {
			dest: addr_callee,
			value: 0,
			gas_limit: GAS_LIMIT / 3,
			storage_deposit_limit: None,
			data: vec![],
		});

		// Call runtime to re-enter back to contracts engine by
		// calling dummy contract
		let result = builder::bare_call(addr_caller.clone())
			.data(call.encode())
			.build_and_unwrap_result();
		// Call to runtime should fail because of the re-entrancy guard
		assert_return_code!(result, RuntimeReturnCode::CallRuntimeFailed);
	});
}

#[test]
fn ecdsa_recover() {
	let (wasm, _code_hash) = compile_module::<Test>("ecdsa_recover").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the ecdsa_recover contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(100_000)
			.build_and_unwrap_account_id();

		#[rustfmt::skip]
		let signature: [u8; 65] = [
			161, 234, 203,  74, 147, 96,  51, 212,   5, 174, 231,   9, 142,  48, 137, 201,
			162, 118, 192,  67, 239, 16,  71, 216, 125,  86, 167, 139,  70,   7,  86, 241,
			 33,  87, 154, 251,  81, 29, 160,   4, 176, 239,  88, 211, 244, 232, 232,  52,
			211, 234, 100, 115, 230, 47,  80,  44, 152, 166,  62,  50,   8,  13,  86, 175,
			 28,
		];
		#[rustfmt::skip]
		let message_hash: [u8; 32] = [
			162, 28, 244, 179, 96, 76, 244, 178, 188,  83, 230, 248, 143, 106,  77, 117,
			239, 95, 244, 171, 65, 95,  62, 153, 174, 166, 182,  28, 130,  73, 196, 208
		];
		#[rustfmt::skip]
		const EXPECTED_COMPRESSED_PUBLIC_KEY: [u8; 33] = [
			  2, 121, 190, 102, 126, 249, 220, 187, 172, 85, 160,  98, 149, 206, 135, 11,
			  7,   2, 155, 252, 219,  45, 206,  40, 217, 89, 242, 129,  91,  22, 248, 23,
			152,
		];
		let mut params = vec![];
		params.extend_from_slice(&signature);
		params.extend_from_slice(&message_hash);
		assert!(params.len() == 65 + 32);
		let result = builder::bare_call(addr.clone()).data(params).build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(result.data, EXPECTED_COMPRESSED_PUBLIC_KEY);
	})
}

#[test]
fn bare_instantiate_returns_events() {
	let (wasm, _code_hash) = compile_module::<Test>("transfer_return_code").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let result = builder::bare_instantiate(Code::Upload(wasm))
			.value(min_balance * 100)
			.collect_events(CollectEvents::UnsafeCollect)
			.build();

		let events = result.events.unwrap();
		assert!(!events.is_empty());
		assert_eq!(events, System::events());
	});
}

#[test]
fn bare_instantiate_does_not_return_events() {
	let (wasm, _code_hash) = compile_module::<Test>("transfer_return_code").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let result = builder::bare_instantiate(Code::Upload(wasm)).value(min_balance * 100).build();

		let events = result.events;
		assert!(!System::events().is_empty());
		assert!(events.is_none());
	});
}

#[test]
fn bare_call_returns_events() {
	let (wasm, _code_hash) = compile_module::<Test>("transfer_return_code").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let result = builder::bare_call(addr.clone())
			.collect_events(CollectEvents::UnsafeCollect)
			.build();

		let events = result.events.unwrap();
		assert_return_code!(&result.result.unwrap(), RuntimeReturnCode::Success);
		assert!(!events.is_empty());
		assert_eq!(events, System::events());
	});
}

#[test]
fn bare_call_does_not_return_events() {
	let (wasm, _code_hash) = compile_module::<Test>("transfer_return_code").unwrap();
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000 * min_balance);

		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(min_balance * 100)
			.build_and_unwrap_account_id();

		let result = builder::bare_call(addr.clone()).build();

		let events = result.events;
		assert_return_code!(&result.result.unwrap(), RuntimeReturnCode::Success);
		assert!(!System::events().is_empty());
		assert!(events.is_none());
	});
}

#[test]
fn sr25519_verify() {
	let (wasm, _code_hash) = compile_module::<Test>("sr25519_verify").unwrap();

	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the sr25519_verify contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(100_000)
			.build_and_unwrap_account_id();

		let call_with = |message: &[u8; 11]| {
			// Alice's signature for "hello world"
			#[rustfmt::skip]
			let signature: [u8; 64] = [
				184, 49, 74, 238, 78, 165, 102, 252, 22, 92, 156, 176, 124, 118, 168, 116, 247,
				99, 0, 94, 2, 45, 9, 170, 73, 222, 182, 74, 60, 32, 75, 64, 98, 174, 69, 55, 83,
				85, 180, 98, 208, 75, 231, 57, 205, 62, 4, 105, 26, 136, 172, 17, 123, 99, 90, 255,
				228, 54, 115, 63, 30, 207, 205, 131,
			];

			// Alice's public key
			#[rustfmt::skip]
			let public_key: [u8; 32] = [
				212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130, 44,
				133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125,
			];

			let mut params = vec![];
			params.extend_from_slice(&signature);
			params.extend_from_slice(&public_key);
			params.extend_from_slice(message);

			builder::bare_call(addr.clone()).data(params).build_and_unwrap_result()
		};

		// verification should succeed for "hello world"
		assert_return_code!(call_with(&b"hello world"), RuntimeReturnCode::Success);

		// verification should fail for other messages
		assert_return_code!(call_with(&b"hello worlD"), RuntimeReturnCode::Sr25519VerifyFailed);
	});
}

#[test]
fn failed_deposit_charge_should_roll_back_call() {
	let (wasm_caller, _) = compile_module::<Test>("call_runtime_and_call").unwrap();
	let (wasm_callee, _) = compile_module::<Test>("store_call").unwrap();
	const ED: u64 = 200;

	let execute = || {
		ExtBuilder::default().existential_deposit(ED).build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			// Instantiate both contracts.
			let addr_caller = builder::bare_instantiate(Code::Upload(wasm_caller.clone()))
				.build_and_unwrap_account_id();
			let addr_callee = builder::bare_instantiate(Code::Upload(wasm_callee.clone()))
				.build_and_unwrap_account_id();

			// Give caller proxy access to Alice.
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(ALICE), addr_caller.clone(), (), 0));

			// Create a Proxy call that will attempt to transfer away Alice's balance.
			let transfer_call =
				Box::new(RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
					dest: CHARLIE,
					value: pallet_balances::Pallet::<Test>::free_balance(&ALICE) - 2 * ED,
				}));

			// Wrap the transfer call in a proxy call.
			let transfer_proxy_call = RuntimeCall::Proxy(pallet_proxy::Call::proxy {
				real: ALICE,
				force_proxy_type: Some(()),
				call: transfer_call,
			});

			let data = (
				(ED - DepositPerItem::get()) as u32, // storage length
				addr_callee,
				transfer_proxy_call,
			);

			builder::call(addr_caller).data(data.encode()).build()
		})
	};

	// With a low enough deposit per byte, the call should succeed.
	let result = execute().unwrap();

	// Bump the deposit per byte to a high value to trigger a FundsUnavailable error.
	DEPOSIT_PER_BYTE.with(|c| *c.borrow_mut() = 20);
	assert_err_with_weight!(execute(), TokenError::FundsUnavailable, result.actual_weight);
}

#[test]
fn upload_code_works() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Drop previous events
		initialize_block(2);

		assert!(!PristineCode::<Test>::contains_key(&code_hash));

		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			Some(codec::Compact(1_000)),
			Determinism::Enforced,
		));
		// Ensure the contract was stored and get expected deposit amount to be reserved.
		let deposit_expected = expected_deposit(ensure_stored(code_hash));

		assert_eq!(
			System::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::Contracts(crate::Event::CodeStored {
					code_hash,
					deposit_held: deposit_expected,
					uploader: ALICE
				}),
				topics: vec![],
			},]
		);
	});
}

#[test]
fn upload_code_limit_too_low() {
	let (wasm, _code_hash) = compile_module::<Test>("dummy").unwrap();
	let deposit_expected = expected_deposit(wasm.len());
	let deposit_insufficient = deposit_expected.saturating_sub(1);

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Drop previous events
		initialize_block(2);

		assert_noop!(
			Contracts::upload_code(
				RuntimeOrigin::signed(ALICE),
				wasm,
				Some(codec::Compact(deposit_insufficient)),
				Determinism::Enforced
			),
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		assert_eq!(System::events(), vec![]);
	});
}

#[test]
fn upload_code_not_enough_balance() {
	let (wasm, _code_hash) = compile_module::<Test>("dummy").unwrap();
	let deposit_expected = expected_deposit(wasm.len());
	let deposit_insufficient = deposit_expected.saturating_sub(1);

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, deposit_insufficient);

		// Drop previous events
		initialize_block(2);

		assert_noop!(
			Contracts::upload_code(
				RuntimeOrigin::signed(ALICE),
				wasm,
				Some(codec::Compact(1_000)),
				Determinism::Enforced
			),
			<Error<Test>>::StorageDepositNotEnoughFunds,
		);

		assert_eq!(System::events(), vec![]);
	});
}

#[test]
fn remove_code_works() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Drop previous events
		initialize_block(2);

		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			Some(codec::Compact(1_000)),
			Determinism::Enforced,
		));
		// Ensure the contract was stored and get expected deposit amount to be reserved.
		let deposit_expected = expected_deposit(ensure_stored(code_hash));

		assert_ok!(Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash));
		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::CodeStored {
						code_hash,
						deposit_held: deposit_expected,
						uploader: ALICE
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::CodeRemoved {
						code_hash,
						deposit_released: deposit_expected,
						remover: ALICE
					}),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn remove_code_wrong_origin() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Drop previous events
		initialize_block(2);

		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			Some(codec::Compact(1_000)),
			Determinism::Enforced,
		));
		// Ensure the contract was stored and get expected deposit amount to be reserved.
		let deposit_expected = expected_deposit(ensure_stored(code_hash));

		assert_noop!(
			Contracts::remove_code(RuntimeOrigin::signed(BOB), code_hash),
			sp_runtime::traits::BadOrigin,
		);

		assert_eq!(
			System::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::Contracts(crate::Event::CodeStored {
					code_hash,
					deposit_held: deposit_expected,
					uploader: ALICE
				}),
				topics: vec![],
			},]
		);
	});
}

#[test]
fn remove_code_in_use() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		assert_ok!(builder::instantiate_with_code(wasm).build());

		// Drop previous events
		initialize_block(2);

		assert_noop!(
			Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash),
			<Error<Test>>::CodeInUse,
		);

		assert_eq!(System::events(), vec![]);
	});
}

#[test]
fn remove_code_not_found() {
	let (_wasm, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Drop previous events
		initialize_block(2);

		assert_noop!(
			Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash),
			<Error<Test>>::CodeNotFound,
		);

		assert_eq!(System::events(), vec![]);
	});
}

#[test]
fn instantiate_with_zero_balance_works() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();

		// Drop previous events
		initialize_block(2);

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		// Ensure the contract was stored and get expected deposit amount to be reserved.
		let deposit_expected = expected_deposit(ensure_stored(code_hash));

		// Make sure the account exists even though no free balance was send
		assert_eq!(<Test as Config>::Currency::free_balance(&addr), min_balance);
		assert_eq!(
			<Test as Config>::Currency::total_balance(&addr),
			min_balance + test_utils::contract_info_storage_deposit(&addr)
		);

		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::CodeStored {
						code_hash,
						deposit_held: deposit_expected,
						uploader: ALICE
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::System(frame_system::Event::NewAccount {
						account: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Endowed {
						account: addr.clone(),
						free_balance: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: addr.clone(),
						amount: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Instantiated {
						deployer: ALICE,
						contract: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndHeld {
							from: ALICE,
							to: addr.clone(),
							amount: test_utils::contract_info_storage_deposit(&addr),
						}
					),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn instantiate_with_below_existential_deposit_works() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();
		let value = 50;

		// Drop previous events
		initialize_block(2);

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(value)
			.build_and_unwrap_account_id();

		// Ensure the contract was stored and get expected deposit amount to be reserved.
		let deposit_expected = expected_deposit(ensure_stored(code_hash));
		// Make sure the account exists even though not enough free balance was send
		assert_eq!(<Test as Config>::Currency::free_balance(&addr), min_balance + value);
		assert_eq!(
			<Test as Config>::Currency::total_balance(&addr),
			min_balance + value + test_utils::contract_info_storage_deposit(&addr)
		);

		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::CodeStored {
						code_hash,
						deposit_held: deposit_expected,
						uploader: ALICE
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::System(frame_system::Event::NewAccount {
						account: addr.clone()
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Endowed {
						account: addr.clone(),
						free_balance: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: addr.clone(),
						amount: min_balance,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: addr.clone(),
						amount: 50,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Instantiated {
						deployer: ALICE,
						contract: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndHeld {
							from: ALICE,
							to: addr.clone(),
							amount: test_utils::contract_info_storage_deposit(&addr),
						}
					),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn storage_deposit_works() {
	let (wasm, _code_hash) = compile_module::<Test>("multi_store").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		let mut deposit = test_utils::contract_info_storage_deposit(&addr);

		// Drop previous events
		initialize_block(2);

		// Create storage
		assert_ok!(builder::call(addr.clone())
			.value(42)
			.data((1_000u32, 5_000u32).encode())
			.build());
		// 4 is for creating 2 storage items
		let charged0 = 4 + 1_000 + 5_000;
		deposit += charged0;
		assert_eq!(get_contract(&addr).total_deposit(), deposit);

		// Add more storage (but also remove some)
		assert_ok!(builder::call(addr.clone()).data((2_000u32, 4_900u32).encode()).build());
		let charged1 = 1_000 - 100;
		deposit += charged1;
		assert_eq!(get_contract(&addr).total_deposit(), deposit);

		// Remove more storage (but also add some)
		assert_ok!(builder::call(addr.clone()).data((2_100u32, 900u32).encode()).build());
		// -1 for numeric instability
		let refunded0 = 4_000 - 100 - 1;
		deposit -= refunded0;
		assert_eq!(get_contract(&addr).total_deposit(), deposit);

		assert_eq!(
			System::events(),
			vec![
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Balances(pallet_balances::Event::Transfer {
						from: ALICE,
						to: addr.clone(),
						amount: 42,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndHeld {
							from: ALICE,
							to: addr.clone(),
							amount: charged0,
						}
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndHeld {
							from: ALICE,
							to: addr.clone(),
							amount: charged1,
						}
					),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(
						pallet_contracts::Event::StorageDepositTransferredAndReleased {
							from: addr.clone(),
							to: ALICE,
							amount: refunded0,
						}
					),
					topics: vec![],
				},
			]
		);
	});
}

#[test]
fn storage_deposit_callee_works() {
	let (wasm_caller, _code_hash_caller) = compile_module::<Test>("call").unwrap();
	let (wasm_callee, _code_hash_callee) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();

		// Create both contracts: Constructors do nothing.
		let addr_caller =
			builder::bare_instantiate(Code::Upload(wasm_caller)).build_and_unwrap_account_id();
		let addr_callee =
			builder::bare_instantiate(Code::Upload(wasm_callee)).build_and_unwrap_account_id();

		assert_ok!(builder::call(addr_caller).data((100u32, &addr_callee).encode()).build());

		let callee = get_contract(&addr_callee);
		let deposit = DepositPerByte::get() * 100 + DepositPerItem::get() * 1;

		assert_eq!(test_utils::get_balance(&addr_callee), min_balance);
		assert_eq!(
			callee.total_deposit(),
			deposit + test_utils::contract_info_storage_deposit(&addr_callee)
		);
	});
}

#[test]
fn set_code_extrinsic() {
	let (wasm, code_hash) = compile_module::<Test>("dummy").unwrap();
	let (new_wasm, new_code_hash) = compile_module::<Test>("crypto_hashes").unwrap();

	assert_ne!(code_hash, new_code_hash);

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			new_wasm,
			None,
			Determinism::Enforced
		));

		// Drop previous events
		initialize_block(2);

		assert_eq!(get_contract(&addr).code_hash, code_hash);
		assert_refcount!(&code_hash, 1);
		assert_refcount!(&new_code_hash, 0);

		// only root can execute this extrinsic
		assert_noop!(
			Contracts::set_code(RuntimeOrigin::signed(ALICE), addr.clone(), new_code_hash),
			sp_runtime::traits::BadOrigin,
		);
		assert_eq!(get_contract(&addr).code_hash, code_hash);
		assert_refcount!(&code_hash, 1);
		assert_refcount!(&new_code_hash, 0);
		assert_eq!(System::events(), vec![]);

		// contract must exist
		assert_noop!(
			Contracts::set_code(RuntimeOrigin::root(), BOB, new_code_hash),
			<Error<Test>>::ContractNotFound,
		);
		assert_eq!(get_contract(&addr).code_hash, code_hash);
		assert_refcount!(&code_hash, 1);
		assert_refcount!(&new_code_hash, 0);
		assert_eq!(System::events(), vec![]);

		// new code hash must exist
		assert_noop!(
			Contracts::set_code(RuntimeOrigin::root(), addr.clone(), Default::default()),
			<Error<Test>>::CodeNotFound,
		);
		assert_eq!(get_contract(&addr).code_hash, code_hash);
		assert_refcount!(&code_hash, 1);
		assert_refcount!(&new_code_hash, 0);
		assert_eq!(System::events(), vec![]);

		// successful call
		assert_ok!(Contracts::set_code(RuntimeOrigin::root(), addr.clone(), new_code_hash));
		assert_eq!(get_contract(&addr).code_hash, new_code_hash);
		assert_refcount!(&code_hash, 0);
		assert_refcount!(&new_code_hash, 1);
		assert_eq!(
			System::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::Contracts(pallet_contracts::Event::ContractCodeUpdated {
					contract: addr.clone(),
					new_code_hash,
					old_code_hash: code_hash,
				}),
				topics: vec![],
			},]
		);
	});
}

#[test]
fn slash_cannot_kill_account() {
	let (wasm, _code_hash) = compile_module::<Test>("dummy").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let value = 700;
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();

		let addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(value)
			.build_and_unwrap_account_id();

		// Drop previous events
		initialize_block(2);

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);

		assert_eq!(
			test_utils::get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &addr),
			info_deposit
		);

		assert_eq!(
			<Test as Config>::Currency::total_balance(&addr),
			info_deposit + value + min_balance
		);

		// Try to destroy the account of the contract by slashing the total balance.
		// The account does not get destroyed because slashing only affects the balance held under
		// certain `reason`. Slashing can for example happen if the contract takes part in staking.
		let _ = <Test as Config>::Currency::slash(
			&HoldReason::StorageDepositReserve.into(),
			&addr,
			<Test as Config>::Currency::total_balance(&addr),
		);

		// Slashing only removed the balance held.
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), value + min_balance);
	});
}

#[test]
fn contract_reverted() {
	let (wasm, code_hash) = compile_module::<Test>("return_with_data").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let flags = ReturnFlags::REVERT;
		let buffer = [4u8, 8, 15, 16, 23, 42];
		let input = (flags.bits(), buffer).encode();

		// We just upload the code for later use
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm.clone(),
			None,
			Determinism::Enforced
		));

		// Calling extrinsic: revert leads to an error
		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).data(input.clone()).build(),
			<Error<Test>>::ContractReverted,
		);

		// Calling extrinsic: revert leads to an error
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(wasm).data(input.clone()).build(),
			<Error<Test>>::ContractReverted,
		);

		// Calling directly: revert leads to success but the flags indicate the error
		// This is just a different way of transporting the error that allows the read out
		// the `data` which is only there on success. Obviously, the contract isn't
		// instantiated.
		let result = builder::bare_instantiate(Code::Existing(code_hash))
			.data(input.clone())
			.build_and_unwrap_result();
		assert_eq!(result.result.flags, flags);
		assert_eq!(result.result.data, buffer);
		assert!(!<ContractInfoOf<Test>>::contains_key(result.account_id));

		// Pass empty flags and therefore successfully instantiate the contract for later use.
		let addr = builder::bare_instantiate(Code::Existing(code_hash))
			.data(ReturnFlags::empty().bits().encode())
			.build_and_unwrap_account_id();

		// Calling extrinsic: revert leads to an error
		assert_err_ignore_postinfo!(
			builder::call(addr.clone()).data(input.clone()).build(),
			<Error<Test>>::ContractReverted,
		);

		// Calling directly: revert leads to success but the flags indicate the error
		let result = builder::bare_call(addr.clone()).data(input).build_and_unwrap_result();
		assert_eq!(result.flags, flags);
		assert_eq!(result.data, buffer);
	});
}

#[test]
fn set_code_hash() {
	let (wasm, code_hash) = compile_module::<Test>("set_code_hash").unwrap();
	let (new_wasm, new_code_hash) = compile_module::<Test>("new_set_code_hash_contract").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the 'caller'
		let contract_addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(300_000)
			.build_and_unwrap_account_id();
		// upload new code
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			new_wasm.clone(),
			None,
			Determinism::Enforced
		));

		System::reset_events();

		// First call sets new code_hash and returns 1
		let result = builder::bare_call(contract_addr.clone())
			.data(new_code_hash.as_ref().to_vec())
			.debug(DebugInfo::UnsafeDebug)
			.build_and_unwrap_result();
		assert_return_code!(result, 1);

		// Second calls new contract code that returns 2
		let result = builder::bare_call(contract_addr.clone())
			.debug(DebugInfo::UnsafeDebug)
			.build_and_unwrap_result();
		assert_return_code!(result, 2);

		// Checking for the last event only
		assert_eq!(
			&System::events(),
			&[
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::ContractCodeUpdated {
						contract: contract_addr.clone(),
						new_code_hash,
						old_code_hash: code_hash,
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: contract_addr.clone(),
					}),
					topics: vec![],
				},
				EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Contracts(crate::Event::Called {
						caller: Origin::from_account_id(ALICE),
						contract: contract_addr.clone(),
					}),
					topics: vec![],
				},
			],
		);
	});
}

#[test]
fn storage_deposit_limit_is_enforced() {
	let (wasm, _code_hash) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let min_balance = Contracts::min_balance();

		// Setting insufficient storage_deposit should fail.
		assert_err!(
			builder::bare_instantiate(Code::Upload(wasm.clone()))
				// expected deposit is 2 * ed + 3 for the call
				.storage_deposit_limit(Some((2 * min_balance + 3 - 1).into()))
				.build()
				.result,
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);
		// Check that the BOB contract has been instantiated and has the minimum balance
		assert_eq!(get_contract(&addr).total_deposit(), info_deposit);
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), info_deposit + min_balance);

		// Create 1 byte of storage with a price of per byte,
		// setting insufficient deposit limit, as it requires 3 Balance:
		// 2 for the item added + 1 for the new storage item.
		assert_err_ignore_postinfo!(
			builder::call(addr.clone())
				.storage_deposit_limit(Some(codec::Compact(2)))
				.data(1u32.to_le_bytes().to_vec())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		// To check that deposit limit fallbacks to DefaultDepositLimit,
		// we customize it here.
		DEFAULT_DEPOSIT_LIMIT.with(|c| *c.borrow_mut() = 3);

		// Create 1 byte of storage, should cost 3 Balance:
		// 2 for the item added + 1 for the new storage item.
		// Should pass as it fallbacks to DefaultDepositLimit.
		assert_ok!(builder::call(addr.clone()).data(1u32.to_le_bytes().to_vec()).build());

		// Use 4 more bytes of the storage for the same item, which requires 4 Balance.
		// Should fail as DefaultDepositLimit is 3 and hence isn't enough.
		assert_err_ignore_postinfo!(
			builder::call(addr.clone()).data(5u32.to_le_bytes().to_vec()).build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);
	});
}

#[test]
fn deposit_limit_in_nested_calls() {
	let (wasm_caller, _code_hash_caller) =
		compile_module::<Test>("create_storage_and_call").unwrap();
	let (wasm_callee, _code_hash_callee) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Create both contracts: Constructors do nothing.
		let addr_caller =
			builder::bare_instantiate(Code::Upload(wasm_caller)).build_and_unwrap_account_id();
		let addr_callee =
			builder::bare_instantiate(Code::Upload(wasm_callee)).build_and_unwrap_account_id();

		// Create 100 bytes of storage with a price of per byte
		// This is 100 Balance + 2 Balance for the item
		assert_ok!(builder::call(addr_callee.clone())
			.storage_deposit_limit(Some(codec::Compact(102)))
			.data(100u32.to_le_bytes().to_vec())
			.build());

		// We do not remove any storage but add a storage item of 12 bytes in the caller
		// contract. This would cost 12 + 2 = 14 Balance.
		// The nested call doesn't get a special limit, which is set by passing 0 to it.
		// This should fail as the specified parent's limit is less than the cost: 13 <
		// 14.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.storage_deposit_limit(Some(codec::Compact(13)))
				.data((100u32, &addr_callee, 0u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);
		// Now we specify the parent's limit high enough to cover the caller's storage additions.
		// However, we use a single byte more in the callee, hence the storage deposit should be 15
		// Balance.
		// The nested call doesn't get a special limit, which is set by passing 0 to it.
		// This should fail as the specified parent's limit is less than the cost: 14
		// < 15.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.storage_deposit_limit(Some(codec::Compact(14)))
				.data((101u32, &addr_callee, 0u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		// Now we specify the parent's limit high enough to cover both the caller's and callee's
		// storage additions. However, we set a special deposit limit of 1 Balance for the nested
		// call. This should fail as callee adds up 2 bytes to the storage, meaning that the nested
		// call should have a deposit limit of at least 2 Balance. The sub-call should be rolled
		// back, which is covered by the next test case.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.storage_deposit_limit(Some(codec::Compact(16)))
				.data((102u32, &addr_callee, 1u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		// Refund in the callee contract but not enough to cover the 14 Balance required by the
		// caller. Note that if previous sub-call wouldn't roll back, this call would pass making
		// the test case fail. We don't set a special limit for the nested call here.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.storage_deposit_limit(Some(codec::Compact(0)))
				.data((87u32, &addr_callee, 0u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000);

		// Require more than the sender's balance.
		// We don't set a special limit for the nested call.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.data((1200u32, &addr_callee, 1u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);

		// Same as above but allow for the additional deposit of 1 Balance in parent.
		// We set the special deposit limit of 1 Balance for the nested call, which isn't
		// enforced as callee frees up storage. This should pass.
		assert_ok!(builder::call(addr_caller.clone())
			.storage_deposit_limit(Some(codec::Compact(1)))
			.data((87u32, &addr_callee, 1u64).encode())
			.build());
	});
}

#[test]
fn deposit_limit_in_nested_instantiate() {
	let (wasm_caller, _code_hash_caller) =
		compile_module::<Test>("create_storage_and_instantiate").unwrap();
	let (wasm_callee, code_hash_callee) = compile_module::<Test>("store_deploy").unwrap();
	const ED: u64 = 5;
	ExtBuilder::default().existential_deposit(ED).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let _ = <Test as Config>::Currency::set_balance(&BOB, 1_000_000);
		// Create caller contract
		let addr_caller = builder::bare_instantiate(Code::Upload(wasm_caller))
			.value(10_000u64) // this balance is later passed to the deployed contract
			.build_and_unwrap_account_id();
		// Deploy a contract to get its occupied storage size
		let addr = builder::bare_instantiate(Code::Upload(wasm_callee))
			.data(vec![0, 0, 0, 0])
			.build_and_unwrap_account_id();

		let callee_info_len = ContractInfoOf::<Test>::get(&addr).unwrap().encoded_size() as u64;

		// We don't set a special deposit limit for the nested instantiation.
		//
		// The deposit limit set for the parent is insufficient for the instantiation, which
		// requires:
		// - callee_info_len + 2 for storing the new contract info,
		// - ED for deployed contract account,
		// - 2 for the storage item of 0 bytes being created in the callee constructor
		// or (callee_info_len + 2 + ED + 2) Balance in total.
		//
		// Provided the limit is set to be 1 Balance less,
		// this call should fail on the return from the caller contract.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.storage_deposit_limit(Some(codec::Compact(callee_info_len + 2 + ED + 1)))
				.data((0u32, &code_hash_callee, 0u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);
		// The charges made on instantiation should be rolled back.
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), 1_000_000);

		// Now we give enough limit for the instantiation itself, but require for 1 more storage
		// byte in the constructor. Hence +1 Balance to the limit is needed. This should fail on the
		// return from constructor.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.storage_deposit_limit(Some(codec::Compact(callee_info_len + 2 + ED + 2)))
				.data((1u32, &code_hash_callee, 0u64).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);
		// The charges made on the instantiation should be rolled back.
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), 1_000_000);

		// Now we set enough limit in parent call, but an insufficient limit for child instantiate.
		// This should fail during the charging for the instantiation in
		// `RawMeter::charge_instantiate()`
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.storage_deposit_limit(Some(codec::Compact(callee_info_len + 2 + ED + 2)))
				.data((0u32, &code_hash_callee, callee_info_len + 2 + ED + 1).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);
		// The charges made on the instantiation should be rolled back.
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), 1_000_000);

		// Same as above but requires for single added storage
		// item of 1 byte to be covered by the limit, which implies 3 more Balance.
		// Now we set enough limit for the parent call, but insufficient limit for child
		// instantiate. This should fail right after the constructor execution.
		assert_err_ignore_postinfo!(
			builder::call(addr_caller.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.storage_deposit_limit(Some(codec::Compact(callee_info_len + 2 + ED + 3))) // enough parent limit
				.data((1u32, &code_hash_callee, callee_info_len + 2 + ED + 2).encode())
				.build(),
			<Error<Test>>::StorageDepositLimitExhausted,
		);
		// The charges made on the instantiation should be rolled back.
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), 1_000_000);

		// Set enough deposit limit for the child instantiate. This should succeed.
		let result = builder::bare_call(addr_caller.clone())
			.origin(BOB)
			.storage_deposit_limit(Some(codec::Compact(callee_info_len + 2 + ED + 4).into()))
			.data((1u32, &code_hash_callee, callee_info_len + 2 + ED + 3).encode())
			.build();

		let returned = result.result.unwrap();
		// All balance of the caller except ED has been transferred to the callee.
		// No deposit has been taken from it.
		assert_eq!(<Test as Config>::Currency::free_balance(&addr_caller), ED);
		// Get address of the deployed contract.
		let addr_callee = AccountId32::from_slice(&returned.data[0..32]).unwrap();
		// 10_000 should be sent to callee from the caller contract, plus ED to be sent from the
		// origin.
		assert_eq!(<Test as Config>::Currency::free_balance(&addr_callee), 10_000 + ED);
		// The origin should be charged with:
		//  - callee instantiation deposit = (callee_info_len + 2)
		//  - callee account ED
		//  - for writing an item of 1 byte to storage = 3 Balance
		assert_eq!(
			<Test as Config>::Currency::free_balance(&BOB),
			1_000_000 - (callee_info_len + 2 + ED + 3)
		);
		// Check that deposit due to be charged still includes these 3 Balance
		assert_eq!(result.storage_deposit.charge_or_zero(), (callee_info_len + 2 + ED + 3))
	});
}

#[test]
fn deposit_limit_honors_liquidity_restrictions() {
	let (wasm, _code_hash) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let bobs_balance = 1_000;
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let _ = <Test as Config>::Currency::set_balance(&BOB, bobs_balance);
		let min_balance = Contracts::min_balance();

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);
		// Check that the contract has been instantiated and has the minimum balance
		assert_eq!(get_contract(&addr).total_deposit(), info_deposit);
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), info_deposit + min_balance);

		// check that the hold is honored
		<Test as Config>::Currency::hold(
			&HoldReason::CodeUploadDepositReserve.into(),
			&BOB,
			bobs_balance - min_balance,
		)
		.unwrap();
		assert_err_ignore_postinfo!(
			builder::call(addr.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.storage_deposit_limit(Some(codec::Compact(200)))
				.data(100u32.to_le_bytes().to_vec())
				.build(),
			<Error<Test>>::StorageDepositNotEnoughFunds,
		);
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), min_balance);
	});
}

#[test]
fn deposit_limit_honors_existential_deposit() {
	let (wasm, _code_hash) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let _ = <Test as Config>::Currency::set_balance(&BOB, 1_000);
		let min_balance = Contracts::min_balance();

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);

		// Check that the contract has been instantiated and has the minimum balance
		assert_eq!(get_contract(&addr).total_deposit(), info_deposit);
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), min_balance + info_deposit);

		// check that the deposit can't bring the account below the existential deposit
		assert_err_ignore_postinfo!(
			builder::call(addr.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.storage_deposit_limit(Some(codec::Compact(900)))
				.data(100u32.to_le_bytes().to_vec())
				.build(),
			<Error<Test>>::StorageDepositNotEnoughFunds,
		);
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), 1_000);
	});
}

#[test]
fn deposit_limit_honors_min_leftover() {
	let (wasm, _code_hash) = compile_module::<Test>("store_call").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let _ = <Test as Config>::Currency::set_balance(&BOB, 1_000);
		let min_balance = Contracts::min_balance();

		// Instantiate the BOB contract.
		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		let info_deposit = test_utils::contract_info_storage_deposit(&addr);

		// Check that the contract has been instantiated and has the minimum balance and the storage
		// deposit
		assert_eq!(get_contract(&addr).total_deposit(), info_deposit);
		assert_eq!(<Test as Config>::Currency::total_balance(&addr), info_deposit + min_balance);

		// check that the minimum leftover (value send) is considered
		assert_err_ignore_postinfo!(
			builder::call(addr.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.value(400)
				.storage_deposit_limit(Some(codec::Compact(500)))
				.data(100u32.to_le_bytes().to_vec())
				.build(),
			<Error<Test>>::StorageDepositNotEnoughFunds,
		);
		assert_eq!(<Test as Config>::Currency::free_balance(&BOB), 1_000);
	});
}

#[test]
fn upload_should_enforce_deterministic_mode_when_possible() {
	let upload = |fixture, determinism| {
		let (wasm, code_hash) = compile_module::<Test>(fixture).unwrap();
		ExtBuilder::default()
			.build()
			.execute_with(|| -> Result<Determinism, DispatchError> {
				let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
				Contracts::bare_upload_code(ALICE, wasm, None, determinism)?;
				let info = CodeInfoOf::<Test>::get(code_hash).unwrap();
				Ok(info.determinism())
			})
	};

	assert_eq!(upload("dummy", Determinism::Enforced), Ok(Determinism::Enforced));
	assert_eq!(upload("dummy", Determinism::Relaxed), Ok(Determinism::Enforced));
	assert_eq!(upload("float_instruction", Determinism::Relaxed), Ok(Determinism::Relaxed));
	assert!(upload("float_instruction", Determinism::Enforced).is_err());
}

#[test]
fn cannot_instantiate_indeterministic_code() {
	let (wasm, code_hash) = compile_module::<Test>("float_instruction").unwrap();
	let (caller_wasm, _) = compile_module::<Test>("instantiate_return_code").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Try to instantiate directly from code
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(wasm.clone()).build(),
			<Error<Test>>::CodeRejected,
		);
		assert_err!(
			builder::bare_instantiate(Code::Upload(wasm.clone())).build().result,
			<Error<Test>>::CodeRejected,
		);

		// Try to upload a non-deterministic code as deterministic
		assert_err!(
			Contracts::upload_code(
				RuntimeOrigin::signed(ALICE),
				wasm.clone(),
				None,
				Determinism::Enforced
			),
			<Error<Test>>::CodeRejected,
		);

		// Try to instantiate from already stored indeterministic code hash
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			None,
			Determinism::Relaxed,
		));

		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).build(),
			<Error<Test>>::Indeterministic,
		);
		assert_err!(
			builder::bare_instantiate(Code::Existing(code_hash)).build().result,
			<Error<Test>>::Indeterministic,
		);

		// Deploy contract which instantiates another contract
		let addr =
			builder::bare_instantiate(Code::Upload(caller_wasm)).build_and_unwrap_account_id();

		// Try to instantiate `code_hash` from another contract in deterministic mode
		assert_err!(
			builder::bare_call(addr.clone()).data(code_hash.encode()).build().result,
			<Error<Test>>::Indeterministic,
		);

		// Instantiations are not allowed even in non-determinism mode
		assert_err!(
			builder::bare_call(addr.clone()).data(code_hash.encode()).build().result,
			<Error<Test>>::Indeterministic,
		);
	});
}

#[test]
fn cannot_set_code_indeterministic_code() {
	let (wasm, code_hash) = compile_module::<Test>("float_instruction").unwrap();
	let (caller_wasm, _) = compile_module::<Test>("set_code_hash").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Put the non-deterministic contract on-chain
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			None,
			Determinism::Relaxed,
		));

		// Create the contract that will call `seal_set_code_hash`
		let caller_addr =
			builder::bare_instantiate(Code::Upload(caller_wasm)).build_and_unwrap_account_id();

		// We do not allow to set the code hash to a non-deterministic wasm
		assert_err!(
			builder::bare_call(caller_addr.clone()).data(code_hash.encode()).build().result,
			<Error<Test>>::Indeterministic,
		);
	});
}

#[test]
fn delegate_call_indeterministic_code() {
	let (wasm, code_hash) = compile_module::<Test>("float_instruction").unwrap();
	let (caller_wasm, _) = compile_module::<Test>("delegate_call_simple").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Put the non-deterministic contract on-chain
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm,
			None,
			Determinism::Relaxed,
		));

		// Create the contract that will call `seal_delegate_call`
		let caller_addr =
			builder::bare_instantiate(Code::Upload(caller_wasm)).build_and_unwrap_account_id();

		// The delegate call will fail in deterministic mode
		assert_err!(
			builder::bare_call(caller_addr.clone()).data(code_hash.encode()).build().result,
			<Error<Test>>::Indeterministic,
		);

		// The delegate call will work on non-deterministic mode
		assert_ok!(
			builder::bare_call(caller_addr.clone())
				.data(code_hash.encode())
				.determinism(Determinism::Relaxed)
				.build()
				.result
		);
	});
}

#[test]
fn locking_delegate_dependency_works() {
	// set hash lock up deposit to 30%, to test deposit calculation.
	CODE_HASH_LOCKUP_DEPOSIT_PERCENT.with(|c| *c.borrow_mut() = Perbill::from_percent(30));
	MAX_DELEGATE_DEPENDENCIES.with(|c| *c.borrow_mut() = 1);

	let (wasm_caller, self_code_hash) =
		compile_module::<Test>("locking_delegate_dependency").unwrap();
	let (wasm_callee, code_hash) = compile_module::<Test>("dummy").unwrap();
	let (wasm_other, other_code_hash) = compile_module::<Test>("call").unwrap();

	// Define inputs with various actions to test locking / unlocking delegate_dependencies.
	// See the contract for more details.
	let noop_input = (0u32, code_hash);
	let lock_delegate_dependency_input = (1u32, code_hash);
	let unlock_delegate_dependency_input = (2u32, code_hash);
	let terminate_input = (3u32, code_hash);

	// Instantiate the caller contract with the given input.
	let instantiate = |input: &(u32, H256)| {
		builder::bare_instantiate(Code::Upload(wasm_caller.clone()))
			.data(input.encode())
			.build()
	};

	// Call contract with the given input.
	let call = |addr_caller: &AccountId32, input: &(u32, H256)| {
		builder::bare_call(addr_caller.clone()).data(input.encode()).build()
	};
	const ED: u64 = 2000;
	ExtBuilder::default().existential_deposit(ED).build().execute_with(|| {
		let _ = Balances::set_balance(&ALICE, 1_000_000);

		// Instantiate with lock_delegate_dependency should fail since the code is not yet on chain.
		assert_err!(
			instantiate(&lock_delegate_dependency_input).result,
			Error::<Test>::CodeNotFound
		);

		// Upload the delegated code.
		let CodeUploadReturnValue { deposit, .. } =
			Contracts::bare_upload_code(ALICE, wasm_callee.clone(), None, Determinism::Enforced)
				.unwrap();

		// Instantiate should now work.
		let addr_caller = instantiate(&lock_delegate_dependency_input).result.unwrap().account_id;

		// There should be a dependency and a deposit.
		let contract = test_utils::get_contract(&addr_caller);

		let dependency_deposit = &CodeHashLockupDepositPercent::get().mul_ceil(deposit);
		assert_eq!(contract.delegate_dependencies().get(&code_hash), Some(dependency_deposit));
		assert_eq!(
			test_utils::get_balance_on_hold(
				&HoldReason::StorageDepositReserve.into(),
				&addr_caller
			),
			dependency_deposit + contract.storage_base_deposit()
		);

		// Removing the code should fail, since we have added a dependency.
		assert_err!(
			Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash),
			<Error<Test>>::CodeInUse
		);

		// Locking an already existing dependency should fail.
		assert_err!(
			call(&addr_caller, &lock_delegate_dependency_input).result,
			Error::<Test>::DelegateDependencyAlreadyExists
		);

		// Locking self should fail.
		assert_err!(
			call(&addr_caller, &(1u32, self_code_hash)).result,
			Error::<Test>::CannotAddSelfAsDelegateDependency
		);

		// Locking more than the maximum allowed delegate_dependencies should fail.
		Contracts::bare_upload_code(ALICE, wasm_other, None, Determinism::Enforced).unwrap();
		assert_err!(
			call(&addr_caller, &(1u32, other_code_hash)).result,
			Error::<Test>::MaxDelegateDependenciesReached
		);

		// Unlocking dependency should work.
		assert_ok!(call(&addr_caller, &unlock_delegate_dependency_input).result);

		// Dependency should be removed, and deposit should be returned.
		let contract = test_utils::get_contract(&addr_caller);
		assert!(contract.delegate_dependencies().is_empty());
		assert_eq!(
			test_utils::get_balance_on_hold(
				&HoldReason::StorageDepositReserve.into(),
				&addr_caller
			),
			contract.storage_base_deposit()
		);

		// Removing a nonexistent dependency should fail.
		assert_err!(
			call(&addr_caller, &unlock_delegate_dependency_input).result,
			Error::<Test>::DelegateDependencyNotFound
		);

		// Locking a dependency with a storage limit too low should fail.
		DEFAULT_DEPOSIT_LIMIT.with(|c| *c.borrow_mut() = dependency_deposit - 1);
		assert_err!(
			call(&addr_caller, &lock_delegate_dependency_input).result,
			Error::<Test>::StorageDepositLimitExhausted
		);

		// Since we unlocked the dependency we should now be able to remove the code.
		assert_ok!(Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash));

		// Calling should fail since the delegated contract is not on chain anymore.
		assert_err!(call(&addr_caller, &noop_input).result, Error::<Test>::ContractTrapped);

		// Restore initial deposit limit and add the dependency back.
		DEFAULT_DEPOSIT_LIMIT.with(|c| *c.borrow_mut() = 10_000_000);
		Contracts::bare_upload_code(ALICE, wasm_callee, None, Determinism::Enforced).unwrap();
		call(&addr_caller, &lock_delegate_dependency_input).result.unwrap();

		// Call terminate should work, and return the deposit.
		let balance_before = test_utils::get_balance(&ALICE);
		assert_ok!(call(&addr_caller, &terminate_input).result);
		assert_eq!(
			test_utils::get_balance(&ALICE),
			ED + balance_before + contract.storage_base_deposit() + dependency_deposit
		);

		// Terminate should also remove the dependency, so we can remove the code.
		assert_ok!(Contracts::remove_code(RuntimeOrigin::signed(ALICE), code_hash));
	});
}

#[test]
fn native_dependency_deposit_works() {
	let (wasm, code_hash) = compile_module::<Test>("set_code_hash").unwrap();
	let (dummy_wasm, dummy_code_hash) = compile_module::<Test>("dummy").unwrap();

	// Set hash lock up deposit to 30%, to test deposit calculation.
	CODE_HASH_LOCKUP_DEPOSIT_PERCENT.with(|c| *c.borrow_mut() = Perbill::from_percent(30));

	// Test with both existing and uploaded code
	for code in [Code::Upload(wasm.clone()), Code::Existing(code_hash)] {
		ExtBuilder::default().build().execute_with(|| {
			let _ = Balances::set_balance(&ALICE, 1_000_000);
			let lockup_deposit_percent = CodeHashLockupDepositPercent::get();

			// Upload the dummy contract,
			Contracts::upload_code(
				RuntimeOrigin::signed(ALICE),
				dummy_wasm.clone(),
				None,
				Determinism::Enforced,
			)
			.unwrap();

			// Upload `set_code_hash` contracts if using Code::Existing.
			let add_upload_deposit = match code {
				Code::Existing(_) => {
					Contracts::upload_code(
						RuntimeOrigin::signed(ALICE),
						wasm.clone(),
						None,
						Determinism::Enforced,
					)
					.unwrap();
					false
				},
				Code::Upload(_) => true,
			};

			// Instantiate the set_code_hash contract.
			let res = builder::bare_instantiate(code).build();

			let addr = res.result.unwrap().account_id;
			let base_deposit = test_utils::contract_info_storage_deposit(&addr);
			let upload_deposit = test_utils::get_code_deposit(&code_hash);
			let extra_deposit = add_upload_deposit.then(|| upload_deposit).unwrap_or_default();

			// Check initial storage_deposit
			// The base deposit should be: contract_info_storage_deposit + 30% * deposit
			let deposit =
				extra_deposit + base_deposit + lockup_deposit_percent.mul_ceil(upload_deposit);

			assert_eq!(res.storage_deposit.charge_or_zero(), deposit + Contracts::min_balance());

			// call set_code_hash
			builder::bare_call(addr.clone())
				.data(dummy_code_hash.encode())
				.build_and_unwrap_result();

			// Check updated storage_deposit
			let code_deposit = test_utils::get_code_deposit(&dummy_code_hash);
			let deposit = base_deposit + lockup_deposit_percent.mul_ceil(code_deposit);
			assert_eq!(test_utils::get_contract(&addr).storage_base_deposit(), deposit);

			assert_eq!(
				test_utils::get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &addr),
				deposit
			);
		});
	}
}

#[test]
fn reentrance_count_works_with_call() {
	let (wasm, _code_hash) = compile_module::<Test>("reentrance_count_call").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let contract_addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(300_000)
			.build_and_unwrap_account_id();

		// passing reentrant count to the input
		let input = 0.encode();

		builder::bare_call(contract_addr)
			.data(input)
			.debug(DebugInfo::UnsafeDebug)
			.build_and_unwrap_result();
	});
}

#[test]
fn reentrance_count_works_with_delegated_call() {
	let (wasm, code_hash) = compile_module::<Test>("reentrance_count_delegated_call").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let contract_addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(300_000)
			.build_and_unwrap_account_id();

		// adding a callstack height to the input
		let input = (code_hash, 1).encode();

		builder::bare_call(contract_addr.clone())
			.data(input)
			.debug(DebugInfo::UnsafeDebug)
			.build_and_unwrap_result();
	});
}

#[test]
fn account_reentrance_count_works() {
	let (wasm, _code_hash) = compile_module::<Test>("account_reentrance_count_call").unwrap();
	let (wasm_reentrance_count, _code_hash_reentrance_count) =
		compile_module::<Test>("reentrance_count_call").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let contract_addr = builder::bare_instantiate(Code::Upload(wasm))
			.value(300_000)
			.build_and_unwrap_account_id();

		let another_contract_addr = builder::bare_instantiate(Code::Upload(wasm_reentrance_count))
			.value(300_000)
			.build_and_unwrap_account_id();

		let result1 = builder::bare_call(contract_addr.clone())
			.data(contract_addr.encode())
			.debug(DebugInfo::UnsafeDebug)
			.build_and_unwrap_result();

		let result2 = builder::bare_call(contract_addr.clone())
			.data(another_contract_addr.encode())
			.debug(DebugInfo::UnsafeDebug)
			.build_and_unwrap_result();

		assert_eq!(result1.data, 1.encode());
		assert_eq!(result2.data, 0.encode());
	});
}

#[test]
fn root_cannot_upload_code() {
	let (wasm, _) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			Contracts::upload_code(RuntimeOrigin::root(), wasm, None, Determinism::Enforced),
			DispatchError::BadOrigin,
		);
	});
}

#[test]
fn root_cannot_remove_code() {
	let (_, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			Contracts::remove_code(RuntimeOrigin::root(), code_hash),
			DispatchError::BadOrigin,
		);
	});
}

#[test]
fn signed_cannot_set_code() {
	let (_, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			Contracts::set_code(RuntimeOrigin::signed(ALICE), BOB, code_hash),
			DispatchError::BadOrigin,
		);
	});
}

#[test]
fn none_cannot_call_code() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			builder::call(BOB).origin(RuntimeOrigin::none()).build(),
			DispatchError::BadOrigin,
		);
	});
}

#[test]
fn root_can_call() {
	let (wasm, _) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let addr = builder::bare_instantiate(Code::Upload(wasm)).build_and_unwrap_account_id();

		// Call the contract.
		assert_ok!(builder::call(addr.clone()).origin(RuntimeOrigin::root()).build());
	});
}

#[test]
fn root_cannot_instantiate_with_code() {
	let (wasm, _) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().build().execute_with(|| {
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(wasm).origin(RuntimeOrigin::root()).build(),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn root_cannot_instantiate() {
	let (_, code_hash) = compile_module::<Test>("dummy").unwrap();

	ExtBuilder::default().build().execute_with(|| {
		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).origin(RuntimeOrigin::root()).build(),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn only_upload_origin_can_upload() {
	let (wasm, _) = compile_module::<Test>("dummy").unwrap();
	UploadAccount::set(Some(ALICE));
	ExtBuilder::default().build().execute_with(|| {
		let _ = Balances::set_balance(&ALICE, 1_000_000);
		let _ = Balances::set_balance(&BOB, 1_000_000);

		assert_err!(
			Contracts::upload_code(
				RuntimeOrigin::root(),
				wasm.clone(),
				None,
				Determinism::Enforced,
			),
			DispatchError::BadOrigin
		);

		assert_err!(
			Contracts::upload_code(
				RuntimeOrigin::signed(BOB),
				wasm.clone(),
				None,
				Determinism::Enforced,
			),
			DispatchError::BadOrigin
		);

		// Only alice is allowed to upload contract code.
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			wasm.clone(),
			None,
			Determinism::Enforced,
		));
	});
}

#[test]
fn only_instantiation_origin_can_instantiate() {
	let (code, code_hash) = compile_module::<Test>("dummy").unwrap();
	InstantiateAccount::set(Some(ALICE));
	ExtBuilder::default().build().execute_with(|| {
		let _ = Balances::set_balance(&ALICE, 1_000_000);
		let _ = Balances::set_balance(&BOB, 1_000_000);

		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(code.clone())
				.origin(RuntimeOrigin::root())
				.build(),
			DispatchError::BadOrigin
		);

		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(code.clone())
				.origin(RuntimeOrigin::signed(BOB))
				.build(),
			DispatchError::BadOrigin
		);

		// Only Alice can instantiate
		assert_ok!(builder::instantiate_with_code(code).build());

		// Bob cannot instantiate with either `instantiate_with_code` or `instantiate`.
		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).origin(RuntimeOrigin::signed(BOB)).build(),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn balance_api_returns_free_balance() {
	let (wasm, _code_hash) = compile_module::<Test>("balance").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		// Instantiate the BOB contract without any extra balance.
		let addr =
			builder::bare_instantiate(Code::Upload(wasm.to_vec())).build_and_unwrap_account_id();

		let value = 0;
		// Call BOB which makes it call the balance runtime API.
		// The contract code asserts that the returned balance is 0.
		assert_ok!(builder::call(addr.clone()).value(value).build());

		let value = 1;
		// Calling with value will trap the contract.
		assert_err_ignore_postinfo!(
			builder::call(addr.clone()).value(value).build(),
			<Error<Test>>::ContractTrapped
		);
	});
}

#[test]
fn gas_consumed_is_linear_for_nested_calls() {
	let (code, _code_hash) = compile_module::<Test>("recurse").unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let addr = builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_account_id();

		let max_call_depth = <Test as Config>::CallStack::size() as u32;
		let [gas_0, gas_1, gas_2, gas_max] = {
			[0u32, 1u32, 2u32, max_call_depth]
				.iter()
				.map(|i| {
					let result = builder::bare_call(addr.clone()).data(i.encode()).build();
					assert_ok!(result.result);
					result.gas_consumed
				})
				.collect::<Vec<_>>()
				.try_into()
				.unwrap()
		};

		let gas_per_recursion = gas_2.checked_sub(&gas_1).unwrap();
		assert_eq!(gas_max, gas_0 + gas_per_recursion * max_call_depth as u64);
	});
}
