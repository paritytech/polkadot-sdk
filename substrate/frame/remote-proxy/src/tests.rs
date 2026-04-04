// Copyright (C) Polkadot Fellows.
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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

// Tests for Remote Proxy Pallet

use super::*;
use crate as remote_proxy;
use codec::{Decode, DecodeWithMemTracking};
use cumulus_pallet_parachain_system::OnSystemEvent;
use frame::{prelude::*, runtime::prelude::*, testing_prelude::*, traits::*};
use frame_system::Call as SystemCall;
use pallet_balances::Call as BalancesCall;
use pallet_proxy::{Error as ProxyError, Event as ProxyEvent};
use pallet_utility::Call as UtilityCall;

type Block = MockBlock<Test>;

construct_runtime!(
	pub struct Test {
		System: frame_system,
		Balances: pallet_balances,
		Proxy: pallet_proxy,
		Utility: pallet_utility,
		RemoteProxy: remote_proxy,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type BaseCallFilter = BaseFilter;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	MaxEncodedLen,
	scale_info::TypeInfo,
)]
pub enum ProxyType {
	Any,
	JustTransfer,
	JustUtility,
}
impl Default for ProxyType {
	fn default() -> Self {
		Self::Any
	}
}
impl frame::traits::InstanceFilter<RuntimeCall> for ProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			ProxyType::Any => true,
			ProxyType::JustTransfer => {
				matches!(
					c,
					RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death { .. })
				)
			},
			ProxyType::JustUtility => matches!(c, RuntimeCall::Utility { .. }),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		self == &ProxyType::Any || self == o
	}
}
pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(c: &RuntimeCall) -> bool {
		match *c {
			// Remark is used as a no-op call in the benchmarking
			RuntimeCall::System(SystemCall::remark { .. }) => true,
			RuntimeCall::System(_) => false,
			_ => true,
		}
	}
}
impl pallet_proxy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ConstU64<1>;
	type ProxyDepositFactor = ConstU64<1>;
	type MaxProxies = ConstU32<4>;
	type WeightInfo = ();
	type CallHasher = BlakeTwo256;
	type MaxPending = ConstU32<2>;
	type AnnouncementDepositBase = ConstU64<1>;
	type AnnouncementDepositFactor = ConstU64<1>;
	type BlockNumberProvider = System;
}

pub struct RemoteProxyImpl;

impl crate::RemoteProxyInterface<u64, ProxyType, u64> for RemoteProxyImpl {
	type RemoteAccountId = u64;
	type RemoteProxyType = ProxyType;
	type RemoteBlockNumber = u64;
	type RemoteHash = H256;
	type RemoteHasher = BlakeTwo256;

	fn block_to_storage_root(
		validation_data: &PersistedValidationData,
	) -> Option<(Self::RemoteBlockNumber, <Self::RemoteHasher as Hasher>::Out)> {
		Some((validation_data.relay_parent_number as _, validation_data.relay_parent_storage_root))
	}

	fn local_to_remote_account_id(local: &u64) -> Option<Self::RemoteAccountId> {
		Some(*local)
	}

	fn remote_to_local_proxy_defintion(
		remote: ProxyDefinition<
			Self::RemoteAccountId,
			Self::RemoteProxyType,
			Self::RemoteBlockNumber,
		>,
	) -> Option<ProxyDefinition<u64, ProxyType, u64>> {
		Some(remote)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_remote_proxy_proof(
		caller: &u64,
		proxy: &u64,
	) -> (RemoteProxyProof<Self::RemoteBlockNumber>, u64, H256) {
		use sp_trie::TrieMut;

		let (mut db, mut root) = sp_trie::MemoryDB::<BlakeTwo256>::default_with_root();
		let mut trie =
			sp_trie::TrieDBMutBuilder::<sp_trie::LayoutV1<_>>::new(&mut db, &mut root).build();

		let proxy_definition = vec![ProxyDefinition::<u64, ProxyType, u64> {
			delegate: *caller,
			proxy_type: ProxyType::default(),
			delay: 0,
		}];

		trie.insert(&Self::proxy_definition_storage_key(proxy), &proxy_definition.encode())
			.unwrap();
		drop(trie);

		(
			RemoteProxyProof::RelayChain {
				proof: db.drain().into_values().map(|d| d.0).collect(),
				block: 1,
			},
			1,
			root,
		)
	}
}

impl Config for Test {
	type MaxStorageRootsToKeep = ConstU32<10>;
	type RemoteProxy = RemoteProxyImpl;
	type WeightInfo = ();
}

pub fn new_test_ext() -> TestState {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 10), (3, 10), (4, 10), (5, 3)],
		dev_accounts: None,
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = TestState::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

fn call_transfer(dest: u64, value: u64) -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value })
}

#[test]
fn remote_proxy_works() {
	let mut ext = new_test_ext();

	let anon = ext.execute_with(|| {
		Balances::make_free_balance_be(&1, 11); // An extra one for the ED.
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));
		let anon = Proxy::pure_account(&1, &ProxyType::Any, 0, None);
		System::assert_last_event(
			ProxyEvent::PureCreated {
				pure: anon,
				who: 1,
				proxy_type: ProxyType::Any,
				disambiguation_index: 0,
			}
			.into(),
		);
		anon
	});

	let proof = sp_state_machine::prove_read(
		ext.as_backend(),
		[pallet_proxy::Proxies::<Test>::hashed_key_for(anon)],
	)
	.unwrap();
	let root = *ext.as_backend().root();

	new_test_ext().execute_with(|| {
		let call = Box::new(call_transfer(6, 1));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(3), anon, 5));
		assert_eq!(Balances::free_balance(6), 0);
		assert_err!(
			Proxy::proxy(RuntimeOrigin::signed(1), anon, None, call.clone()),
			ProxyError::<Test>::NotProxy
		);
		assert_eq!(Balances::free_balance(6), 0);

		RemoteProxy::on_validation_data(&PersistedValidationData {
			parent_head: vec![].into(),
			relay_parent_number: 1,
			relay_parent_storage_root: root,
			max_pov_size: 5000000,
		});
		assert_ok!(RemoteProxy::remote_proxy(
			RuntimeOrigin::signed(1),
			anon,
			None,
			call.clone(),
			RemoteProxyProof::RelayChain {
				proof: proof.clone().into_iter_nodes().collect(),
				block: 1
			}
		));

		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_eq!(Balances::free_balance(6), 1);

		assert_err!(
			RemoteProxy::remote_proxy(
				RuntimeOrigin::signed(1),
				anon,
				None,
				call.clone(),
				RemoteProxyProof::RelayChain { proof: proof.into_iter_nodes().collect(), block: 2 }
			),
			Error::<Test>::UnknownProofAnchorBlock
		);

		assert_err!(
			RemoteProxy::remote_proxy(
				RuntimeOrigin::signed(1),
				anon,
				None,
				call,
				RemoteProxyProof::RelayChain { proof: Vec::new(), block: 1 }
			),
			Error::<Test>::InvalidProof
		);
	});
}

#[test]
fn remote_proxy_register_works() {
	let mut ext = new_test_ext();

	let anon = ext.execute_with(|| {
		Balances::make_free_balance_be(&1, 11); // An extra one for the ED.
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));
		let anon = Proxy::pure_account(&1, &ProxyType::Any, 0, None);
		System::assert_last_event(
			ProxyEvent::PureCreated {
				pure: anon,
				who: 1,
				proxy_type: ProxyType::Any,
				disambiguation_index: 0,
			}
			.into(),
		);
		anon
	});

	let proof = sp_state_machine::prove_read(
		ext.as_backend(),
		[pallet_proxy::Proxies::<Test>::hashed_key_for(anon)],
	)
	.unwrap();
	let root = *ext.as_backend().root();

	new_test_ext().execute_with(|| {
		let call = Box::new(call_transfer(6, 1));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(3), anon, 5));
		assert_eq!(Balances::free_balance(6), 0);
		assert_err!(
			Proxy::proxy(RuntimeOrigin::signed(1), anon, None, call.clone()),
			ProxyError::<Test>::NotProxy
		);
		assert_eq!(Balances::free_balance(6), 0);

		RemoteProxy::on_validation_data(&PersistedValidationData {
			parent_head: vec![].into(),
			relay_parent_number: 1,
			relay_parent_storage_root: root,
			max_pov_size: 5000000,
		});
		assert_ok!(RuntimeCall::from(UtilityCall::batch {
			calls: vec![
				crate::Call::register_remote_proxy_proof {
					proof: RemoteProxyProof::RelayChain {
						proof: proof.clone().into_iter_nodes().collect(),
						block: 1
					}
				}
				.into(),
				crate::Call::remote_proxy_with_registered_proof {
					real: anon,
					force_proxy_type: None,
					call: call.clone(),
				}
				.into()
			]
		})
		.dispatch(RuntimeOrigin::signed(1)));

		System::assert_has_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		System::reset_events();
		assert_eq!(Balances::free_balance(6), 1);

		assert_ok!(RuntimeCall::from(UtilityCall::batch {
			calls: vec![
				crate::Call::register_remote_proxy_proof {
					proof: RemoteProxyProof::RelayChain {
						proof: proof.clone().into_iter_nodes().collect(),
						block: 1
					}
				}
				.into(),
				UtilityCall::batch {
					calls: vec![crate::Call::remote_proxy_with_registered_proof {
						real: anon,
						force_proxy_type: None,
						call: call.clone(),
					}
					.into()]
				}
				.into()
			]
		})
		.dispatch(RuntimeOrigin::signed(1)));

		System::assert_has_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_eq!(Balances::free_balance(6), 2);

		assert_err!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					crate::Call::register_remote_proxy_proof {
						proof: RemoteProxyProof::RelayChain {
							proof: proof.clone().into_iter_nodes().collect(),
							block: 1
						}
					}
					.into(),
					crate::Call::remote_proxy_with_registered_proof {
						real: anon,
						force_proxy_type: None,
						call: call.clone(),
					}
					.into(),
					crate::Call::remote_proxy_with_registered_proof {
						real: anon,
						force_proxy_type: None,
						call: call.clone(),
					}
					.into()
				]
			})
			.dispatch(RuntimeOrigin::signed(1))
			.map_err(|e| e.error),
			Error::<Test>::ProxyProofNotRegistered
		);
	});
}

#[test]
fn remote_proxy_multiple_register_works() {
	let mut ext = new_test_ext();

	let (anon, anon2) = ext.execute_with(|| {
		Balances::make_free_balance_be(&1, 11); // An extra one for the ED.
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));
		let anon = Proxy::pure_account(&1, &ProxyType::Any, 0, None);
		System::assert_last_event(
			ProxyEvent::PureCreated {
				pure: anon,
				who: 1,
				proxy_type: ProxyType::Any,
				disambiguation_index: 0,
			}
			.into(),
		);

		Balances::make_free_balance_be(&2, 11); // An extra one for the ED.
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 1));
		let anon2 = Proxy::pure_account(&1, &ProxyType::Any, 1, None);
		System::assert_last_event(
			ProxyEvent::PureCreated {
				pure: anon2,
				who: 1,
				proxy_type: ProxyType::Any,
				disambiguation_index: 1,
			}
			.into(),
		);

		(anon, anon2)
	});

	let proof = sp_state_machine::prove_read(
		ext.as_backend(),
		[pallet_proxy::Proxies::<Test>::hashed_key_for(anon)],
	)
	.unwrap();

	let proof2 = sp_state_machine::prove_read(
		ext.as_backend(),
		[pallet_proxy::Proxies::<Test>::hashed_key_for(anon2)],
	)
	.unwrap();

	let root = *ext.as_backend().root();

	new_test_ext().execute_with(|| {
		let call = Box::new(call_transfer(6, 1));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(3), anon, 5));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(4), anon2, 5));
		assert_eq!(Balances::free_balance(6), 0);
		assert_err!(
			Proxy::proxy(RuntimeOrigin::signed(1), anon, None, call.clone()),
			ProxyError::<Test>::NotProxy
		);
		assert_eq!(Balances::free_balance(6), 0);

		RemoteProxy::on_validation_data(&PersistedValidationData {
			parent_head: vec![].into(),
			relay_parent_number: 1,
			relay_parent_storage_root: root,
			max_pov_size: 5000000,
		});
		assert_ok!(RuntimeCall::from(UtilityCall::batch {
			calls: vec![
				crate::Call::register_remote_proxy_proof {
					proof: RemoteProxyProof::RelayChain {
						proof: proof.clone().into_iter_nodes().collect(),
						block: 1
					}
				}
				.into(),
				crate::Call::remote_proxy_with_registered_proof {
					real: anon,
					force_proxy_type: None,
					call: call.clone(),
				}
				.into(),
				crate::Call::register_remote_proxy_proof {
					proof: RemoteProxyProof::RelayChain {
						proof: proof2.clone().into_iter_nodes().collect(),
						block: 1
					}
				}
				.into(),
				crate::Call::remote_proxy_with_registered_proof {
					real: anon2,
					force_proxy_type: None,
					call: call.clone(),
				}
				.into()
			]
		})
		.dispatch(RuntimeOrigin::signed(1)));

		System::assert_has_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		System::reset_events();
		assert_eq!(Balances::free_balance(6), 2);

		assert_ok!(RuntimeCall::from(UtilityCall::batch {
			calls: vec![
				crate::Call::register_remote_proxy_proof {
					proof: RemoteProxyProof::RelayChain {
						proof: proof.clone().into_iter_nodes().collect(),
						block: 1
					}
				}
				.into(),
				crate::Call::register_remote_proxy_proof {
					proof: RemoteProxyProof::RelayChain {
						proof: proof2.clone().into_iter_nodes().collect(),
						block: 1
					}
				}
				.into(),
				crate::Call::remote_proxy_with_registered_proof {
					real: anon2,
					force_proxy_type: None,
					call: call.clone(),
				}
				.into(),
				crate::Call::remote_proxy_with_registered_proof {
					real: anon,
					force_proxy_type: None,
					call: call.clone(),
				}
				.into()
			]
		})
		.dispatch(RuntimeOrigin::signed(1)));

		System::assert_has_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		System::reset_events();
		assert_eq!(Balances::free_balance(6), 4);

		assert_err!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					crate::Call::register_remote_proxy_proof {
						proof: RemoteProxyProof::RelayChain {
							proof: proof.clone().into_iter_nodes().collect(),
							block: 1
						}
					}
					.into(),
					crate::Call::register_remote_proxy_proof {
						proof: RemoteProxyProof::RelayChain {
							proof: proof2.clone().into_iter_nodes().collect(),
							block: 1
						}
					}
					.into(),
					crate::Call::remote_proxy_with_registered_proof {
						real: anon,
						force_proxy_type: None,
						call: call.clone(),
					}
					.into(),
				]
			})
			.dispatch(RuntimeOrigin::signed(1))
			.map_err(|e| e.error),
			Error::<Test>::InvalidProof
		);
	});
}

#[test]
fn clean_up_works_and_old_blocks_are_rejected() {
	new_test_ext().execute_with(|| {
		let root = H256::zero();
		let call = Box::new(call_transfer(6, 1));

		BlockToRoot::<Test>::set(BoundedVec::truncate_from(vec![
			(0, root),
			(10, root),
			(20, root),
			(29, root),
		]));

		RemoteProxy::on_validation_data(&PersistedValidationData {
			parent_head: vec![].into(),
			relay_parent_number: 30,
			relay_parent_storage_root: root,
			max_pov_size: 5000000,
		});
		BlockToRoot::<Test>::get()
			.iter()
			.for_each(|(b, _)| assert!(*b == 29 || *b == 30));
		assert_err!(
			RemoteProxy::remote_proxy(
				RuntimeOrigin::signed(1),
				1000,
				None,
				call.clone(),
				RemoteProxyProof::RelayChain { proof: vec![], block: 5 }
			),
			Error::<Test>::UnknownProofAnchorBlock
		);

		for i in 31u32..=40u32 {
			RemoteProxy::on_validation_data(&PersistedValidationData {
				parent_head: vec![].into(),
				relay_parent_number: dbg!(i),
				relay_parent_storage_root: root,
				max_pov_size: 5000000,
			});
		}

		BlockToRoot::<Test>::get()
			.iter()
			.for_each(|(b, _)| assert!(*b >= 31 && *b <= 40));
	});
}

#[test]
fn on_validation_data_does_not_insert_duplicates() {
	new_test_ext().execute_with(|| {
		let data1 = PersistedValidationData {
			parent_head: Default::default(),
			relay_parent_number: 10,
			relay_parent_storage_root: H256::from_low_u64_be(1),
			max_pov_size: 0,
		};

		RemoteProxy::on_validation_data(&data1);
		let expected_roots = vec![(10u64, H256::from_low_u64_be(1))];
		assert_eq!(BlockToRoot::<Test>::get().into_inner(), expected_roots);

		let data2 = PersistedValidationData {
			parent_head: Default::default(),
			relay_parent_number: 10,
			relay_parent_storage_root: H256::from_low_u64_be(2),
			max_pov_size: 0,
		};
		RemoteProxy::on_validation_data(&data2);

		assert_eq!(
			BlockToRoot::<Test>::get().into_inner(),
			expected_roots,
			"Roots should not change for the same block number"
		);

		let data3 = PersistedValidationData {
			parent_head: Default::default(),
			relay_parent_number: 11,
			relay_parent_storage_root: H256::from_low_u64_be(3),
			max_pov_size: 0,
		};
		RemoteProxy::on_validation_data(&data3);

		let expected_roots_2 =
			vec![(10u64, H256::from_low_u64_be(1)), (11u64, H256::from_low_u64_be(3))];
		assert_eq!(
			BlockToRoot::<Test>::get().into_inner(),
			expected_roots_2,
			"A new block should be added"
		);
	});
}
