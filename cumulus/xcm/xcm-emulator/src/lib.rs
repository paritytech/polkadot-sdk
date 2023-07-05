// Copyright 2023 Parity Technologies (UK) Ltd.
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

pub use codec::{Decode, Encode};
pub use log;
pub use paste;
pub use std::{collections::HashMap, error::Error, fmt, thread::LocalKey};

// Substrate
pub use frame_support::{
	assert_ok,
	traits::{EnqueueMessage, Get, Hooks, ProcessMessage, ProcessMessageError, ServiceQueues},
	weights::{Weight, WeightMeter},
};
pub use frame_system::AccountInfo;
pub use pallet_balances::AccountData;
pub use sp_arithmetic::traits::Bounded;
pub use sp_core::{storage::Storage, H256};
pub use sp_io;
pub use sp_std::{cell::RefCell, collections::vec_deque::VecDeque, fmt::Debug};
pub use sp_trie::StorageProof;

//Cumulus
pub use cumulus_pallet_dmp_queue;
pub use cumulus_pallet_parachain_system;
pub use cumulus_pallet_xcmp_queue;
pub use cumulus_primitives_core::{
	self, relay_chain::BlockNumber as RelayBlockNumber, DmpMessageHandler, ParaId,
	PersistedValidationData, XcmpMessageHandler,
};
pub use cumulus_primitives_parachain_inherent::ParachainInherentData;
pub use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
pub use cumulus_test_service::get_account_id_from_seed;
pub use pallet_message_queue;
pub use parachain_info;
pub use parachains_common::{AccountId, BlockNumber};
pub use polkadot_primitives;
pub use polkadot_runtime_parachains::{
	dmp,
	inclusion::{AggregateMessageOrigin, UmpQueueId},
};

// Polkadot
pub use xcm::v3::prelude::*;

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
	/// Global incremental relay chain block number
	pub static RELAY_BLOCK_NUMBER: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
	/// Parachains Ids a the Network
	pub static PARA_IDS: RefCell<HashMap<String, Vec<u32>>> = RefCell::new(HashMap::new());
	/// Flag indicating if global variables have been initialized for a certain Network
	pub static INITIALIZED: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
}

pub trait TestExt {
	fn build_new_ext(storage: Storage) -> sp_io::TestExternalities;
	fn new_ext() -> sp_io::TestExternalities;
	fn reset_ext();
	fn execute_with<R>(execute: impl FnOnce() -> R) -> R;
	fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R;
}

impl TestExt for () {
	fn build_new_ext(_storage: Storage) -> sp_io::TestExternalities {
		sp_io::TestExternalities::default()
	}
	fn new_ext() -> sp_io::TestExternalities {
		sp_io::TestExternalities::default()
	}
	fn reset_ext() {}
	fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
		execute()
	}
	fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
		func()
	}
}

pub trait Network {
	type Bridge: Bridge;

	fn init();
	fn para_ids() -> Vec<u32>;
	fn relay_block_number() -> u32;
	fn set_relay_block_number(block_number: u32);
	fn process_messages();
	fn has_unprocessed_messages() -> bool;
	fn process_downward_messages();
	fn process_horizontal_messages();
	fn process_upward_messages();
	fn process_bridged_messages();
	fn hrmp_channel_parachain_inherent_data(
		para_id: u32,
		relay_parent_number: u32,
	) -> ParachainInherentData;
}

pub trait NetworkComponent {
	type Network: Network;

	fn network_name() -> &'static str;

	fn send_horizontal_messages<I: Iterator<Item = (ParaId, RelayBlockNumber, Vec<u8>)>>(
		to_para_id: u32,
		iter: I,
	) {
		HORIZONTAL_MESSAGES.with(|b| {
			b.borrow_mut()
				.get_mut(Self::network_name())
				.unwrap()
				.push_back((to_para_id, iter.collect()))
		});
	}

	fn send_upward_message(from_para_id: u32, msg: Vec<u8>) {
		UPWARD_MESSAGES.with(|b| {
			b.borrow_mut()
				.get_mut(Self::network_name())
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
				.get_mut(Self::network_name())
				.unwrap()
				.push_back((to_para_id, iter.collect()))
		});
	}

	fn send_bridged_messages(msg: BridgeMessage) {
		BRIDGED_MESSAGES
			.with(|b| b.borrow_mut().get_mut(Self::network_name()).unwrap().push_back(msg));
	}
}

pub trait RelayChain: ProcessMessage {
	type Runtime;
	type RuntimeOrigin;
	type RuntimeCall;
	type RuntimeEvent;
	type XcmConfig;
	type SovereignAccountOf;
	type System;
	type Balances;
}

pub trait Parachain: XcmpMessageHandler + DmpMessageHandler {
	type Runtime;
	type RuntimeOrigin;
	type RuntimeCall;
	type RuntimeEvent;
	type XcmpMessageHandler;
	type DmpMessageHandler;
	type LocationToAccountId;
	type System;
	type Balances;
	type ParachainSystem;
	type ParachainInfo;
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
				runtime = {
					Runtime: $runtime:path,
					RuntimeOrigin: $runtime_origin:path,
					RuntimeCall: $runtime_call:path,
					RuntimeEvent: $runtime_event:path,
					MessageQueue: $mq:path,
					XcmConfig: $xcm_config:path,
					SovereignAccountOf: $sovereign_acc_of:path,
					System: $system:path,
					Balances: $balances:path,
				},
				pallets_extra = {
					$($pallet_name:ident: $pallet_path:path,)*
				}
			}
		),
		+
	) => {
		$(
			pub struct $name;

			impl RelayChain for $name {
				type Runtime = $runtime;
				type RuntimeOrigin = $runtime_origin;
				type RuntimeCall = $runtime_call;
				type RuntimeEvent = $runtime_event;
				type XcmConfig = $xcm_config;
				type SovereignAccountOf = $sovereign_acc_of;
				type System = $system;
				type Balances = $balances;
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

			impl $crate::ProcessMessage for $name {
				type Origin = $crate::ParaId;

				fn process_message(
					msg: &[u8],
					para: Self::Origin,
					meter: &mut $crate::WeightMeter,
					_id: &mut XcmHash
				) -> Result<bool, $crate::ProcessMessageError> {
					use $crate::{Weight, AggregateMessageOrigin, UmpQueueId, ServiceQueues, EnqueueMessage};
					use $mq as message_queue;
					use $runtime_event as runtime_event;

					Self::execute_with(|| {
						<$mq as EnqueueMessage<AggregateMessageOrigin>>::enqueue_message(
							msg.try_into().expect("Message too long"),
							AggregateMessageOrigin::Ump(UmpQueueId::Para(para.clone()))
						);

						<$system>::reset_events();
						<$mq as ServiceQueues>::service_queues(Weight::MAX);
						let events = <$system>::events();
						let event = events.last().expect("There must be at least one event");

						match &event.event {
							runtime_event::MessageQueue(
								$crate::pallet_message_queue::Event::Processed {origin, ..}) => {
								assert_eq!(origin, &AggregateMessageOrigin::Ump(UmpQueueId::Para(para)));
							},
							event => panic!("Unexpected event: {:#?}", event),
						}
						Ok(true)
					})
				}
			}

			$crate::__impl_test_ext_for_relay_chain!($name, $genesis, $on_init, $api_version);
		)+
	};
}

#[macro_export]
macro_rules! __impl_test_ext_for_relay_chain {
	// entry point: generate ext name
	($name:ident, $genesis:expr, $on_init:expr, $api_version:tt) => {
		$crate::paste::paste! {
			$crate::__impl_test_ext_for_relay_chain!(@impl $name, $genesis, $on_init, [<ParachainHostV $api_version>], [<EXT_ $name:upper>]);
		}
	};
	// impl
	(@impl $name:ident, $genesis:expr, $on_init:expr, $api_version:ident, $ext_name:ident) => {
		thread_local! {
			pub static $ext_name: $crate::RefCell<$crate::sp_io::TestExternalities>
				= $crate::RefCell::new(<$name>::build_new_ext($genesis));
		}

		impl TestExt for $name {
			fn build_new_ext(storage: $crate::Storage) -> $crate::sp_io::TestExternalities {
				let mut ext = sp_io::TestExternalities::new(storage);
				ext.execute_with(|| {
					#[allow(clippy::no_effect)]
					$on_init;
					sp_tracing::try_init_simple();
					<Self as RelayChain>::System::set_block_number(1);
				});
				ext
			}

			fn new_ext() -> $crate::sp_io::TestExternalities {
				<$name>::build_new_ext($genesis)
			}

			fn reset_ext() {
				$ext_name.with(|v| *v.borrow_mut() = <$name>::build_new_ext($genesis));
			}

			fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
				use $crate::{NetworkComponent, Network};
				// Make sure the Network is initialized
				<$name as NetworkComponent>::Network::init();

				let r = $ext_name.with(|v| v.borrow_mut().execute_with(execute));

				// send messages if needed
				$ext_name.with(|v| {
					v.borrow_mut().execute_with(|| {
						use $crate::polkadot_primitives::runtime_api::runtime_decl_for_parachain_host::$api_version;

						//TODO: mark sent count & filter out sent msg
						for para_id in<$name as NetworkComponent>::Network::para_ids() {
							// downward messages
							let downward_messages = <Self as RelayChain>::Runtime::dmq_contents(para_id.into())
								.into_iter()
								.map(|inbound| (inbound.sent_at, inbound.msg));
							if downward_messages.len() == 0 {
								continue;
							}
							<$name>::send_downward_messages(para_id, downward_messages.into_iter());

							// Note: no need to handle horizontal messages, as the
							// simulator directly sends them to dest (not relayed).
						}
					})
				});

				<$name as NetworkComponent>::Network::process_messages();

				r
			}

			fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
				$ext_name.with(|v| {
					v.borrow_mut().execute_with(|| {
						func()
					})
				})
			}
		}
	};
}

#[macro_export]
macro_rules! __impl_relay {
	($network:ident, $relay_chain:ty) => {
		impl $crate::NetworkComponent for $relay_chain {
			type Network = $network;

			fn network_name() -> &'static str {
				stringify!($network)
			}
		}

		impl $relay_chain {
			pub fn child_location_of(id: $crate::ParaId) -> MultiLocation {
				(Ancestor(0), Parachain(id.into())).into()
			}

			pub fn account_id_of(seed: &str) -> $crate::AccountId {
				$crate::get_account_id_from_seed::<sr25519::Public>(seed)
			}

			pub fn account_data_of(account: AccountId) -> $crate::AccountData<Balance> {
				Self::ext_wrapper(|| <Self as RelayChain>::System::account(account).data)
			}

			pub fn sovereign_account_id_of(location: $crate::MultiLocation) -> $crate::AccountId {
				<Self as RelayChain>::SovereignAccountOf::convert_location(&location).unwrap()
			}

			pub fn fund_accounts(accounts: Vec<(AccountId, Balance)>) {
				Self::ext_wrapper(|| {
					for account in accounts {
						let _ = <Self as RelayChain>::Balances::force_set_balance(
							<Self as RelayChain>::RuntimeOrigin::root(),
							account.0.into(),
							account.1.into(),
						);
					}
				});
			}

			pub fn events() -> Vec<<Self as RelayChain>::RuntimeEvent> {
				<Self as RelayChain>::System::events()
					.iter()
					.map(|record| record.event.clone())
					.collect()
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
				runtime = {
					Runtime: $runtime:path,
					RuntimeOrigin: $runtime_origin:path,
					RuntimeCall: $runtime_call:path,
					RuntimeEvent: $runtime_event:path,
					XcmpMessageHandler: $xcmp_message_handler:path,
					DmpMessageHandler: $dmp_message_handler:path,
					LocationToAccountId: $location_to_account:path,
					System: $system:path,
					Balances: $balances_pallet:path,
					ParachainSystem: $parachain_system:path,
					ParachainInfo: $parachain_info:path,
				},
				pallets_extra = {
					$($pallet_name:ident: $pallet_path:path,)*
				}
			}
		),
		+
	) => {
		$(
			pub struct $name;

			impl Parachain for $name {
				type Runtime = $runtime;
				type RuntimeOrigin = $runtime_origin;
				type RuntimeCall = $runtime_call;
				type RuntimeEvent = $runtime_event;
				type XcmpMessageHandler = $xcmp_message_handler;
				type DmpMessageHandler = $dmp_message_handler;
				type LocationToAccountId = $location_to_account;
				type System = $system;
				type Balances = $balances_pallet;
				type ParachainSystem = $parachain_system;
				type ParachainInfo = $parachain_info;
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

			$crate::__impl_xcm_handlers_for_parachain!($name);
			$crate::__impl_test_ext_for_parachain!($name, $genesis, $on_init);
		)+
	};
}

#[macro_export]
macro_rules! __impl_xcm_handlers_for_parachain {
	($name:ident) => {
		impl $crate::XcmpMessageHandler for $name {
			fn handle_xcmp_messages<
				'a,
				I: Iterator<Item = ($crate::ParaId, $crate::RelayBlockNumber, &'a [u8])>,
			>(
				iter: I,
				max_weight: $crate::Weight,
			) -> $crate::Weight {
				use $crate::{TestExt, XcmpMessageHandler};

				$name::execute_with(|| {
					<Self as Parachain>::XcmpMessageHandler::handle_xcmp_messages(iter, max_weight)
				})
			}
		}

		impl $crate::DmpMessageHandler for $name {
			fn handle_dmp_messages(
				iter: impl Iterator<Item = ($crate::RelayBlockNumber, Vec<u8>)>,
				max_weight: $crate::Weight,
			) -> $crate::Weight {
				use $crate::{DmpMessageHandler, TestExt};

				$name::execute_with(|| {
					<Self as Parachain>::DmpMessageHandler::handle_dmp_messages(iter, max_weight)
				})
			}
		}
	};
}

#[macro_export]
macro_rules! __impl_test_ext_for_parachain {
	// entry point: generate ext name
	($name:ident, $genesis:expr, $on_init:expr) => {
		$crate::paste::paste! {
			$crate::__impl_test_ext_for_parachain!(@impl $name, $genesis, $on_init, [<EXT_ $name:upper>]);
		}
	};
	// impl
	(@impl $name:ident, $genesis:expr, $on_init:expr, $ext_name:ident) => {
		thread_local! {
			pub static $ext_name: $crate::RefCell<$crate::sp_io::TestExternalities>
				= $crate::RefCell::new(<$name>::build_new_ext($genesis));
		}

		impl TestExt for $name {
			fn build_new_ext(storage: $crate::Storage) -> $crate::sp_io::TestExternalities {
				let mut ext = sp_io::TestExternalities::new(storage);
				ext.execute_with(|| {
					#[allow(clippy::no_effect)]
					$on_init;
					sp_tracing::try_init_simple();
					<Self as Parachain>::System::set_block_number(1);
				});
				ext
			}

			fn new_ext() -> $crate::sp_io::TestExternalities {
				<$name>::build_new_ext($genesis)
			}

			fn reset_ext() {
				$ext_name.with(|v| *v.borrow_mut() = <$name>::build_new_ext($genesis));
			}

			fn execute_with<R>(execute: impl FnOnce() -> R) -> R {
				use $crate::{Get, Hooks, NetworkComponent, Network, Bridge};

				// Make sure the Network is initialized
				<$name as NetworkComponent>::Network::init();

				let mut relay_block_number = <$name as NetworkComponent>::Network::relay_block_number();
				relay_block_number += 1;
				<$name as NetworkComponent>::Network::set_relay_block_number(relay_block_number);

				let para_id = <$name>::para_id().into();

				$ext_name.with(|v| {
					v.borrow_mut().execute_with(|| {
						// Make sure it has been recorded properly
						let relay_block_number = <$name as NetworkComponent>::Network::relay_block_number();
						let _ = <Self as Parachain>::ParachainSystem::set_validation_data(
							<Self as Parachain>::RuntimeOrigin::none(),
							<$name as NetworkComponent>::Network::hrmp_channel_parachain_inherent_data(para_id, relay_block_number),
						);
					})
				});


				let r = $ext_name.with(|v| v.borrow_mut().execute_with(execute));

				// send messages if needed
				$ext_name.with(|v| {
					v.borrow_mut().execute_with(|| {
						use sp_runtime::traits::Header as HeaderT;

						let block_number = <Self as Parachain>::System::block_number();
						let mock_header = HeaderT::new(
							0,
							Default::default(),
							Default::default(),
							Default::default(),
							Default::default(),
						);

						// get xcmp messages
						<Self as Parachain>::ParachainSystem::on_finalize(block_number);
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
						type NetworkBridge = <<$name as NetworkComponent>::Network as Network>::Bridge;

						let bridge_messages = <NetworkBridge as Bridge>::Handler::get_source_outbound_messages();

						// send bridged messages
						for msg in bridge_messages {
							<$name>::send_bridged_messages(msg);
						}

						// clean messages
						<Self as Parachain>::ParachainSystem::on_initialize(block_number);
					})
				});

				<$name as NetworkComponent>::Network::process_messages();

				r
			}

			fn ext_wrapper<R>(func: impl FnOnce() -> R) -> R {
				$ext_name.with(|v| {
					v.borrow_mut().execute_with(|| {
						func()
					})
				})
			}
		}
	};
}

#[macro_export]
macro_rules! __impl_parachain {
	($network:ident, $parachain:ty) => {
		impl $crate::NetworkComponent for $parachain {
			type Network = $network;

			fn network_name() -> &'static str {
				stringify!($network)
			}
		}

		impl $parachain {
			pub fn para_id() -> $crate::ParaId {
				Self::ext_wrapper(|| <Self as Parachain>::ParachainInfo::get())
			}

			pub fn parent_location() -> $crate::MultiLocation {
				(Parent).into()
			}

			pub fn sibling_location_of(para_id: $crate::ParaId) -> $crate::MultiLocation {
				(Parent, X1(Parachain(para_id.into()))).into()
			}

			pub fn account_id_of(seed: &str) -> $crate::AccountId {
				$crate::get_account_id_from_seed::<sr25519::Public>(seed)
			}

			pub fn account_data_of(account: AccountId) -> $crate::AccountData<Balance> {
				Self::ext_wrapper(|| <Self as Parachain>::System::account(account).data)
			}

			pub fn sovereign_account_id_of(location: $crate::MultiLocation) -> $crate::AccountId {
				<Self as Parachain>::LocationToAccountId::convert_location(&location).unwrap()
			}

			pub fn fund_accounts(accounts: Vec<(AccountId, Balance)>) {
				Self::ext_wrapper(|| {
					for account in accounts {
						let _ = <Self as Parachain>::Balances::force_set_balance(
							<Self as Parachain>::RuntimeOrigin::root(),
							account.0.into(),
							account.1.into(),
						);
					}
				});
			}

			pub fn events() -> Vec<<Self as Parachain>::RuntimeEvent> {
				<Self as Parachain>::System::events()
					.iter()
					.map(|record| record.event.clone())
					.collect()
			}

			fn prepare_for_xcmp() {
				use $crate::{Network, NetworkComponent};
				let para_id = Self::para_id();

				<Self as TestExt>::ext_wrapper(|| {
					use $crate::{Get, Hooks};

					let block_number = <Self as Parachain>::System::block_number();

					let _ = <Self as Parachain>::ParachainSystem::set_validation_data(
						<Self as Parachain>::RuntimeOrigin::none(),
						<Self as NetworkComponent>::Network::hrmp_channel_parachain_inherent_data(
							para_id.into(),
							1,
						),
					);
					// set `AnnouncedHrmpMessagesPerCandidate`
					<Self as Parachain>::ParachainSystem::on_initialize(block_number);
				});
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
	) => {
		$(
			pub struct $name;

			impl $name {
				pub fn reset() {
					use $crate::{TestExt, VecDeque};

					$crate::INITIALIZED.with(|b| b.borrow_mut().remove(stringify!($name)));
					$crate::DOWNWARD_MESSAGES.with(|b| b.borrow_mut().remove(stringify!($name)));
					$crate::DMP_DONE.with(|b| b.borrow_mut().remove(stringify!($name)));
					$crate::UPWARD_MESSAGES.with(|b| b.borrow_mut().remove(stringify!($name)));
					$crate::HORIZONTAL_MESSAGES.with(|b| b.borrow_mut().remove(stringify!($name)));
					$crate::BRIDGED_MESSAGES.with(|b| b.borrow_mut().remove(stringify!($name)));
					$crate::RELAY_BLOCK_NUMBER.with(|b| b.borrow_mut().remove(stringify!($name)));

					<$relay_chain>::reset_ext();
					$( <$parachain>::reset_ext(); )*
				}
			}

			impl $crate::Network for $name {
				type Bridge = $bridge;

				fn init() {
					// If Network has not been itialized yet, it gets initialized
					if $crate::INITIALIZED.with(|b| b.borrow_mut().get(stringify!($name)).is_none()) {
						$crate::INITIALIZED.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), true));
						$crate::DOWNWARD_MESSAGES.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), $crate::VecDeque::new()));
						$crate::DMP_DONE.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), $crate::VecDeque::new()));
						$crate::UPWARD_MESSAGES.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), $crate::VecDeque::new()));
						$crate::HORIZONTAL_MESSAGES.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), $crate::VecDeque::new()));
						$crate::BRIDGED_MESSAGES.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), $crate::VecDeque::new()));
						$crate::RELAY_BLOCK_NUMBER.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), 1));
						$crate::PARA_IDS.with(|b| b.borrow_mut().insert(stringify!($name).to_string(), Self::para_ids()));

						$( <$parachain>::prepare_for_xcmp(); )*
					}
				}

				fn para_ids() -> Vec<u32> {
					vec![$(
						<$parachain>::para_id().into(),
					)*]
				}

				fn relay_block_number() -> u32 {
					$crate::RELAY_BLOCK_NUMBER.with(|v| *v.clone().borrow().get(stringify!($name)).unwrap())
				}

				fn set_relay_block_number(block_number: u32) {
					$crate::RELAY_BLOCK_NUMBER.with(|v| v.borrow_mut().insert(stringify!($name).to_string(), block_number));
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
					$crate::DOWNWARD_MESSAGES.with(|b| !b.borrow_mut().get_mut(stringify!($name)).unwrap().is_empty())
					|| $crate::HORIZONTAL_MESSAGES.with(|b| !b.borrow_mut().get_mut(stringify!($name)).unwrap().is_empty())
					|| $crate::UPWARD_MESSAGES.with(|b| !b.borrow_mut().get_mut(stringify!($name)).unwrap().is_empty())
					|| $crate::BRIDGED_MESSAGES.with(|b| !b.borrow_mut().get_mut(stringify!($name)).unwrap().is_empty())
				}

				fn process_downward_messages() {
					use $crate::{DmpMessageHandler, Bounded};
					use polkadot_parachain::primitives::RelayChainBlockNumber;

					while let Some((to_para_id, messages))
						= $crate::DOWNWARD_MESSAGES.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().pop_front()) {
						$(
							let para_id: u32 = <$parachain>::para_id().into();

							if $crate::PARA_IDS.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().contains(&to_para_id)) && para_id == to_para_id {
								let mut msg_dedup: Vec<(RelayChainBlockNumber, Vec<u8>)> = Vec::new();
								for m in &messages {
									msg_dedup.push((m.0, m.1.clone()));
								}
								msg_dedup.dedup();

								let msgs = msg_dedup.clone().into_iter().filter(|m| {
									!$crate::DMP_DONE.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap_or(&mut $crate::VecDeque::new()).contains(&(to_para_id, m.0, m.1.clone())))
								}).collect::<Vec<(RelayChainBlockNumber, Vec<u8>)>>();
								if msgs.len() != 0 {
									<$parachain>::handle_dmp_messages(msgs.clone().into_iter(), $crate::Weight::max_value());
									$crate::log::debug!(target: concat!("dmp::", stringify!($name)) , "DMP messages processed {:?} to para_id {:?}", msgs.clone(), &to_para_id);
									for m in msgs {
										$crate::DMP_DONE.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().push_back((to_para_id, m.0, m.1)));
									}
								}
							}
						)*
					}
				}

				fn process_horizontal_messages() {
					use $crate::{XcmpMessageHandler, Bounded};

					while let Some((to_para_id, messages))
						= $crate::HORIZONTAL_MESSAGES.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().pop_front()) {
						let iter = messages.iter().map(|(p, b, m)| (*p, *b, &m[..])).collect::<Vec<_>>().into_iter();
						$(
							let para_id: u32 = <$parachain>::para_id().into();

							if $crate::PARA_IDS.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().contains(&to_para_id)) && para_id == to_para_id {
								<$parachain>::handle_xcmp_messages(iter.clone(), $crate::Weight::max_value());
								$crate::log::debug!(target: concat!("hrmp::", stringify!($name)) , "HRMP messages processed {:?} to para_id {:?}", &messages, &to_para_id);
							}
						)*
					}
				}

				fn process_upward_messages() {
					use $crate::{Bounded, ProcessMessage, WeightMeter};
					use sp_core::Encode;
					while let Some((from_para_id, msg)) = $crate::UPWARD_MESSAGES.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().pop_front()) {
						let mut weight_meter = WeightMeter::max_limit();
						let _ =  <$relay_chain>::process_message(
							&msg[..],
							from_para_id.into(),
							&mut weight_meter,
							&mut msg.using_encoded(sp_core::blake2_256),
						);
						$crate::log::debug!(target: concat!("ump::", stringify!($name)) , "Upward message processed {:?} from para_id {:?}", &msg, &from_para_id);
					}
				}

				fn process_bridged_messages() {
					use $crate::Bridge;
					// Make sure both, including the target `Network` are initialized
					<Self::Bridge as Bridge>::init();

					while let Some(msg) = $crate::BRIDGED_MESSAGES.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().pop_front()) {
						let dispatch_result = <<Self::Bridge as $crate::Bridge>::Target as TestExt>::ext_wrapper(|| {
							<<Self::Bridge as Bridge>::Handler as BridgeMessageHandler>::dispatch_target_inbound_message(msg.clone())
						});

						match dispatch_result {
							Err(e) => panic!("Error {:?} processing bridged message: {:?}", e, msg.clone()),
							Ok(()) => {
								<<Self::Bridge as $crate::Bridge>::Source as TestExt>::ext_wrapper(|| {
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
				) -> $crate::ParachainInherentData {
					use $crate::cumulus_primitives_core::{relay_chain::HrmpChannelId, AbridgedHrmpChannel};

					let mut sproof = $crate::RelayStateSproofBuilder::default();
					sproof.para_id = para_id.into();

					// egress channel
					let e_index = sproof.hrmp_egress_channel_index.get_or_insert_with(Vec::new);
					for recipient_para_id in $crate::PARA_IDS.with(|b| b.borrow_mut().get_mut(stringify!($name)).unwrap().clone()) {
						let recipient_para_id = $crate::ParaId::from(recipient_para_id);
						if let Err(idx) = e_index.binary_search(&recipient_para_id) {
							e_index.insert(idx, recipient_para_id);
						}

						sproof
							.hrmp_channels
							.entry(HrmpChannelId {
								sender: sproof.para_id,
								recipient: recipient_para_id,
							})
							.or_insert_with(|| AbridgedHrmpChannel {
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

			$crate::__impl_relay!($name, $relay_chain);

			$(
				$crate::__impl_parachain!($name, $parachain);
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
					// Make sure source and target `Network` has been initialized
					<$source as NetworkComponent>::Network::init();
					<$target as NetworkComponent>::Network::init();
				}
			}
		)+
	};
}

#[macro_export]
macro_rules! assert_expected_events {
	( $chain:ident, vec![$( $event_pat:pat => { $($attr:ident : $condition:expr, )* }, )*] ) => {
		let mut message: Vec<String> = Vec::new();
		$(
			let mut meet_conditions = true;
			let mut event_message: Vec<String> = Vec::new();

			let event_received = <$chain>::events().iter().any(|event| {
				$crate::log::debug!(target: concat!("events::", stringify!($chain)), "{:?}", event);

				match event {
					$event_pat => {
						$(
							if !$condition {
								event_message.push(format!(" - The attribute {:?} = {:?} did not met the condition {:?}\n", stringify!($attr), $attr, stringify!($condition)));
								meet_conditions &= $condition
							}
						)*
						true
					},
					_ => false
				}
			});

			if event_received && !meet_conditions  {
				message.push(format!("\n\nEvent \x1b[31m{}\x1b[0m was received but some of its attributes did not meet the conditions:\n{}", stringify!($event_pat), event_message.concat()));
			} else if !event_received {
				message.push(format!("\n\nEvent \x1b[31m{}\x1b[0m was never received", stringify!($event_pat)));
			}
		)*
		if !message.is_empty() {
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
			parameter_types! {
				$(
					pub [<$chain Sender>]: $crate::AccountId = <$chain>::account_id_of($sender);
					pub [<$chain Receiver>]: $crate::AccountId = <$chain>::account_id_of($receiver);
				)+
			}
		}
	};
}

pub mod helpers {
	use super::Weight;

	pub fn within_threshold(threshold: u64, expected_value: u64, current_value: u64) -> bool {
		let margin = (current_value * threshold) / 100;
		let lower_limit = expected_value - margin;
		let upper_limit = expected_value + margin;

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
}
