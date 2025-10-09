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
mod precompiles;
mod pvm;
mod sol;

use crate::{
	self as pallet_revive,
	evm::{
		fees::{BlockRatioFee, Info as FeeInfo},
		runtime::{EthExtra, SetWeightLimit},
	},
	genesis::{Account, ContractData},
	test_utils::*,
	AccountId32Mapper, AddressMapper, BalanceOf, BalanceWithDust, Call, CodeInfoOf, Config,
	ExecOrigin as Origin, GenesisConfig, OriginFor, Pallet, PristineCode,
};
use frame_support::{
	assert_ok, derive_impl,
	pallet_prelude::EnsureOrigin,
	parameter_types,
	traits::{ConstU32, ConstU64, FindAuthor, StorageVersion},
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, FixedFee, Weight},
};
use pallet_revive_fixtures::compile_module;
use pallet_transaction_payment::{ChargeTransactionPayment, ConstFeeMultiplier, Multiplier};
use sp_core::U256;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
	generic::Header,
	traits::{BlakeTwo256, Convert, IdentityLookup, One},
	AccountId32, BuildStorage, MultiAddress, MultiSignature, Perbill,
};

pub type Address = MultiAddress<AccountId32, u32>;
pub type Block = sp_runtime::generic::Block<Header<u64, BlakeTwo256>, UncheckedExtrinsic>;
pub type Signature = MultiSignature;
pub type SignedExtra = (
	frame_system::CheckNonce<Test>,
	ChargeTransactionPayment<Test>,
	crate::evm::tx_extension::SetOrigin<Test>,
);
pub type UncheckedExtrinsic =
	crate::evm::runtime::UncheckedExtrinsic<Address, Signature, EthExtraImpl>;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EthExtraImpl;

impl EthExtra for EthExtraImpl {
	type Config = Test;
	type Extension = SignedExtra;

	fn get_eth_extension(nonce: u32, tip: BalanceOf<Test>) -> Self::Extension {
		(
			frame_system::CheckNonce::from(nonce),
			ChargeTransactionPayment::from(tip),
			crate::evm::tx_extension::SetOrigin::<Test>::new_from_eth_transaction(),
		)
	}
}

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,
		Utility: pallet_utility,
		Contracts: pallet_revive,
		Proxy: pallet_proxy,
		TransactionPayment: pallet_transaction_payment,
		Dummy: pallet_dummy
	}
);

#[macro_export]
macro_rules! assert_return_code {
	( $x:expr , $y:expr $(,)? ) => {{
		assert_eq!(u32::from_le_bytes($x.data[..].try_into().unwrap()), $y as u32);
	}};
}

#[macro_export]
macro_rules! assert_refcount {
	( $code_hash:expr , $should:expr $(,)? ) => {{
		let is = crate::CodeInfoOf::<Test>::get($code_hash).map(|m| m.refcount()).unwrap();
		assert_eq!(is, $should);
	}};
}

pub mod test_utils {
	use super::{
		BalanceWithDust, CodeHashLockupDepositPercent, Contracts, DepositPerByte, DepositPerItem,
		Test,
	};
	use crate::{
		address::AddressMapper, exec::AccountIdOf, AccountInfo, AccountInfoOf, BalanceOf, CodeInfo,
		CodeInfoOf, Config, ContractInfo, PristineCode,
	};
	use codec::{Encode, MaxEncodedLen};
	use frame_support::traits::fungible::{InspectHold, Mutate};
	use sp_core::H160;

	pub fn place_contract(address: &AccountIdOf<Test>, code_hash: sp_core::H256) {
		set_balance(address, Contracts::min_balance() * 10);
		<CodeInfoOf<Test>>::insert(code_hash, CodeInfo::new(address.clone()));
		let address =
			<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(&address);
		let contract = <ContractInfo<Test>>::new(&address, 0, code_hash).unwrap();
		AccountInfo::<Test>::insert_contract(&address, contract);
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
	pub fn get_contract(addr: &H160) -> ContractInfo<Test> {
		get_contract_checked(addr).unwrap()
	}
	pub fn get_contract_checked(addr: &H160) -> Option<ContractInfo<Test>> {
		AccountInfo::<Test>::load_contract(addr)
	}
	pub fn get_code_deposit(code_hash: &sp_core::H256) -> BalanceOf<Test> {
		crate::CodeInfoOf::<Test>::get(code_hash).unwrap().deposit()
	}
	pub fn lockup_deposit(code_hash: &sp_core::H256) -> BalanceOf<Test> {
		CodeHashLockupDepositPercent::get().mul_ceil(get_code_deposit(code_hash)).into()
	}
	pub fn contract_base_deposit(addr: &H160) -> BalanceOf<Test> {
		let contract_info = self::get_contract(&addr);
		let info_size = contract_info.encoded_size() as u64;
		let code_deposit = CodeHashLockupDepositPercent::get()
			.mul_ceil(get_code_deposit(&contract_info.code_hash));
		let deposit = DepositPerByte::get()
			.saturating_mul(info_size)
			.saturating_add(DepositPerItem::get())
			.saturating_add(code_deposit);
		let immutable_size = contract_info.immutable_data_len() as u64;
		if immutable_size > 0 {
			let immutable_deposit = DepositPerByte::get()
				.saturating_mul(immutable_size)
				.saturating_add(DepositPerItem::get());
			deposit.saturating_add(immutable_deposit)
		} else {
			deposit
		}
	}
	pub fn expected_deposit(code_len: usize) -> u64 {
		// For code_info, the deposit for max_encoded_len is taken.
		let code_info_len = CodeInfo::<Test>::max_encoded_len() as u64;
		// Calculate deposit to be reserved.
		// We add 2 storage items: one for code, other for code_info
		DepositPerByte::get().saturating_mul(code_len as u64 + code_info_len) +
			DepositPerItem::get().saturating_mul(2)
	}
	pub fn ensure_stored(code_hash: sp_core::H256) -> usize {
		// Assert that code_info is stored
		assert!(CodeInfoOf::<Test>::contains_key(&code_hash));
		// Assert that contract code is stored, and get its size.
		PristineCode::<Test>::try_get(&code_hash).unwrap().len()
	}
	pub fn u256_bytes(u: u64) -> [u8; 32] {
		let mut buffer = [0u8; 32];
		let bytes = u.to_le_bytes();
		buffer[..8].copy_from_slice(&bytes);
		buffer
	}

	pub fn set_balance_with_dust(address: &H160, value: BalanceWithDust<BalanceOf<Test>>) {
		use frame_support::traits::Currency;
		let ed = <Test as Config>::Currency::minimum_balance();
		let (value, dust) = value.deconstruct();
		let account_id = <Test as Config>::AddressMapper::to_account_id(&address);
		<Test as Config>::Currency::set_balance(&account_id, ed + value);
		if dust > 0 {
			AccountInfoOf::<Test>::mutate(&address, |account| {
				if let Some(account) = account {
					account.dust = dust;
				} else {
					*account = Some(AccountInfo { dust, ..Default::default() });
				}
			});
		}
	}
}

pub(crate) mod builder {
	use super::Test;
	use crate::{
		test_utils::{builder::*, ALICE},
		tests::RuntimeOrigin,
		Code,
	};
	use sp_core::{H160, H256};

	pub fn bare_instantiate(code: Code) -> BareInstantiateBuilder<Test> {
		BareInstantiateBuilder::<Test>::bare_instantiate(RuntimeOrigin::signed(ALICE), code)
	}

	pub fn bare_call(dest: H160) -> BareCallBuilder<Test> {
		BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), dest)
	}

	pub fn instantiate_with_code(code: Vec<u8>) -> InstantiateWithCodeBuilder<Test> {
		InstantiateWithCodeBuilder::<Test>::instantiate_with_code(
			RuntimeOrigin::signed(ALICE),
			code,
		)
	}

	pub fn instantiate(code_hash: H256) -> InstantiateBuilder<Test> {
		InstantiateBuilder::<Test>::instantiate(RuntimeOrigin::signed(ALICE), code_hash)
	}

	pub fn call(dest: H160) -> CallBuilder<Test> {
		CallBuilder::<Test>::call(RuntimeOrigin::signed(ALICE), dest)
	}

	pub fn eth_call(dest: H160) -> EthCallBuilder<Test> {
		EthCallBuilder::<Test>::eth_call(RuntimeOrigin::signed(ALICE), dest)
	}
}

impl Test {
	pub fn set_unstable_interface(unstable_interface: bool) {
		UNSTABLE_INTERFACE.with(|v| *v.borrow_mut() = unstable_interface);
	}

	pub fn set_allow_evm_bytecode(allow_evm_bytecode: bool) {
		ALLOW_EVM_BYTECODE.with(|v| *v.borrow_mut() = allow_evm_bytecode);
	}
}

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(
			Weight::from_parts(2 * WEIGHT_REF_TIME_PER_SECOND, 10 * 1024 * 1024),
		);
	pub static ExistentialDeposit: u64 = 1;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type BlockWeights = BlockWeights;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<u64>;
}

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
	type BlockNumberProvider = frame_system::Pallet<Test>;
}

parameter_types! {
	pub FeeMultiplier: Multiplier = Multiplier::one();
}

#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Test {
	type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
	type WeightToFee = BlockRatioFee<1, 1, Self>;
	type LengthToFee = FixedFee<100, <Self as pallet_balances::Config>::Balance>;
	type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
}

impl pallet_dummy::Config for Test {}

parameter_types! {
	pub static DepositPerByte: BalanceOf<Test> = 1;
	pub const DepositPerItem: BalanceOf<Test> = 2;
	pub const CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(30);
	pub static ChainId: u64 = 448;
}

impl Convert<Weight, BalanceOf<Self>> for Test {
	fn convert(w: Weight) -> BalanceOf<Self> {
		w.ref_time()
	}
}

parameter_types! {
	pub static UploadAccount: Option<<Test as frame_system::Config>::AccountId> = None;
	pub static InstantiateAccount: Option<<Test as frame_system::Config>::AccountId> = None;
}

pub struct EnsureAccount<T, A>(core::marker::PhantomData<(T, A)>);
impl<T: Config, A: sp_core::Get<Option<crate::AccountIdOf<T>>>>
	EnsureOrigin<<T as frame_system::Config>::RuntimeOrigin> for EnsureAccount<T, A>
where
	<T as frame_system::Config>::AccountId: From<AccountId32>,
{
	type Success = T::AccountId;

	fn try_origin(o: OriginFor<T>) -> Result<Self::Success, OriginFor<T>> {
		let who = <frame_system::EnsureSigned<_> as EnsureOrigin<_>>::try_origin(o.clone())?;
		if matches!(A::get(), Some(a) if who != a) {
			return Err(o);
		}

		Ok(who)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<OriginFor<T>, ()> {
		Err(())
	}
}
parameter_types! {
	pub static UnstableInterface: bool = true;
	pub static AllowEvmBytecode: bool = true;
	pub CheckingAccount: AccountId32 = BOB.clone();
	pub static DebugFlag: bool = false;
}

impl FindAuthor<<Test as frame_system::Config>::AccountId> for Test {
	fn find_author<'a, I>(_digests: I) -> Option<<Test as frame_system::Config>::AccountId>
	where
		I: 'a + IntoIterator<Item = (frame_support::ConsensusEngineId, &'a [u8])>,
	{
		Some(EVE)
	}
}

#[derive_impl(crate::config_preludes::TestDefaultConfig)]
impl Config for Test {
	type Time = Timestamp;
	type AddressMapper = AccountId32Mapper<Self>;
	type Balance = u64;
	type Currency = Balances;
	type DepositPerByte = DepositPerByte;
	type DepositPerItem = DepositPerItem;
	type UnsafeUnstableInterface = UnstableInterface;
	type AllowEVMBytecode = AllowEvmBytecode;
	type UploadOrigin = EnsureAccount<Self, UploadAccount>;
	type InstantiateOrigin = EnsureAccount<Self, InstantiateAccount>;
	type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
	type ChainId = ChainId;
	type FindAuthor = Test;
	type Precompiles = (precompiles::WithInfo<Self>, precompiles::NoInfo<Self>);
	type FeeInfo = FeeInfo<Address, Signature, EthExtraImpl>;
	type DebugEnabled = DebugFlag;
}

impl TryFrom<RuntimeCall> for Call<Test> {
	type Error = ();

	fn try_from(value: RuntimeCall) -> Result<Self, Self::Error> {
		match value {
			RuntimeCall::Contracts(call) => Ok(call),
			_ => Err(()),
		}
	}
}

impl SetWeightLimit for RuntimeCall {
	fn set_weight_limit(&mut self, weight_limit: Weight) {
		match self {
			Self::Contracts(
				Call::eth_call { gas_limit, .. } |
				Call::eth_instantiate_with_code { gas_limit, .. },
			) => *gas_limit = weight_limit,
			_ => (),
		}
	}
}

pub struct ExtBuilder {
	existential_deposit: u64,
	storage_version: Option<StorageVersion>,
	code_hashes: Vec<sp_core::H256>,
	genesis_config: Option<crate::GenesisConfig<Test>>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			existential_deposit: ExistentialDeposit::get(),
			storage_version: None,
			code_hashes: vec![],
			genesis_config: Some(crate::GenesisConfig::<Test>::default()),
		}
	}
}

impl ExtBuilder {
	/// The pallet genesis config to use, or None if you don't want to include it.
	pub fn genesis_config(mut self, config: Option<crate::GenesisConfig<Test>>) -> Self {
		self.genesis_config = config;
		self
	}
	pub fn existential_deposit(mut self, existential_deposit: u64) -> Self {
		self.existential_deposit = existential_deposit;
		self
	}
	pub fn with_code_hashes(mut self, code_hashes: Vec<sp_core::H256>) -> Self {
		self.code_hashes = code_hashes;
		self
	}
	pub fn set_associated_consts(&self) {
		EXISTENTIAL_DEPOSIT.with(|v| *v.borrow_mut() = self.existential_deposit);
	}
	pub fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		self.set_associated_consts();
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let checking_account = Pallet::<Test>::checking_account();

		pallet_balances::GenesisConfig::<Test> {
			balances: vec![(checking_account.clone(), 1_000_000_000_000)],
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		if let Some(genesis_config) = self.genesis_config {
			genesis_config.assimilate_storage(&mut t).unwrap();
		}
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
		ext.execute_with(|| {
			assert_ok!(Pallet::<Test>::map_account(RuntimeOrigin::signed(checking_account)));
		});
		ext
	}
}

fn initialize_block(number: u64) {
	System::reset_events();
	System::initialize(&number, &[0u8; 32].into(), &Default::default());
}

impl Default for Origin<Test> {
	fn default() -> Self {
		Self::Signed(ALICE)
	}
}

#[test]
fn ext_builder_with_genesis_config_works() {
	let pvm_contract = Account {
		address: BOB_ADDR,
		balance: U256::from(100_000_100),
		nonce: 42,
		contract_data: Some(ContractData {
			code: compile_module("dummy").unwrap().0,
			storage: [([1u8; 32].into(), [2u8; 32].into())].into_iter().collect(),
		}),
	};

	let evm_contract = Account {
		address: CHARLIE_ADDR,
		balance: U256::from(1_000_00_100),
		nonce: 43,
		contract_data: Some(ContractData {
			code: vec![revm::bytecode::opcode::RETURN],
			storage: [([3u8; 32].into(), [4u8; 32].into())].into_iter().collect(),
		}),
	};

	let eoa =
		Account { address: ALICE_ADDR, balance: U256::from(100), nonce: 44, contract_data: None };

	let config = GenesisConfig::<Test> {
		mapped_accounts: vec![EVE],
		accounts: vec![eoa.clone(), pvm_contract.clone(), evm_contract.clone()],
		..Default::default()
	};

	// Genesis serialization works
	let json = serde_json::to_string(&config).unwrap();
	assert_eq!(config, serde_json::from_str::<GenesisConfig<Test>>(&json).unwrap());

	ExtBuilder::default().genesis_config(Some(config)).build().execute_with(|| {
		// account is mapped
		assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));

		// EOA is created
		assert_eq!(Pallet::<Test>::evm_balance(&eoa.address), eoa.balance);

		// Contract is created
		for contract in [pvm_contract, evm_contract] {
			let contract_data = contract.contract_data.unwrap();
			let contract_info = test_utils::get_contract(&contract.address);
			assert_eq!(
				PristineCode::<Test>::get(&contract_info.code_hash).unwrap(),
				contract_data.code
			);
			assert_eq!(Pallet::<Test>::evm_nonce(&contract.address), contract.nonce);
			assert_eq!(Pallet::<Test>::evm_balance(&contract.address), contract.balance);

			for (key, value) in contract_data.storage.iter() {
				assert_eq!(
					Pallet::<Test>::get_storage(contract.address, key.0),
					Ok(Some(value.0.to_vec()))
				);
			}
		}
	});
}
