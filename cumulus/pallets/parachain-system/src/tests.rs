// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.
use super::*;

use codec::Encode;
use cumulus_primitives_core::{
	relay_chain::BlockNumber as RelayBlockNumber, AbridgedHrmpChannel, InboundDownwardMessage,
	InboundHrmpMessage, PersistedValidationData,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::{
	assert_ok,
	dispatch::UnfilteredDispatchable,
	inherent::{InherentData, ProvideInherent},
	parameter_types,
	traits::{OnFinalize, OnInitialize},
	weights::Weight,
};
use frame_system::RawOrigin;
use hex_literal::hex;
use relay_chain::v1::HrmpChannelId;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use sp_version::RuntimeVersion;
use std::cell::RefCell;

use crate as parachain_system;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		ParachainSystem: parachain_system::{Pallet, Call, Config, Storage, Inherent, Event<T>, ValidateUnsigned},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub Version: RuntimeVersion = RuntimeVersion {
		spec_name: sp_version::create_runtime_str!("test"),
		impl_name: sp_version::create_runtime_str!("system-test"),
		authoring_version: 1,
		spec_version: 1,
		impl_version: 1,
		apis: sp_version::create_apis_vec!([]),
		transaction_version: 1,
		state_version: 1,
	};
	pub const ParachainId: ParaId = ParaId::new(200);
	pub const ReservedXcmpWeight: Weight = 0;
	pub const ReservedDmpWeight: Weight = 0;
}
impl frame_system::Config for Test {
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type BlockLength = ();
	type BlockWeights = ();
	type Version = Version;
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = frame_support::traits::Everything;
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ParachainSetCode<Self>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}
impl Config for Test {
	type Event = Event;
	type OnSystemEvent = ();
	type SelfParaId = ParachainId;
	type OutboundXcmpMessageSource = FromThreadLocal;
	type DmpMessageHandler = SaveIntoThreadLocal;
	type ReservedDmpWeight = ReservedDmpWeight;
	type XcmpMessageHandler = SaveIntoThreadLocal;
	type ReservedXcmpWeight = ReservedXcmpWeight;
}

pub struct FromThreadLocal;
pub struct SaveIntoThreadLocal;

std::thread_local! {
	static HANDLED_DMP_MESSAGES: RefCell<Vec<(relay_chain::BlockNumber, Vec<u8>)>> = RefCell::new(Vec::new());
	static HANDLED_XCMP_MESSAGES: RefCell<Vec<(ParaId, relay_chain::BlockNumber, Vec<u8>)>> = RefCell::new(Vec::new());
	static SENT_MESSAGES: RefCell<Vec<(ParaId, Vec<u8>)>> = RefCell::new(Vec::new());
}

fn send_message(dest: ParaId, message: Vec<u8>) {
	SENT_MESSAGES.with(|m| m.borrow_mut().push((dest, message)));
}

impl XcmpMessageSource for FromThreadLocal {
	fn take_outbound_messages(maximum_channels: usize) -> Vec<(ParaId, Vec<u8>)> {
		let mut ids = std::collections::BTreeSet::<ParaId>::new();
		let mut taken = 0;
		let mut result = Vec::new();
		SENT_MESSAGES.with(|ms| {
			ms.borrow_mut().retain(|m| {
				let status = <Pallet<Test> as GetChannelInfo>::get_channel_status(m.0);
				let ready = matches!(status, ChannelStatus::Ready(..));
				if ready && !ids.contains(&m.0) && taken < maximum_channels {
					ids.insert(m.0);
					taken += 1;
					result.push(m.clone());
					false
				} else {
					true
				}
			})
		});
		result
	}
}

impl DmpMessageHandler for SaveIntoThreadLocal {
	fn handle_dmp_messages(
		iter: impl Iterator<Item = (RelayBlockNumber, Vec<u8>)>,
		_max_weight: Weight,
	) -> Weight {
		HANDLED_DMP_MESSAGES.with(|m| {
			for i in iter {
				m.borrow_mut().push(i);
			}
			0
		})
	}
}

impl XcmpMessageHandler for SaveIntoThreadLocal {
	fn handle_xcmp_messages<'a, I: Iterator<Item = (ParaId, RelayBlockNumber, &'a [u8])>>(
		iter: I,
		_max_weight: Weight,
	) -> Weight {
		HANDLED_XCMP_MESSAGES.with(|m| {
			for (sender, sent_at, message) in iter {
				m.borrow_mut().push((sender, sent_at, message.to_vec()));
			}
			0
		})
	}
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
fn new_test_ext() -> sp_io::TestExternalities {
	HANDLED_DMP_MESSAGES.with(|m| m.borrow_mut().clear());
	HANDLED_XCMP_MESSAGES.with(|m| m.borrow_mut().clear());

	frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}

struct ReadRuntimeVersion(Vec<u8>);

impl sp_core::traits::ReadRuntimeVersion for ReadRuntimeVersion {
	fn read_runtime_version(
		&self,
		_wasm_code: &[u8],
		_ext: &mut dyn sp_externalities::Externalities,
	) -> Result<Vec<u8>, String> {
		Ok(self.0.clone())
	}
}

fn wasm_ext() -> sp_io::TestExternalities {
	let version = RuntimeVersion {
		spec_name: "test".into(),
		spec_version: 2,
		impl_version: 1,
		..Default::default()
	};

	let mut ext = new_test_ext();
	ext.register_extension(sp_core::traits::ReadRuntimeVersionExt::new(ReadRuntimeVersion(
		version.encode(),
	)));
	ext
}

struct BlockTest {
	n: <Test as frame_system::Config>::BlockNumber,
	within_block: Box<dyn Fn()>,
	after_block: Option<Box<dyn Fn()>>,
}

/// BlockTests exist to test blocks with some setup: we have to assume that
/// `validate_block` will mutate and check storage in certain predictable
/// ways, for example, and we want to always ensure that tests are executed
/// in the context of some particular block number.
#[derive(Default)]
struct BlockTests {
	tests: Vec<BlockTest>,
	pending_upgrade: Option<RelayChainBlockNumber>,
	ran: bool,
	relay_sproof_builder_hook:
		Option<Box<dyn Fn(&BlockTests, RelayChainBlockNumber, &mut RelayStateSproofBuilder)>>,
	persisted_validation_data_hook:
		Option<Box<dyn Fn(&BlockTests, RelayChainBlockNumber, &mut PersistedValidationData)>>,
	inherent_data_hook:
		Option<Box<dyn Fn(&BlockTests, RelayChainBlockNumber, &mut ParachainInherentData)>>,
}

impl BlockTests {
	fn new() -> BlockTests {
		Default::default()
	}

	fn add_raw(mut self, test: BlockTest) -> Self {
		self.tests.push(test);
		self
	}

	fn add<F>(self, n: <Test as frame_system::Config>::BlockNumber, within_block: F) -> Self
	where
		F: 'static + Fn(),
	{
		self.add_raw(BlockTest { n, within_block: Box::new(within_block), after_block: None })
	}

	fn add_with_post_test<F1, F2>(
		self,
		n: <Test as frame_system::Config>::BlockNumber,
		within_block: F1,
		after_block: F2,
	) -> Self
	where
		F1: 'static + Fn(),
		F2: 'static + Fn(),
	{
		self.add_raw(BlockTest {
			n,
			within_block: Box::new(within_block),
			after_block: Some(Box::new(after_block)),
		})
	}

	fn with_relay_sproof_builder<F>(mut self, f: F) -> Self
	where
		F: 'static + Fn(&BlockTests, RelayChainBlockNumber, &mut RelayStateSproofBuilder),
	{
		self.relay_sproof_builder_hook = Some(Box::new(f));
		self
	}

	#[allow(dead_code)] // might come in handy in future. If now is future and it still hasn't - feel free.
	fn with_validation_data<F>(mut self, f: F) -> Self
	where
		F: 'static + Fn(&BlockTests, RelayChainBlockNumber, &mut PersistedValidationData),
	{
		self.persisted_validation_data_hook = Some(Box::new(f));
		self
	}

	fn with_inherent_data<F>(mut self, f: F) -> Self
	where
		F: 'static + Fn(&BlockTests, RelayChainBlockNumber, &mut ParachainInherentData),
	{
		self.inherent_data_hook = Some(Box::new(f));
		self
	}

	fn run(&mut self) {
		self.ran = true;
		wasm_ext().execute_with(|| {
			for BlockTest { n, within_block, after_block } in self.tests.iter() {
				// clear pending updates, as applicable
				if let Some(upgrade_block) = self.pending_upgrade {
					if n >= &upgrade_block.into() {
						self.pending_upgrade = None;
					}
				}

				// begin initialization
				System::reset_events();
				System::initialize(&n, &Default::default(), &Default::default());

				// now mess with the storage the way validate_block does
				let mut sproof_builder = RelayStateSproofBuilder::default();
				if let Some(ref hook) = self.relay_sproof_builder_hook {
					hook(self, *n as RelayChainBlockNumber, &mut sproof_builder);
				}
				let (relay_parent_storage_root, relay_chain_state) =
					sproof_builder.into_state_root_and_proof();
				let mut vfp = PersistedValidationData {
					relay_parent_number: *n as RelayChainBlockNumber,
					relay_parent_storage_root,
					..Default::default()
				};
				if let Some(ref hook) = self.persisted_validation_data_hook {
					hook(self, *n as RelayChainBlockNumber, &mut vfp);
				}

				<ValidationData<Test>>::put(&vfp);
				NewValidationCode::<Test>::kill();

				// It is insufficient to push the validation function params
				// to storage; they must also be included in the inherent data.
				let inherent_data = {
					let mut inherent_data = InherentData::default();
					let mut system_inherent_data = ParachainInherentData {
						validation_data: vfp.clone(),
						relay_chain_state,
						downward_messages: Default::default(),
						horizontal_messages: Default::default(),
					};
					if let Some(ref hook) = self.inherent_data_hook {
						hook(self, *n as RelayChainBlockNumber, &mut system_inherent_data);
					}
					inherent_data
						.put_data(
							cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER,
							&system_inherent_data,
						)
						.expect("failed to put VFP inherent");
					inherent_data
				};

				// execute the block
				ParachainSystem::on_initialize(*n);
				ParachainSystem::create_inherent(&inherent_data)
					.expect("got an inherent")
					.dispatch_bypass_filter(RawOrigin::None.into())
					.expect("dispatch succeeded");
				within_block();
				ParachainSystem::on_finalize(*n);

				// did block execution set new validation code?
				if NewValidationCode::<Test>::exists() {
					if self.pending_upgrade.is_some() {
						panic!("attempted to set validation code while upgrade was pending");
					}
				}

				// clean up
				System::finalize();
				if let Some(after_block) = after_block {
					after_block();
				}
			}
		});
	}
}

impl Drop for BlockTests {
	fn drop(&mut self) {
		if !self.ran {
			self.run();
		}
	}
}

#[test]
#[should_panic]
fn block_tests_run_on_drop() {
	BlockTests::new().add(123, || panic!("if this test passes, block tests run properly"));
}

#[test]
fn events() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, block_number, builder| {
			if block_number > 123 {
				builder.upgrade_go_ahead = Some(relay_chain::v1::UpgradeGoAhead::GoAhead);
			}
		})
		.add_with_post_test(
			123,
			|| {
				assert_ok!(System::set_code(RawOrigin::Root.into(), Default::default()));
			},
			|| {
				let events = System::events();
				assert_eq!(
					events[0].event,
					Event::ParachainSystem(crate::Event::ValidationFunctionStored.into())
				);
			},
		)
		.add_with_post_test(
			1234,
			|| {},
			|| {
				let events = System::events();
				assert_eq!(
					events[0].event,
					Event::ParachainSystem(crate::Event::ValidationFunctionApplied(1234).into())
				);
			},
		);
}

#[test]
fn non_overlapping() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, _, builder| {
			builder.host_config.validation_upgrade_delay = 1000;
		})
		.add(123, || {
			assert_ok!(System::set_code(RawOrigin::Root.into(), Default::default()));
		})
		.add(234, || {
			assert_eq!(
				System::set_code(RawOrigin::Root.into(), Default::default()),
				Err(Error::<Test>::OverlappingUpgrades.into()),
			)
		});
}

#[test]
fn manipulates_storage() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, block_number, builder| {
			if block_number > 123 {
				builder.upgrade_go_ahead = Some(relay_chain::v1::UpgradeGoAhead::GoAhead);
			}
		})
		.add(123, || {
			assert!(
				!<PendingValidationCode<Test>>::exists(),
				"validation function must not exist yet"
			);
			assert_ok!(System::set_code(RawOrigin::Root.into(), Default::default()));
			assert!(<PendingValidationCode<Test>>::exists(), "validation function must now exist");
		})
		.add_with_post_test(
			1234,
			|| {},
			|| {
				assert!(
					!<PendingValidationCode<Test>>::exists(),
					"validation function must have been unset"
				);
			},
		);
}

#[test]
fn aborted_upgrade() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, block_number, builder| {
			if block_number > 123 {
				builder.upgrade_go_ahead = Some(relay_chain::v1::UpgradeGoAhead::Abort);
			}
		})
		.add(123, || {
			assert_ok!(System::set_code(RawOrigin::Root.into(), Default::default()));
		})
		.add_with_post_test(
			1234,
			|| {},
			|| {
				assert!(
					!<PendingValidationCode<Test>>::exists(),
					"validation function must have been unset"
				);
				let events = System::events();
				assert_eq!(
					events[0].event,
					Event::ParachainSystem(crate::Event::ValidationFunctionDiscarded.into())
				);
			},
		);
}

#[test]
fn checks_size() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, _, builder| {
			builder.host_config.max_code_size = 8;
		})
		.add(123, || {
			assert_eq!(
				System::set_code(RawOrigin::Root.into(), vec![0; 64]),
				Err(Error::<Test>::TooBig.into()),
			);
		});
}

#[test]
fn send_upward_message_num_per_candidate() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, _, sproof| {
			sproof.host_config.max_upward_message_num_per_candidate = 1;
			sproof.relay_dispatch_queue_size = None;
		})
		.add_with_post_test(
			1,
			|| {
				ParachainSystem::send_upward_message(b"Mr F was here".to_vec()).unwrap();
				ParachainSystem::send_upward_message(b"message 2".to_vec()).unwrap();
			},
			|| {
				let v = UpwardMessages::<Test>::get();
				assert_eq!(v, vec![b"Mr F was here".to_vec()]);
			},
		)
		.add_with_post_test(
			2,
			|| { /* do nothing within block */ },
			|| {
				let v = UpwardMessages::<Test>::get();
				assert_eq!(v, vec![b"message 2".to_vec()]);
			},
		);
}

#[test]
fn send_upward_message_relay_bottleneck() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| {
			sproof.host_config.max_upward_message_num_per_candidate = 2;
			sproof.host_config.max_upward_queue_count = 5;

			match relay_block_num {
				1 => sproof.relay_dispatch_queue_size = Some((5, 0)),
				2 => sproof.relay_dispatch_queue_size = Some((4, 0)),
				_ => unreachable!(),
			}
		})
		.add_with_post_test(
			1,
			|| {
				ParachainSystem::send_upward_message(vec![0u8; 8]).unwrap();
			},
			|| {
				// The message won't be sent because there is already one message in queue.
				let v = UpwardMessages::<Test>::get();
				assert!(v.is_empty());
			},
		)
		.add_with_post_test(
			2,
			|| { /* do nothing within block */ },
			|| {
				let v = UpwardMessages::<Test>::get();
				assert_eq!(v, vec![vec![0u8; 8]]);
			},
		);
}

#[test]
fn send_hrmp_message_buffer_channel_close() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| {
			//
			// Base case setup
			//
			sproof.para_id = ParaId::from(200);
			sproof.hrmp_egress_channel_index = Some(vec![ParaId::from(300), ParaId::from(400)]);
			sproof.hrmp_channels.insert(
				HrmpChannelId { sender: ParaId::from(200), recipient: ParaId::from(300) },
				AbridgedHrmpChannel {
					max_capacity: 1,
					msg_count: 1, // <- 1/1 means the channel is full
					max_total_size: 1024,
					max_message_size: 8,
					total_size: 0,
					mqc_head: Default::default(),
				},
			);
			sproof.hrmp_channels.insert(
				HrmpChannelId { sender: ParaId::from(200), recipient: ParaId::from(400) },
				AbridgedHrmpChannel {
					max_capacity: 1,
					msg_count: 1,
					max_total_size: 1024,
					max_message_size: 8,
					total_size: 0,
					mqc_head: Default::default(),
				},
			);

			//
			// Adjustment according to block
			//
			match relay_block_num {
				1 => {},
				2 => {},
				3 => {
					// The channel 200->400 ceases to exist at the relay chain block 3
					sproof
						.hrmp_egress_channel_index
						.as_mut()
						.unwrap()
						.retain(|n| n != &ParaId::from(400));
					sproof.hrmp_channels.remove(&HrmpChannelId {
						sender: ParaId::from(200),
						recipient: ParaId::from(400),
					});

					// We also free up space for a message in the 200->300 channel.
					sproof
						.hrmp_channels
						.get_mut(&HrmpChannelId {
							sender: ParaId::from(200),
							recipient: ParaId::from(300),
						})
						.unwrap()
						.msg_count = 0;
				},
				_ => unreachable!(),
			}
		})
		.add_with_post_test(
			1,
			|| {
				send_message(ParaId::from(300), b"1".to_vec());
				send_message(ParaId::from(400), b"2".to_vec());
			},
			|| {},
		)
		.add_with_post_test(
			2,
			|| {},
			|| {
				// both channels are at capacity so we do not expect any messages.
				let v = HrmpOutboundMessages::<Test>::get();
				assert!(v.is_empty());
			},
		)
		.add_with_post_test(
			3,
			|| {},
			|| {
				let v = HrmpOutboundMessages::<Test>::get();
				assert_eq!(
					v,
					vec![OutboundHrmpMessage { recipient: ParaId::from(300), data: b"1".to_vec() }]
				);
			},
		);
}

#[test]
fn message_queue_chain() {
	assert_eq!(MessageQueueChain::default().head(), H256::zero());

	// Note that the resulting hashes are the same for HRMP and DMP. That's because even though
	// the types are nominally different, they have the same structure and computation of the
	// new head doesn't differ.
	//
	// These cases are taken from https://github.com/paritytech/polkadot/pull/2351
	assert_eq!(
		MessageQueueChain::default()
			.extend_downward(&InboundDownwardMessage { sent_at: 2, msg: vec![1, 2, 3] })
			.extend_downward(&InboundDownwardMessage { sent_at: 3, msg: vec![4, 5, 6] })
			.head(),
		hex!["88dc00db8cc9d22aa62b87807705831f164387dfa49f80a8600ed1cbe1704b6b"].into(),
	);
	assert_eq!(
		MessageQueueChain::default()
			.extend_hrmp(&InboundHrmpMessage { sent_at: 2, data: vec![1, 2, 3] })
			.extend_hrmp(&InboundHrmpMessage { sent_at: 3, data: vec![4, 5, 6] })
			.head(),
		hex!["88dc00db8cc9d22aa62b87807705831f164387dfa49f80a8600ed1cbe1704b6b"].into(),
	);
}

#[test]
fn receive_dmp() {
	lazy_static::lazy_static! {
		static ref MSG: InboundDownwardMessage = InboundDownwardMessage {
			sent_at: 1,
			msg: b"down".to_vec(),
		};
	}

	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| match relay_block_num {
			1 => {
				sproof.dmq_mqc_head =
					Some(MessageQueueChain::default().extend_downward(&MSG).head());
			},
			_ => unreachable!(),
		})
		.with_inherent_data(|_, relay_block_num, data| match relay_block_num {
			1 => {
				data.downward_messages.push(MSG.clone());
			},
			_ => unreachable!(),
		})
		.add(1, || {
			HANDLED_DMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(&*m, &[(MSG.sent_at, MSG.msg.clone())]);
				m.clear();
			});
		});
}

#[test]
fn receive_dmp_after_pause() {
	lazy_static::lazy_static! {
		static ref MSG_1: InboundDownwardMessage = InboundDownwardMessage {
			sent_at: 1,
			msg: b"down1".to_vec(),
		};
		static ref MSG_2: InboundDownwardMessage = InboundDownwardMessage {
			sent_at: 3,
			msg: b"down2".to_vec(),
		};
	}

	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| match relay_block_num {
			1 => {
				sproof.dmq_mqc_head =
					Some(MessageQueueChain::default().extend_downward(&MSG_1).head());
			},
			2 => {
				// no new messages, mqc stayed the same.
				sproof.dmq_mqc_head =
					Some(MessageQueueChain::default().extend_downward(&MSG_1).head());
			},
			3 => {
				sproof.dmq_mqc_head = Some(
					MessageQueueChain::default()
						.extend_downward(&MSG_1)
						.extend_downward(&MSG_2)
						.head(),
				);
			},
			_ => unreachable!(),
		})
		.with_inherent_data(|_, relay_block_num, data| match relay_block_num {
			1 => {
				data.downward_messages.push(MSG_1.clone());
			},
			2 => {
				// no new messages
			},
			3 => {
				data.downward_messages.push(MSG_2.clone());
			},
			_ => unreachable!(),
		})
		.add(1, || {
			HANDLED_DMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(&*m, &[(MSG_1.sent_at, MSG_1.msg.clone())]);
				m.clear();
			});
		})
		.add(2, || {})
		.add(3, || {
			HANDLED_DMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(&*m, &[(MSG_2.sent_at, MSG_2.msg.clone())]);
				m.clear();
			});
		});
}

#[test]
fn receive_hrmp() {
	lazy_static::lazy_static! {
		static ref MSG_1: InboundHrmpMessage = InboundHrmpMessage {
			sent_at: 1,
			data: b"1".to_vec(),
		};

		static ref MSG_2: InboundHrmpMessage = InboundHrmpMessage {
			sent_at: 1,
			data: b"2".to_vec(),
		};

		static ref MSG_3: InboundHrmpMessage = InboundHrmpMessage {
			sent_at: 2,
			data: b"3".to_vec(),
		};

		static ref MSG_4: InboundHrmpMessage = InboundHrmpMessage {
			sent_at: 2,
			data: b"4".to_vec(),
		};
	}

	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| match relay_block_num {
			1 => {
				// 200 - doesn't exist yet
				// 300 - one new message
				sproof.upsert_inbound_channel(ParaId::from(300)).mqc_head =
					Some(MessageQueueChain::default().extend_hrmp(&MSG_1).head());
			},
			2 => {
				// 200 - now present with one message
				// 300 - two new messages
				sproof.upsert_inbound_channel(ParaId::from(200)).mqc_head =
					Some(MessageQueueChain::default().extend_hrmp(&MSG_4).head());
				sproof.upsert_inbound_channel(ParaId::from(300)).mqc_head = Some(
					MessageQueueChain::default()
						.extend_hrmp(&MSG_1)
						.extend_hrmp(&MSG_2)
						.extend_hrmp(&MSG_3)
						.head(),
				);
			},
			3 => {
				// 200 - no new messages
				// 300 - is gone
				sproof.upsert_inbound_channel(ParaId::from(200)).mqc_head =
					Some(MessageQueueChain::default().extend_hrmp(&MSG_4).head());
			},
			_ => unreachable!(),
		})
		.with_inherent_data(|_, relay_block_num, data| match relay_block_num {
			1 => {
				data.horizontal_messages.insert(ParaId::from(300), vec![MSG_1.clone()]);
			},
			2 => {
				data.horizontal_messages.insert(
					ParaId::from(300),
					vec![
						// can't be sent at the block 1 actually. However, we cheat here
						// because we want to test the case where there are multiple messages
						// but the harness at the moment doesn't support block skipping.
						MSG_2.clone(),
						MSG_3.clone(),
					],
				);
				data.horizontal_messages.insert(ParaId::from(200), vec![MSG_4.clone()]);
			},
			3 => {},
			_ => unreachable!(),
		})
		.add(1, || {
			HANDLED_XCMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(&*m, &[(ParaId::from(300), 1, b"1".to_vec())]);
				m.clear();
			});
		})
		.add(2, || {
			HANDLED_XCMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(
					&*m,
					&[
						(ParaId::from(300), 1, b"2".to_vec()),
						(ParaId::from(200), 2, b"4".to_vec()),
						(ParaId::from(300), 2, b"3".to_vec()),
					]
				);
				m.clear();
			});
		})
		.add(3, || {});
}

#[test]
fn receive_hrmp_empty_channel() {
	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| match relay_block_num {
			1 => {
				// no channels
			},
			2 => {
				// one new channel
				sproof.upsert_inbound_channel(ParaId::from(300)).mqc_head =
					Some(MessageQueueChain::default().head());
			},
			_ => unreachable!(),
		})
		.add(1, || {})
		.add(2, || {});
}

#[test]
fn receive_hrmp_after_pause() {
	lazy_static::lazy_static! {
		static ref MSG_1: InboundHrmpMessage = InboundHrmpMessage {
			sent_at: 1,
			data: b"mikhailinvanovich".to_vec(),
		};

		static ref MSG_2: InboundHrmpMessage = InboundHrmpMessage {
			sent_at: 3,
			data: b"1000000000".to_vec(),
		};
	}

	const ALICE: ParaId = ParaId::new(300);

	BlockTests::new()
		.with_relay_sproof_builder(|_, relay_block_num, sproof| match relay_block_num {
			1 => {
				sproof.upsert_inbound_channel(ALICE).mqc_head =
					Some(MessageQueueChain::default().extend_hrmp(&MSG_1).head());
			},
			2 => {
				// 300 - no new messages, mqc stayed the same.
				sproof.upsert_inbound_channel(ALICE).mqc_head =
					Some(MessageQueueChain::default().extend_hrmp(&MSG_1).head());
			},
			3 => {
				// 300 - new message.
				sproof.upsert_inbound_channel(ALICE).mqc_head = Some(
					MessageQueueChain::default().extend_hrmp(&MSG_1).extend_hrmp(&MSG_2).head(),
				);
			},
			_ => unreachable!(),
		})
		.with_inherent_data(|_, relay_block_num, data| match relay_block_num {
			1 => {
				data.horizontal_messages.insert(ALICE, vec![MSG_1.clone()]);
			},
			2 => {
				// no new messages
			},
			3 => {
				data.horizontal_messages.insert(ALICE, vec![MSG_2.clone()]);
			},
			_ => unreachable!(),
		})
		.add(1, || {
			HANDLED_XCMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(&*m, &[(ALICE, 1, b"mikhailinvanovich".to_vec())]);
				m.clear();
			});
		})
		.add(2, || {})
		.add(3, || {
			HANDLED_XCMP_MESSAGES.with(|m| {
				let mut m = m.borrow_mut();
				assert_eq!(&*m, &[(ALICE, 3, b"1000000000".to_vec())]);
				m.clear();
			});
		});
}
