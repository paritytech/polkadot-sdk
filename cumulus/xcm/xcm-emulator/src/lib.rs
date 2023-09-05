// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

pub use codec::{Decode, Encode, EncodeLike};
pub use lazy_static::lazy_static;
pub use log;
pub use paste;
pub use std::{
	any::type_name, collections::HashMap, error::Error, fmt, marker::PhantomData, ops::Deref,
	sync::Mutex,
};

// Substrate
pub use frame_support::{
	assert_ok,
	sp_runtime::{traits::Header as HeaderT, AccountId32, DispatchResult},
	traits::{
		EnqueueMessage, Get, Hooks, OriginTrait, ProcessMessage, ProcessMessageError, ServiceQueues,
	},
	weights::{Weight, WeightMeter},
};
pub use frame_system::{Config as SystemConfig, Pallet as SystemPallet};
pub use pallet_balances::AccountData;
pub use sp_arithmetic::traits::Bounded;
pub use sp_core::{blake2_256, parameter_types, sr25519, storage::Storage, Pair};
pub use sp_io::TestExternalities;
pub use sp_std::{cell::RefCell, collections::vec_deque::VecDeque, fmt::Debug};
pub use sp_tracing;

// Cumulus
pub use cumulus_pallet_parachain_system::Pallet as ParachainSystemPallet;
pub use cumulus_primitives_core::{
	relay_chain::{BlockNumber as RelayBlockNumber, HeadData, HrmpChannelId},
	AbridgedHrmpChannel, DmpMessageHandler, ParaId, PersistedValidationData, XcmpMessageHandler,
};
pub use cumulus_primitives_parachain_inherent::ParachainInherentData;
pub use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
pub use pallet_message_queue::{Config as MessageQueueConfig, Pallet as MessageQueuePallet};
pub use parachains_common::{AccountId, Balance, BlockNumber};
pub use polkadot_primitives;
pub use polkadot_runtime_parachains::inclusion::{AggregateMessageOrigin, UmpQueueId};

// Polkadot
pub use polkadot_parachain_primitives::primitives::RelayChainBlockNumber;
pub use xcm::v3::prelude::{
	Ancestor, MultiAssets, MultiLocation, Parachain as ParachainJunction, Parent, WeightLimit,
	XcmHash, X1,
};
pub use xcm_executor::traits::ConvertLocation;

thread_local! {
	/// Downward messages, each message is: `(to_para_id, [(relay_block_number, msg)])`
	#[allow(clippy::type_complexity)]
	pub static DOWNWARD_MESSAGES: RefCell<HashMap<String, VecDeque<(u32, Vec<(RelayBlockNumber, Vec<u8>)>)>>>
		= RefCell::new(HashMap::new());
	/// Downward messages that already processed by parachains, each message is: `(to_para_id, relay_block_number, Vec<u8>)`
	#[allow(clippy::type_complexity)]
	pub static DMP_DONE: RefCell<HashMap<String, VecDeque<(u32, RelayBlockNumber, Vec<u8>)>>>
		= RefCell::new(HashMap::new());
	/// Horizontal messages, each message is: `(to_para_id, [(from_para_id, relay_block_number, msg)])`
	#[allow(clippy::type_complexity)]
	pub static HORIZONTAL_MESSAGES: RefCell<HashMap<String, VecDeque<(u32, Vec<(ParaId, RelayBlockNumber, Vec<u8>)>)>>>
		= RefCell::new(HashMap::new());
	/// Upward messages, each message is: `(from_para_id, msg)`
	pub static UPWARD_MESSAGES: RefCell<HashMap<String, VecDeque<(u32, Vec<u8>)>>> = RefCell::new(HashMap::new());
	/// Bridged messages, each message is: `BridgeMessage`
	pub static BRIDGED_MESSAGES: RefCell<HashMap<String, VecDeque<BridgeMessage>>> = RefCell::new(HashMap::new());
	/// Parachains Ids a the Network
	pub static PARA_IDS: RefCell<HashMap<String, Vec<u32>>> = RefCell::new(HashMap::new());
	/// Flag indicating if global variables have been initialized for a certain Network
	pub static INITIALIZED: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
	/// Most recent `HeadData` of each parachain, encoded.
	pub static LAST_HEAD: RefCell<HashMap<String, HashMap<u32, HeadData>>> = RefCell::new(HashMap::new());
}

pub trait CheckAssertion<Origin, Destination, Hops, Args>
where
	Origin: Chain + Clone,
	Destination: Chain + Clone,
	Origin::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Destination::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Hops: Clone,
	Args: Clone,
{
	fn check_assertion(test: Test<Origin, Destination, Hops, Args>);
}

#[impl_trait_for_tuples::impl_for_tuples(5)]
impl<Origin, Destination, Hops, Args> CheckAssertion<Origin, Destination, Hops, Args> for Tuple
where
	Origin: Chain + Clone,
	Destination: Chain + Clone,
	Origin::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Destination::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Hops: Clone,
	Args: Clone,
{
	fn check_assertion(test: Test<Origin, Destination, Hops, Args>) {
		for_tuples!( #(
			Tuple::check_assertion(test.clone());
		)* );
	}
}

pub trait TestExt {
	fn build_new_ext(storage: Storage) -> TestExternalities;
	fn new_ext() -> TestExternalities;
	fn move_ext_out(id: &'static str);
	fn move_ext_in(id: &'static str);
	fn reset_ext();
	fn execute_with<R>(execute: impl FnOnce() -> R) -> R;
	fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R;
}

impl TestExt for () {
	fn build_new_ext(_storage: Storage) -> TestExternalities {
		TestExternalities::default()
	}
	fn new_ext() -> TestExternalities {
		TestExternalities::default()
	}
	fn move_ext_out(_id: &'static str) {}
	fn move_ext_in(_id: &'static str) {}
	fn reset_ext() {}
	fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
		execute()
	}
	fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
		func()
	}
}

pub trait Network {
	type Relay: RelayChain;
	type Bridge: Bridge;

	fn name() -> &'static str;
	fn init();
	fn reset();
	fn para_ids() -> Vec<u32>;
	fn relay_block_number() -> u32;
	fn set_relay_block_number(number: u32);
	fn process_messages();
	fn has_unprocessed_messages() -> bool;
	fn process_downward_messages();
	fn process_horizontal_messages();
	fn process_upward_messages();
	fn process_bridged_messages();
	fn hrmp_channel_parachain_inherent_data(
		para_id: u32,
		relay_parent_number: u32,
		parent_head_data: HeadData,
	) -> ParachainInherentData;
}

pub trait NetworkComponent {
	type Network: Network;

	fn send_horizontal_messages<I: Iterator<Item = (ParaId, RelayBlockNumber, Vec<u8>)>>(
		to_para_id: u32,
		iter: I,
	) {
		HORIZONTAL_MESSAGES.with(|b| {
			b.borrow_mut()
				.get_mut(Self::Network::name())
				.unwrap()
				.push_back((to_para_id, iter.collect()))
		});
	}

	fn send_upward_message(from_para_id: u32, msg: Vec<u8>) {
		UPWARD_MESSAGES.with(|b| {
			b.borrow_mut()
				.get_mut(Self::Network::name())
				.unwrap()
				.push_back((from_para_id, msg))
		});
	}

	fn send_downward_messages(
		to_para_id: u32,
		iter: impl Iterator<Item = (RelayBlockNumber, Vec<u8>)>,
	) {
		DOWNWARD_MESSAGES.with(|b| {
			b.borrow_mut()
				.get_mut(Self::Network::name())
				.unwrap()
				.push_back((to_para_id, iter.collect()))
		});
	}

	fn send_bridged_messages(msg: BridgeMessage) {
		BRIDGED_MESSAGES
			.with(|b| b.borrow_mut().get_mut(Self::Network::name()).unwrap().push_back(msg));
	}
}

pub trait Chain: TestExt + NetworkComponent {
	type Runtime: SystemConfig;
	type RuntimeCall;
	type RuntimeOrigin;
	type RuntimeEvent;
	type System;

	fn account_id_of(seed: &str) -> AccountId {
		helpers::get_account_id_from_seed::<sr25519::Public>(seed)
	}

	fn account_data_of(account: AccountId) -> AccountData<Balance>;

	fn events() -> Vec<<Self as Chain>::RuntimeEvent>;
}

pub trait RelayChain: Chain {
	type MessageProcessor: ProcessMessage;
	type SovereignAccountOf: ConvertLocation<AccountId>;

	fn child_location_of(id: ParaId) -> MultiLocation {
		(Ancestor(0), ParachainJunction(id.into())).into()
	}

	fn sovereign_account_id_of(location: MultiLocation) -> AccountId {
		Self::SovereignAccountOf::convert_location(&location).unwrap()
	}

	fn sovereign_account_id_of_child_para(id: ParaId) -> AccountId {
		Self::sovereign_account_id_of(Self::child_location_of(id))
	}
}

pub trait Parachain: Chain {
	type XcmpMessageHandler: XcmpMessageHandler;
	type DmpMessageHandler: DmpMessageHandler;
	type LocationToAccountId: ConvertLocation<AccountId>;
	type ParachainInfo: Get<ParaId>;
	type ParachainSystem;

	fn init();

	fn new_block();

	fn finalize_block();

	fn set_last_head();

	fn para_id() -> ParaId {
		Self::ext_wrapper(|| Self::ParachainInfo::get())
	}

	fn parent_location() -> MultiLocation {
		(Parent).into()
	}

	fn sibling_location_of(para_id: ParaId) -> MultiLocation {
		(Parent, X1(ParachainJunction(para_id.into()))).into()
	}

	fn sovereign_account_id_of(location: MultiLocation) -> AccountId {
		Self::LocationToAccountId::convert_location(&location).unwrap()
	}
}

pub trait Bridge {
	type Source: TestExt;
	type Target: TestExt;
	type Handler: BridgeMessageHandler;

	fn init();
}

impl Bridge for () {
	type Source = ();
	type Target = ();
	type Handler = ();

	fn init() {}
}

#[derive(Clone, Default, Debug)]
pub struct BridgeMessage {
	pub id: u32,
	pub nonce: u64,
	pub payload: Vec<u8>,
}

pub trait BridgeMessageHandler {
	fn get_source_outbound_messages() -> Vec<BridgeMessage>;

	fn dispatch_target_inbound_message(
		message: BridgeMessage,
	) -> Result<(), BridgeMessageDispatchError>;

	fn notify_source_message_delivery(lane_id: u32);
}

impl BridgeMessageHandler for () {
	fn get_source_outbound_messages() -> Vec<BridgeMessage> {
		Default::default()
	}

	fn dispatch_target_inbound_message(
		_message: BridgeMessage,
	) -> Result<(), BridgeMessageDispatchError> {
		Err(BridgeMessageDispatchError(Box::new("Not a bridge")))
	}

	fn notify_source_message_delivery(_lane_id: u32) {}
}

#[derive(Debug)]
pub struct BridgeMessageDispatchError(pub Box<dyn Debug>);

impl Error for BridgeMessageDispatchError {}

impl fmt::Display for BridgeMessageDispatchError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{:?}", self.0)
	}
}

// Relay Chain Implementation
#[macro_export]
macro_rules! decl_test_relay_chains {
	(
		$(
			#[api_version($api_version:tt)]
			pub struct $name:ident {
				genesis = $genesis:expr,
				on_init = $on_init:expr,
				runtime = $runtime:ident,
				core = {
					MessageProcessor: $mp:path,
					SovereignAccountOf: $sovereign_acc_of:path,

				},
				pallets = {
					$($pallet_name:ident: $pallet_path:path,)*
				}
			}
		),
		+
		$(,)?
	) => {
		$(
			#[derive(Clone)]
			pub struct $name;

			impl $crate::Chain for $name {
				type Runtime = $runtime::Runtime;
				type RuntimeCall = $runtime::RuntimeCall;
				type RuntimeOrigin = $runtime::RuntimeOrigin;
				type RuntimeEvent = $runtime::RuntimeEvent;
				type System = $crate::SystemPallet::<Self::Runtime>;

				fn account_data_of(account: $crate::AccountId) -> $crate::AccountData<$crate::Balance> {
					<Self as $crate::TestExt>::ext_wrapper(|| $crate::SystemPallet::<Self::Runtime>::account(account).data.into())
				}

				fn events() -> Vec<<Self as $crate::Chain>::RuntimeEvent> {
					Self::System::events()
						.iter()
						.map(|record| record.event.clone())
						.collect()
				}
			}

			impl $crate::RelayChain for $name {
				type SovereignAccountOf = $sovereign_acc_of;
				type MessageProcessor = $mp;
			}

			$crate::paste::paste! {
				pub trait [<$name Pallet>] {
					$(
						type $pallet_name;
					)?
				}

				impl [<$name Pallet>] for $name {
					$(
						type $pallet_name = $pallet_path;
					)?
				}
			}

			$crate::__impl_test_ext_for_relay_chain!($name, $genesis, $on_init, $api_version);
			$crate::__impl_check_assertion!($name);
		)+
	};
}

#[macro_export]
macro_rules! __impl_test_ext_for_relay_chain {
	// entry point: generate ext name
	($name:ident, $genesis:expr, $on_init:expr, $api_version:tt) => {
		$crate::paste::paste! {
			$crate::__impl_test_ext_for_relay_chain!(
				@impl $name,
				$genesis,
				$on_init,
				[<ParachainHostV $api_version>],
				[<LOCAL_EXT_ $name:upper>],
				[<GLOBAL_EXT_ $name:upper>]
			);
		}
	};
	// impl
	(@impl $name:ident, $genesis:expr, $on_init:expr, $api_version:ident, $local_ext:ident, $global_ext:ident) => {
		thread_local! {
			pub static $local_ext: $crate::RefCell<$crate::TestExternalities>
				= $crate::RefCell::new(<$name as $crate::TestExt>::build_new_ext($genesis));
		}

		$crate::lazy_static! {
			pub static ref $global_ext: $crate::Mutex<$crate::RefCell<$crate::HashMap<String, $crate::TestExternalities>>>
				= $crate::Mutex::new($crate::RefCell::new($crate::HashMap::new()));
		}

		impl $crate::TestExt for $name {
			fn build_new_ext(storage: $crate::Storage) -> $crate::TestExternalities {
				use $crate::{sp_tracing, NetworkComponent, Network, Chain, TestExternalities};

				let mut ext = TestExternalities::new(storage);

				ext.execute_with(|| {
					#[allow(clippy::no_effect)]
					$on_init;
					sp_tracing::try_init_simple();

					let mut block_number = <Self as Chain>::System::block_number();
					block_number = std::cmp::max(1, block_number);
					<Self as Chain>::System::set_block_number(block_number);
				});
				ext
			}

			fn new_ext() -> $crate::TestExternalities {
				<$name>::build_new_ext($genesis)
			}

			fn move_ext_out(id: &'static str) {
				use $crate::Deref;

				// Take TestExternality from thread_local
				let local_ext = $local_ext.with(|v| {
					v.take()
				});

				// Get TestExternality from lazy_static
				let global_ext_guard = $global_ext.lock().unwrap();

				// Replace TestExternality in lazy_static by TestExternality from thread_local
				global_ext_guard.deref().borrow_mut().insert(id.to_string(), local_ext);
			}

			fn move_ext_in(id: &'static str) {
				use $crate::Deref;

				let mut global_ext_unlocked = false;

				// Keep the mutex unlocked until TesExternality from lazy_static
				// has been updated
				while !global_ext_unlocked {
					// Get TesExternality from lazy_static
					let global_ext_result = $global_ext.try_lock();

					if let Ok(global_ext_guard) = global_ext_result {
						// Unlock the mutex as long as the condition is not met
						if !global_ext_guard.deref().borrow().contains_key(id) {
							drop(global_ext_guard);
						} else {
							global_ext_unlocked = true;
						}
					}
				}

				// Now that we know that lazy_static TestExt has been updated, we lock its mutex
				let mut global_ext_guard = $global_ext.lock().unwrap();

				// and set TesExternality from lazy_static into TesExternality for local_thread
				let global_ext = global_ext_guard.deref();

				$local_ext.with(|v| {
					v.replace(global_ext.take().remove(id).unwrap());
				});
			}

			fn reset_ext() {
				$local_ext.with(|v| *v.borrow_mut() = <$name>::build_new_ext($genesis));
			}

			fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
				use $crate::{Chain, NetworkComponent, Network};
				// Make sure the Network is initialized
				<$name as NetworkComponent>::Network::init();

				// Execute
				let r = $local_ext.with(|v| v.borrow_mut().execute_with(execute));

				// Send messages if needed
				$local_ext.with(|v| {
					v.borrow_mut().execute_with(|| {
						use $crate::polkadot_primitives::runtime_api::runtime_decl_for_parachain_host::$api_version;

						//TODO: mark sent count & filter out sent msg
						for para_id in<$name as NetworkComponent>::Network::para_ids() {
							// downward messages
							let downward_messages = <Self as $crate::Chain>::Runtime::dmq_contents(para_id.into())
								.into_iter()
								.map(|inbound| (inbound.sent_at, inbound.msg));
							if downward_messages.len() == 0 {
								continue;
							}
							<$name>::send_downward_messages(para_id, downward_messages.into_iter());

							// Note: no need to handle horizontal messages, as the
							// simulator directly sends them to dest (not relayed).
						}

						// log events
						Self::events().iter().for_each(|event| {
							$crate::log::debug!(target: concat!("events::", stringify!($name)), "{:?}", event);
						});

						// clean events
						<Self as Chain>::System::reset_events();
					})
				});

				<$name as NetworkComponent>::Network::process_messages();

				r
			}

			fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
				$local_ext.with(|v| {
					v.borrow_mut().execute_with(|| {
						func()
					})
				})
			}
		}
	};
}

// Parachain Implementation
#[macro_export]
macro_rules! decl_test_parachains {
	(
		$(
			pub struct $name:ident {
				genesis = $genesis:expr,
				on_init = $on_init:expr,
				runtime = $runtime:ident,
				core = {
					XcmpMessageHandler: $xcmp_message_handler:path,
					DmpMessageHandler: $dmp_message_handler:path,
					LocationToAccountId: $location_to_account:path,
					ParachainInfo: $parachain_info:path,
				},
				pallets = {
					$($pallet_name:ident: $pallet_path:path,)*
				}
			}
		),
		+
		$(,)?
	) => {
		$(
			#[derive(Clone)]
			pub struct $name;

			impl $crate::Chain for $name {
				type Runtime = $runtime::Runtime;
				type RuntimeCall = $runtime::RuntimeCall;
				type RuntimeOrigin = $runtime::RuntimeOrigin;
				type RuntimeEvent = $runtime::RuntimeEvent;
				type System = $crate::SystemPallet::<Self::Runtime>;

				fn account_data_of(account: $crate::AccountId) -> $crate::AccountData<$crate::Balance> {
					<Self as $crate::TestExt>::ext_wrapper(|| $crate::SystemPallet::<Self::Runtime>::account(account).data.into())
				}

				fn events() -> Vec<<Self as $crate::Chain>::RuntimeEvent> {
					Self::System::events()
						.iter()
						.map(|record| record.event.clone())
						.collect()
				}
			}

			impl $crate::Parachain for $name {
				type XcmpMessageHandler = $xcmp_message_handler;
				type DmpMessageHandler = $dmp_message_handler;
				type LocationToAccountId = $location_to_account;
				type ParachainSystem = $crate::ParachainSystemPallet<<Self as $crate::Chain>::Runtime>;
				type ParachainInfo = $parachain_info;

				// We run an empty block during initialisation to open HRMP channels
				// and have them ready for the next block
				fn init() {
					use $crate::{Chain, HeadData, Network, NetworkComponent, Hooks, Encode, Parachain, TestExt};
					// Set the last block head for later use in the next block
					Self::set_last_head();
					// Initialize a new block
					Self::new_block();
					// Finalize the new block
					Self::finalize_block();
				}

				fn new_block() {
					use $crate::{Chain, HeadData, Network, NetworkComponent, Hooks, Encode, Parachain, TestExt};

					let para_id = Self::para_id().into();

					Self::ext_wrapper(|| {
						// Increase Relay Chain block number
						let mut relay_block_number = <$name as NetworkComponent>::Network::relay_block_number();
						relay_block_number += 1;
						<$name as NetworkComponent>::Network::set_relay_block_number(relay_block_number);

						// Initialize a new Parachain block
						let mut block_number = <Self as Chain>::System::block_number();
						block_number += 1;
						let parent_head_data = $crate::LAST_HEAD.with(|b| b.borrow_mut()
							.get_mut(<Self as NetworkComponent>::Network::name())
							.expect("network not initialized?")
							.get(&para_id)
							.expect("network not initialized?")
							.clone()
						);
						<Self as Chain>::System::initialize(&block_number, &parent_head_data.hash(), &Default::default());
						<<Self as Parachain>::ParachainSystem as Hooks<$crate::BlockNumber>>::on_initialize(block_number);

						let _ = <Self as Parachain>::ParachainSystem::set_validation_data(
							<Self as Chain>::RuntimeOrigin::none(),
							<$name as NetworkComponent>::Network::hrmp_channel_parachain_inherent_data(para_id, relay_block_number, parent_head_data),
						);
					});
				}

				fn finalize_block() {
					use $crate::{Chain, Encode, Hooks, Network, NetworkComponent, Parachain, TestExt};

					Self::ext_wrapper(|| {
						let block_number = <Self as Chain>::System::block_number();
						<Self as Parachain>::ParachainSystem::on_finalize(block_number);
					});

					Self::set_last_head();
				}


				fn set_last_head() {
					use $crate::{Chain, Encode, HeadData, Network, NetworkComponent, Parachain, TestExt};

					let para_id = Self::para_id().into();

					Self::ext_wrapper(|| {
						// Store parent head data for use later.
						let created_header = <Self as Chain>::System::finalize();
						$crate::LAST_HEAD.with(|b| b.borrow_mut()
							.get_mut(<Self as NetworkComponent>::Network::name())
							.expect("network not initialized?")
							.insert(para_id, HeadData(created_header.encode()))
						);
					});
				}
			}

			$crate::paste::paste! {
				pub trait [<$name Pallet>] {
					$(
						type $pallet_name;
					)*
				}

				impl [<$name Pallet>] for $name {
					$(
						type $pallet_name = $pallet_path;
					)*
				}
			}

			$crate::__impl_test_ext_for_parachain!($name, $genesis, $on_init);
			$crate::__impl_check_assertion!($name);
		)+
	};
}

#[macro_export]
macro_rules! __impl_test_ext_for_parachain {
	// entry point: generate ext name
	($name:ident, $genesis:expr, $on_init:expr) => {
		$crate::paste::paste! {
			$crate::__impl_test_ext_for_parachain!(@impl $name, $genesis, $on_init, [<LOCAL_EXT_ $name:upper>], [<GLOBAL_EXT_ $name:upper>]);
		}
	};
	// impl
	(@impl $name:ident, $genesis:expr, $on_init:expr, $local_ext:ident, $global_ext:ident) => {
		thread_local! {
			pub static $local_ext: $crate::RefCell<$crate::TestExternalities>
				= $crate::RefCell::new(<$name as $crate::TestExt>::build_new_ext($genesis));
		}

		$crate::lazy_static! {
			pub static ref $global_ext: $crate::Mutex<$crate::RefCell<$crate::HashMap<String, $crate::TestExternalities>>>
				= $crate::Mutex::new($crate::RefCell::new($crate::HashMap::new()));
		}

		impl $crate::TestExt for $name {
			fn build_new_ext(storage: $crate::Storage) -> $crate::TestExternalities {
				let mut ext = $crate::TestExternalities::new(storage);

				ext.execute_with(|| {
					#[allow(clippy::no_effect)]
					$on_init;
					$crate::sp_tracing::try_init_simple();

					let mut block_number = <Self as $crate::Chain>::System::block_number();
					block_number = std::cmp::max(1, block_number);
					<Self as $crate::Chain>::System::set_block_number(block_number);
				});
				ext
			}

			fn new_ext() -> $crate::TestExternalities {
				<$name>::build_new_ext($genesis)
			}

			fn move_ext_out(id: &'static str) {
				use $crate::Deref;

				// Take TestExternality from thread_local
				let local_ext = $local_ext.with(|v| {
					v.take()
				});

				// Get TestExternality from lazy_static
				let global_ext_guard = $global_ext.lock().unwrap();

				// Replace TestExternality in lazy_static by TestExternality from thread_local
				global_ext_guard.deref().borrow_mut().insert(id.to_string(), local_ext);
			}

			fn move_ext_in(id: &'static str) {
				use $crate::Deref;

				let mut global_ext_unlocked = false;

				// Keep the mutex unlocked until TesExternality from lazy_static
				// has been updated
				while !global_ext_unlocked {
					// Get TesExternality from lazy_static
					let global_ext_result = $global_ext.try_lock();

					if let Ok(global_ext_guard) = global_ext_result {
						// Unlock the mutex as long as the condition is not met
						if !global_ext_guard.deref().borrow().contains_key(id) {
							drop(global_ext_guard);
						} else {
							global_ext_unlocked = true;
						}
					}
				}

				// Now that we know that lazy_static TestExt has been updated, we lock its mutex
				let mut global_ext_guard = $global_ext.lock().unwrap();

				// and set TesExternality from lazy_static into TesExternality for local_thread
				let global_ext = global_ext_guard.deref();

				$local_ext.with(|v| {
					v.replace(global_ext.take().remove(id).unwrap());
				});
			}

			fn reset_ext() {
				$local_ext.with(|v| *v.borrow_mut() = <$name>::build_new_ext($genesis));
			}

			fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
				use $crate::{Chain, Get, Hooks, NetworkComponent, Network, Parachain, Encode};

				// Make sure the Network is initialized
				<$name as NetworkComponent>::Network::init();

				// Initialize a new block
				Self::new_block();

				// Execute
				let r = $local_ext.with(|v| v.borrow_mut().execute_with(execute));

				// Finalize the block
				Self::finalize_block();

				let para_id = <$name>::para_id().into();

				// Send messages if needed
				$local_ext.with(|v| {
					v.borrow_mut().execute_with(|| {
						let mock_header = $crate::HeaderT::new(
							0,
							Default::default(),
							Default::default(),
							Default::default(),
							Default::default(),
						);

						let collation_info = <Self as Parachain>::ParachainSystem::collect_collation_info(&mock_header);

						// send upward messages
						let relay_block_number = <$name as NetworkComponent>::Network::relay_block_number();
						for msg in collation_info.upward_messages.clone() {
							<$name>::send_upward_message(para_id, msg);
						}

						// send horizontal messages
						for msg in collation_info.horizontal_messages {
							<$name>::send_horizontal_messages(
								msg.recipient.into(),
								vec![(para_id.into(), relay_block_number, msg.data)].into_iter(),
							);
						}

						// get bridge messages
						type NetworkBridge = <<$name as NetworkComponent>::Network as $crate::Network>::Bridge;

						let bridge_messages = <<NetworkBridge as $crate::Bridge>::Handler as $crate::BridgeMessageHandler>::get_source_outbound_messages();

						// send bridged messages
						for msg in bridge_messages {
							<$name>::send_bridged_messages(msg);
						}

						// log events
						<Self as $crate::Chain>::events().iter().for_each(|event| {
							$crate::log::debug!(target: concat!("events::", stringify!($name)), "{:?}", event);
						});

						// clean events
						<Self as $crate::Chain>::System::reset_events();
					})
				});

				// provide inbound DMP/HRMP messages through a side-channel.
				// normally this would come through the `set_validation_data`,
				// but we go around that.
				<$name as NetworkComponent>::Network::process_messages();

				r
			}

			fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
				$local_ext.with(|v| {
					v.borrow_mut().execute_with(|| {
						func()
					})
				})
			}
		}
	};
}

// Network Implementation
#[macro_export]
macro_rules! decl_test_networks {
	(
		$(
			pub struct $name:ident {
				relay_chain = $relay_chain:ty,
				parachains = vec![ $( $parachain:ty, )* ],
				bridge = $bridge:ty
			}
		),
		+
		$(,)?
	) => {
		$(
			pub struct $name;

			impl $crate::Network for $name {
				type Relay = $relay_chain;
				type Bridge = $bridge;

				fn name() -> &'static str {
					$crate::type_name::<Self>()
				}

				fn reset() {
					use $crate::{TestExt};

					$crate::INITIALIZED.with(|b| b.borrow_mut().remove(Self::name()));
					$crate::DOWNWARD_MESSAGES.with(|b| b.borrow_mut().remove(Self::name()));
					$crate::DMP_DONE.with(|b| b.borrow_mut().remove(Self::name()));
					$crate::UPWARD_MESSAGES.with(|b| b.borrow_mut().remove(Self::name()));
					$crate::HORIZONTAL_MESSAGES.with(|b| b.borrow_mut().remove(Self::name()));
					$crate::BRIDGED_MESSAGES.with(|b| b.borrow_mut().remove(Self::name()));
					$crate::LAST_HEAD.with(|b| b.borrow_mut().remove(Self::name()));

					<$relay_chain>::reset_ext();
					$( <$parachain>::reset_ext(); )*
				}

				fn init() {
					// If Network has not been initialized yet, it gets initialized
					if $crate::INITIALIZED.with(|b| b.borrow_mut().get(Self::name()).is_none()) {
						$crate::INITIALIZED.with(|b| b.borrow_mut().insert(Self::name().to_string(), true));
						$crate::DOWNWARD_MESSAGES.with(|b| b.borrow_mut().insert(Self::name().to_string(), $crate::VecDeque::new()));
						$crate::DMP_DONE.with(|b| b.borrow_mut().insert(Self::name().to_string(), $crate::VecDeque::new()));
						$crate::UPWARD_MESSAGES.with(|b| b.borrow_mut().insert(Self::name().to_string(), $crate::VecDeque::new()));
						$crate::HORIZONTAL_MESSAGES.with(|b| b.borrow_mut().insert(Self::name().to_string(), $crate::VecDeque::new()));
						$crate::BRIDGED_MESSAGES.with(|b| b.borrow_mut().insert(Self::name().to_string(), $crate::VecDeque::new()));
						$crate::PARA_IDS.with(|b| b.borrow_mut().insert(Self::name().to_string(), Self::para_ids()));
						$crate::LAST_HEAD.with(|b| b.borrow_mut().insert(Self::name().to_string(), $crate::HashMap::new()));

						$( <$parachain as $crate::Parachain>::init(); )*
					}
				}

				fn para_ids() -> Vec<u32> {
					vec![$(
						<$parachain as $crate::Parachain>::para_id().into(),
					)*]
				}

				fn relay_block_number() -> u32 {
					<Self::Relay as $crate::TestExt>::ext_wrapper(|| {
						<Self::Relay as $crate::Chain>::System::block_number()
					})
				}

				fn set_relay_block_number(number: u32) {
					<Self::Relay as $crate::TestExt>::ext_wrapper(|| {
						<Self::Relay as $crate::Chain>::System::set_block_number(number);
					})
				}

				fn process_messages() {
					while Self::has_unprocessed_messages() {
						Self::process_upward_messages();
						Self::process_horizontal_messages();
						Self::process_downward_messages();
						Self::process_bridged_messages();
					}
				}

				fn has_unprocessed_messages() -> bool {
					$crate::DOWNWARD_MESSAGES.with(|b| !b.borrow_mut().get_mut(Self::name()).unwrap().is_empty())
					|| $crate::HORIZONTAL_MESSAGES.with(|b| !b.borrow_mut().get_mut(Self::name()).unwrap().is_empty())
					|| $crate::UPWARD_MESSAGES.with(|b| !b.borrow_mut().get_mut(Self::name()).unwrap().is_empty())
					|| $crate::BRIDGED_MESSAGES.with(|b| !b.borrow_mut().get_mut(Self::name()).unwrap().is_empty())
				}

				fn process_downward_messages() {
					use $crate::{DmpMessageHandler, Bounded, Parachain, RelayChainBlockNumber, TestExt};

					while let Some((to_para_id, messages))
						= $crate::DOWNWARD_MESSAGES.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().pop_front()) {
						$(
							let para_id: u32 = <$parachain>::para_id().into();

							if $crate::PARA_IDS.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().contains(&to_para_id)) && para_id == to_para_id {
								let mut msg_dedup: Vec<(RelayChainBlockNumber, Vec<u8>)> = Vec::new();
								for m in &messages {
									msg_dedup.push((m.0, m.1.clone()));
								}
								msg_dedup.dedup();

								let msgs = msg_dedup.clone().into_iter().filter(|m| {
									!$crate::DMP_DONE.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap_or(&mut $crate::VecDeque::new()).contains(&(to_para_id, m.0, m.1.clone())))
								}).collect::<Vec<(RelayChainBlockNumber, Vec<u8>)>>();
								if msgs.len() != 0 {
									<$parachain>::ext_wrapper(|| {
										<$parachain as Parachain>::DmpMessageHandler::handle_dmp_messages(msgs.clone().into_iter(), $crate::Weight::max_value());
									});
									$crate::log::debug!(target: concat!("dmp::", stringify!($name)) , "DMP messages processed {:?} to para_id {:?}", msgs.clone(), &to_para_id);
									for m in msgs {
										$crate::DMP_DONE.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().push_back((to_para_id, m.0, m.1)));
									}
								}
							}
						)*
					}
				}

				fn process_horizontal_messages() {
					use $crate::{XcmpMessageHandler, Bounded, Parachain, TestExt};

					while let Some((to_para_id, messages))
						= $crate::HORIZONTAL_MESSAGES.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().pop_front()) {
						let iter = messages.iter().map(|(p, b, m)| (*p, *b, &m[..])).collect::<Vec<_>>().into_iter();
						$(
							let para_id: u32 = <$parachain>::para_id().into();

							if $crate::PARA_IDS.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().contains(&to_para_id)) && para_id == to_para_id {
								<$parachain>::ext_wrapper(|| {
									<$parachain as Parachain>::XcmpMessageHandler::handle_xcmp_messages(iter.clone(), $crate::Weight::max_value());
								});
								$crate::log::debug!(target: concat!("hrmp::", stringify!($name)) , "HRMP messages processed {:?} to para_id {:?}", &messages, &to_para_id);
							}
						)*
					}
				}

				fn process_upward_messages() {
					use $crate::{Encode, ProcessMessage, TestExt};

					while let Some((from_para_id, msg)) = $crate::UPWARD_MESSAGES.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().pop_front()) {
						let mut weight_meter = $crate::WeightMeter::new();
						<$relay_chain>::ext_wrapper(|| {
							let _ =  <$relay_chain as $crate::RelayChain>::MessageProcessor::process_message(
								&msg[..],
								from_para_id.into(),
								&mut weight_meter,
								&mut msg.using_encoded($crate::blake2_256),
							);
						});
						$crate::log::debug!(target: concat!("ump::", stringify!($name)) , "Upward message processed {:?} from para_id {:?}", &msg, &from_para_id);
					}
				}

				fn process_bridged_messages() {
					use $crate::{Bridge, BridgeMessageHandler, TestExt};
					// Make sure both, including the target `Network` are initialized
					<Self::Bridge as Bridge>::init();

					while let Some(msg) = $crate::BRIDGED_MESSAGES.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().pop_front()) {
						let dispatch_result = <<Self::Bridge as Bridge>::Target as TestExt>::ext_wrapper(|| {
							<<Self::Bridge as Bridge>::Handler as BridgeMessageHandler>::dispatch_target_inbound_message(msg.clone())
						});

						match dispatch_result {
							Err(e) => panic!("Error {:?} processing bridged message: {:?}", e, msg.clone()),
							Ok(()) => {
								<<Self::Bridge as Bridge>::Source as TestExt>::ext_wrapper(|| {
									<<Self::Bridge as Bridge>::Handler as BridgeMessageHandler>::notify_source_message_delivery(msg.id);
								});
								$crate::log::debug!(target: concat!("bridge::", stringify!($name)) , "Bridged message processed {:?}", msg.clone());
							}
						}
					}
				}

				fn hrmp_channel_parachain_inherent_data(
					para_id: u32,
					relay_parent_number: u32,
					parent_head_data: $crate::HeadData,
				) -> $crate::ParachainInherentData {
					let mut sproof = $crate::RelayStateSproofBuilder::default();
					sproof.para_id = para_id.into();

					// egress channel
					let e_index = sproof.hrmp_egress_channel_index.get_or_insert_with(Vec::new);
					for recipient_para_id in $crate::PARA_IDS.with(|b| b.borrow_mut().get_mut(Self::name()).unwrap().clone()) {
						let recipient_para_id = $crate::ParaId::from(recipient_para_id);
						if let Err(idx) = e_index.binary_search(&recipient_para_id) {
							e_index.insert(idx, recipient_para_id);
						}

						sproof.included_para_head = parent_head_data.clone().into();

						sproof
							.hrmp_channels
							.entry($crate::HrmpChannelId {
								sender: sproof.para_id,
								recipient: recipient_para_id,
							})
							.or_insert_with(|| $crate::AbridgedHrmpChannel {
								max_capacity: 1024,
								max_total_size: 1024 * 1024,
								max_message_size: 1024 * 1024,
								msg_count: 0,
								total_size: 0,
								mqc_head: Option::None,
							});
					}

					let (relay_storage_root, proof) = sproof.into_state_root_and_proof();

					$crate::ParachainInherentData {
						validation_data: $crate::PersistedValidationData {
							parent_head: Default::default(),
							relay_parent_number,
							relay_parent_storage_root: relay_storage_root,
							max_pov_size: Default::default(),
						},
						relay_chain_state: proof,
						downward_messages: Default::default(),
						horizontal_messages: Default::default(),
					}
				}
			}

			impl $crate::NetworkComponent for $relay_chain {
				type Network = $name;
			}

			$(
				impl $crate::NetworkComponent for $parachain {
					type Network = $name;
				}
			)*
		)+
	};
}

#[macro_export]
macro_rules! decl_test_bridges {
	(
		$(
			pub struct $name:ident {
				source = $source:ty,
				target = $target:ty,
				handler = $handler:ty
			}
		),
		+
		$(,)?
	) => {
		$(
			#[derive(Debug)]
			pub struct $name;

			impl $crate::Bridge for $name {
				type Source = $source;
				type Target = $target;
				type Handler = $handler;

				fn init() {
					use $crate::{NetworkComponent, Network};
					// Make sure source and target `Network` have been initialized
					<$source as NetworkComponent>::Network::init();
					<$target as NetworkComponent>::Network::init();
				}
			}
		)+
	};
}

#[macro_export]
macro_rules! __impl_check_assertion {
	($chain:ident) => {
		impl<Origin, Destination, Hops, Args>
			$crate::CheckAssertion<Origin, Destination, Hops, Args> for $chain
		where
			Origin: $crate::Chain + Clone,
			Destination: $crate::Chain + Clone,
			Origin::RuntimeOrigin: $crate::OriginTrait<AccountId = $crate::AccountId32> + Clone,
			Destination::RuntimeOrigin:
				$crate::OriginTrait<AccountId = $crate::AccountId32> + Clone,
			Hops: Clone,
			Args: Clone,
		{
			fn check_assertion(test: $crate::Test<Origin, Destination, Hops, Args>) {
				use $crate::TestExt;

				let chain_name = std::any::type_name::<$chain>();

				<$chain>::execute_with(|| {
					if let Some(dispatchable) = test.hops_dispatchable.get(chain_name) {
						$crate::assert_ok!(dispatchable(test.clone()));
					}
					if let Some(assertion) = test.hops_assertion.get(chain_name) {
						assertion(test);
					}
				});
			}
		}
	};
}

#[macro_export]
macro_rules! assert_expected_events {
	( $chain:ident, vec![$( $event_pat:pat => { $($attr:ident : $condition:expr, )* }, )*] ) => {
		let mut message: Vec<String> = Vec::new();
		let mut events = <$chain as $crate::Chain>::events();

		$(
			let mut event_received = false;
			let mut meet_conditions = true;
			let mut index_match = 0;
			let mut event_message: Vec<String> = Vec::new();

			for (index, event) in events.iter().enumerate() {
				// Have to reset the variable to override a previous partial match
				meet_conditions = true;
				match event {
					$event_pat => {
						event_received = true;
						let mut conditions_message: Vec<String> = Vec::new();

						$(
							// We only want to record condition error messages in case it did not happened before
							// Only the first partial match is recorded
							if !$condition && event_message.is_empty() {
								conditions_message.push(
									format!(
										" - The attribute {:?} = {:?} did not met the condition {:?}\n",
										stringify!($attr),
										$attr,
										stringify!($condition)
									)
								);
							}
							meet_conditions &= $condition;
						)*

						// Set the index where we found a perfect match
						if event_received && meet_conditions {
							index_match = index;
							break;
						} else {
							event_message.extend(conditions_message);
						}
					},
					_ => {}
				}
			}

			if event_received && !meet_conditions  {
				message.push(
					format!(
						"\n\n{}::\x1b[31m{}\x1b[0m was received but some of its attributes did not meet the conditions:\n{}",
						stringify!($chain),
						stringify!($event_pat),
						event_message.concat()
					)
				);
			} else if !event_received {
				message.push(format!("\n\n{}::\x1b[31m{}\x1b[0m was never received", stringify!($chain), stringify!($event_pat)));
			} else {
				// If we find a perfect match we remove the event to avoid being potentially assessed multiple times
				events.remove(index_match);
			}
		)*

		if !message.is_empty() {
			// Log events as they will not be logged after the panic
			<$chain as $crate::Chain>::events().iter().for_each(|event| {
				$crate::log::debug!(target: concat!("events::", stringify!($chain)), "{:?}", event);
			});
			panic!("{}", message.concat())
		}
	}
}

#[macro_export]
macro_rules! bx {
	($e:expr) => {
		Box::new($e)
	};
}

#[macro_export]
macro_rules! decl_test_sender_receiver_accounts_parameter_types {
	( $( $chain:ident { sender: $sender:expr, receiver: $receiver:expr }),+ ) => {
		$crate::paste::paste! {
			$crate::parameter_types! {
				$(
					pub [<$chain Sender>]: $crate::AccountId = <$chain as $crate::Chain>::account_id_of($sender);
					pub [<$chain Receiver>]: $crate::AccountId = <$chain as $crate::Chain>::account_id_of($receiver);
				)+
			}
		}
	};
}

pub struct DefaultMessageProcessor<T>(PhantomData<T>);
impl<T> ProcessMessage for DefaultMessageProcessor<T>
where
	T: Chain + RelayChain,
	T::Runtime: MessageQueueConfig,
	<<T::Runtime as MessageQueueConfig>::MessageProcessor as ProcessMessage>::Origin:
		PartialEq<AggregateMessageOrigin>,
	MessageQueuePallet<T::Runtime>: EnqueueMessage<AggregateMessageOrigin> + ServiceQueues,
{
	type Origin = ParaId;

	fn process_message(
		msg: &[u8],
		para: Self::Origin,
		_meter: &mut WeightMeter,
		_id: &mut XcmHash,
	) -> Result<bool, ProcessMessageError> {
		MessageQueuePallet::<T::Runtime>::enqueue_message(
			msg.try_into().expect("Message too long"),
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)),
		);
		MessageQueuePallet::<T::Runtime>::service_queues(Weight::MAX);

		Ok(true)
	}
}

/// Struct that keeps account's id and balance
#[derive(Clone)]
pub struct TestAccount {
	pub account_id: AccountId,
	pub balance: Balance,
}

/// Default `Args` provided by xcm-emulator to be stored in a `Test` instance
#[derive(Clone)]
pub struct TestArgs {
	pub dest: MultiLocation,
	pub beneficiary: MultiLocation,
	pub amount: Balance,
	pub assets: MultiAssets,
	pub asset_id: Option<u32>,
	pub fee_asset_item: u32,
	pub weight_limit: WeightLimit,
}

/// Auxiliar struct to help creating a new `Test` instance
pub struct TestContext<T> {
	pub sender: AccountId,
	pub receiver: AccountId,
	pub args: T,
}

/// Struct that help with tests where either dispatchables or assertions need
/// to be reused. The struct keeps the test's arguments of your choice in the generic `Args`.
/// These arguments can be easily reused and shared between the assertions functions
/// and dispatchables functions, which are also stored in `Test`.
/// `Origin` corresponds to the chain where the XCM interaction starts with an initial execution.
/// `Destination` corresponds to the last chain where an effect of the intial execution is expected
/// happen. `Hops` refer all the ordered intermediary chains an initial XCM execution can provoke
/// some effect.
#[derive(Clone)]
pub struct Test<Origin, Destination, Hops = (), Args = TestArgs>
where
	Origin: Chain + Clone,
	Destination: Chain + Clone,
	Origin::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Destination::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Hops: Clone,
{
	pub sender: TestAccount,
	pub receiver: TestAccount,
	pub signed_origin: Origin::RuntimeOrigin,
	pub root_origin: Origin::RuntimeOrigin,
	pub hops_assertion: HashMap<String, fn(Self)>,
	pub hops_dispatchable: HashMap<String, fn(Self) -> DispatchResult>,
	pub args: Args,
	_marker: PhantomData<(Destination, Hops)>,
}

/// `Test` implementation
impl<Origin, Destination, Hops, Args> Test<Origin, Destination, Hops, Args>
where
	Args: Clone,
	Origin: Chain + Clone + CheckAssertion<Origin, Destination, Hops, Args>,
	Destination: Chain + Clone + CheckAssertion<Origin, Destination, Hops, Args>,
	Origin::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Destination::RuntimeOrigin: OriginTrait<AccountId = AccountId32> + Clone,
	Hops: Clone + CheckAssertion<Origin, Destination, Hops, Args>,
{
	/// Creates a new `Test` instance
	pub fn new(test_args: TestContext<Args>) -> Self {
		Test {
			sender: TestAccount {
				account_id: test_args.sender.clone(),
				balance: Origin::account_data_of(test_args.sender.clone()).free,
			},
			receiver: TestAccount {
				account_id: test_args.receiver.clone(),
				balance: Destination::account_data_of(test_args.receiver.clone()).free,
			},
			signed_origin: <Origin as Chain>::RuntimeOrigin::signed(test_args.sender),
			root_origin: <Origin as Chain>::RuntimeOrigin::root(),
			hops_assertion: Default::default(),
			hops_dispatchable: Default::default(),
			args: test_args.args,
			_marker: Default::default(),
		}
	}
	/// Stores an assertion in a particular Chain
	pub fn set_assertion<Hop>(&mut self, assertion: fn(Self)) {
		let chain_name = std::any::type_name::<Hop>();
		self.hops_assertion.insert(chain_name.to_string(), assertion);
	}
	/// Stores an assertion in a particular Chain
	pub fn set_dispatchable<Hop>(&mut self, dispatchable: fn(Self) -> DispatchResult) {
		let chain_name = std::any::type_name::<Hop>();
		self.hops_dispatchable.insert(chain_name.to_string(), dispatchable);
	}
	/// Executes all dispatchables and assertions in order from `Origin` to `Destination`
	pub fn assert(&mut self) {
		Origin::check_assertion(self.clone());
		Hops::check_assertion(self.clone());
		Destination::check_assertion(self.clone());
		Self::update_balances(self);
	}
	/// Updates sender and receiver balances
	fn update_balances(&mut self) {
		self.sender.balance = Origin::account_data_of(self.sender.account_id.clone()).free;
		self.receiver.balance = Destination::account_data_of(self.receiver.account_id.clone()).free;
	}
}

pub mod helpers {
	use super::*;

	pub fn within_threshold(threshold: u64, expected_value: u64, current_value: u64) -> bool {
		let margin = (current_value * threshold) / 100;
		let lower_limit = expected_value.checked_sub(margin).unwrap_or(u64::MIN);
		let upper_limit = expected_value.checked_add(margin).unwrap_or(u64::MAX);

		current_value >= lower_limit && current_value <= upper_limit
	}

	pub fn weight_within_threshold(
		(threshold_time, threshold_size): (u64, u64),
		expected_weight: Weight,
		weight: Weight,
	) -> bool {
		let ref_time_within =
			within_threshold(threshold_time, expected_weight.ref_time(), weight.ref_time());
		let proof_size_within =
			within_threshold(threshold_size, expected_weight.proof_size(), weight.proof_size());

		ref_time_within && proof_size_within
	}

	/// Helper function to generate an account ID from seed.
	pub fn get_account_id_from_seed<TPublic: sp_core::Public>(seed: &str) -> AccountId
	where
		sp_runtime::MultiSigner:
			From<<<TPublic as sp_runtime::CryptoType>::Pair as sp_core::Pair>::Public>,
	{
		use sp_runtime::traits::IdentifyAccount;
		let pubkey = TPublic::Pair::from_string(&format!("//{}", seed), None)
			.expect("static values are valid; qed")
			.public();
		sp_runtime::MultiSigner::from(pubkey).into_account()
	}
}
