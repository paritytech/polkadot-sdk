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

use crate::*;
use frame_benchmarking::v2::*;
use frame_support::traits::{
	schedule::DispatchTime, tokens::IdAmount, Consideration, Currency, Footprint, Polling,
	VoteTally,
};
use frame_system::RawOrigin;
use pallet_asset_rate::AssetKindFactory;
use pallet_bounties::BountyStatus;
use pallet_conviction_voting::{AccountVote, Casting, Delegations, Vote, Voting};
use pallet_nomination_pools::TotalUnbondingPools;
use pallet_proxy::ProxyDefinition;
use pallet_rc_migrator::{
	bounties::{alias::Bounty, RcBountiesMessage},
	claims::{alias::EthereumAddress, RcClaimsMessage},
	conviction_voting::RcConvictionVotingMessage,
	crowdloan::RcCrowdloanMessage,
	indices::RcIndicesIndex,
	preimage::{
		alias::{PreimageFor, RequestStatus as PreimageRequestStatus, MAX_SIZE},
		CHUNK_SIZE,
	},
	proxy::{RcProxy, RcProxyAnnouncement},
	scheduler::RcSchedulerMessage,
	staking::{
		bags_list::alias::Node,
		nom_pools_alias::{SubPools, UnbondPool},
	},
	treasury::{alias::SpendStatus, RcTreasuryMessage},
};
use pallet_referenda::{Deposit, ReferendumInfo, ReferendumStatus, TallyOf, TracksInfo};
use pallet_treasury::PaymentState;
use scheduler::RcScheduledOf;
use sp_runtime::traits::Hash;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

/// Type alias for the conviction voting index constraint
pub type ConvictionVotingIndexOf<T> = <<T as pallet_conviction_voting::Config>::Polls as Polling<
	pallet_conviction_voting::TallyOf<T, ()>,
>>::Index;

#[benchmarks(where
	ConvictionVotingIndexOf<T>: From<u8>,
)]
pub mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_finalize() {
		let block_num = BlockNumberFor::<T>::from(1u32);
		DmpDataMessageCounts::<T>::put((1, 0));

		#[block]
		{
			Pallet::<T>::on_finalize(block_num)
		}
	}

	#[benchmark]
	fn receive_multisigs(n: Linear<1, 255>) {
		let create_multisig = |n: u8| -> RcMultisigOf<T> {
			let creator: AccountId32 = [n; 32].into();
			let deposit =
				<<T as pallet_multisig::Config>::Currency as Currency<_>>::minimum_balance();
			let _ = <<T as pallet_multisig::Config>::Currency>::deposit_creating(
				&creator,
				deposit + deposit,
			);
			let _ = <<T as pallet_multisig::Config>::Currency>::reserve(&creator, deposit).unwrap();

			RcMultisig { creator, deposit, details: Some([2u8; 32].into()) }
		};

		let messages = (0..n).map(|i| create_multisig(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Multisig,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_accounts(n: Linear<1, 255>) {
		let create_account = |n: u8| -> RcAccountFor<T> {
			let who: AccountId32 = [n; 32].into();
			let ed = <pallet_balances::Pallet<T> as Currency<_>>::minimum_balance();
			let _ = <pallet_balances::Pallet<T> as Currency<_>>::deposit_creating(&who, ed);

			let hold_amount = ed;
			let holds = vec![IdAmount { id: T::RcHoldReason::default(), amount: hold_amount }];

			let freeze_amount = 2 * ed;
			let freezes =
				vec![IdAmount { id: T::RcFreezeReason::default(), amount: freeze_amount }];

			let lock_amount = 3 * ed;
			let locks = vec![pallet_balances::BalanceLock::<u128> {
				id: [1u8; 8],
				amount: lock_amount,
				reasons: pallet_balances::Reasons::All,
			}];

			let unnamed_reserve = 4 * ed;

			let free = ed + hold_amount + freeze_amount + lock_amount + unnamed_reserve;
			let reserved = hold_amount + unnamed_reserve;
			let frozen = freeze_amount + lock_amount;

			RcAccount {
				who,
				free,
				reserved,
				frozen,
				holds: holds.try_into().unwrap(),
				freezes: freezes.try_into().unwrap(),
				locks: locks.try_into().unwrap(),
				unnamed_reserve,
				consumers: 1,
				providers: 1,
			}
		};

		let messages = (0..n).map(|i| create_account(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Balances,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_liquid_accounts(n: Linear<1, 255>) {
		let create_liquid_account = |n: u8| -> RcAccountFor<T> {
			let who: AccountId32 = [n; 32].into();
			let ed = <pallet_balances::Pallet<T> as Currency<_>>::minimum_balance();
			let _ = <pallet_balances::Pallet<T> as Currency<_>>::deposit_creating(&who, ed);

			RcAccount {
				who,
				free: ed,
				reserved: 0,
				frozen: 0,
				holds: Default::default(),
				freezes: Default::default(),
				locks: Default::default(),
				unnamed_reserve: 0,
				consumers: 1,
				providers: 1,
			}
		};

		let messages =
			(0..n).map(|i| create_liquid_account(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		receive_accounts(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Balances,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_claims(n: Linear<1, 255>) {
		let create_vesting_msg = |n: u8| -> RcClaimsMessageOf<T> {
			RcClaimsMessage::Vesting {
				who: EthereumAddress([n; 20]),
				schedule: (100u32.into(), 200u32.into(), 300u32.into()),
			}
		};

		let messages =
			(0..n).map(|i| create_vesting_msg(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed { pallet: PalletEventName::Claims, count_good: n, count_bad: 0 }
				.into(),
		);
	}

	#[benchmark]
	fn receive_proxy_proxies(n: Linear<1, 255>) {
		let create_proxy = |n: u8| -> RcProxyOf<T, T::RcProxyType> {
			let proxy_def = ProxyDefinition {
				proxy_type: T::RcProxyType::default(),
				delegate: [n; 32].into(),
				delay: 100u32.into(),
			};
			let proxies = vec![proxy_def; T::MaxProxies::get() as usize];

			RcProxy { delegator: [n; 32].into(), deposit: 200u32.into(), proxies }
		};

		let messages = (0..n).map(|i| create_proxy(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ProxyProxies,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_proxy_announcements(n: Linear<1, 255>) {
		let create_proxy_announcement = |n: u8| -> RcProxyAnnouncementOf<T> {
			let creator: AccountId32 = [n; 32].into();
			let deposit = <<T as pallet_proxy::Config>::Currency as Currency<_>>::minimum_balance();
			let _ = <<T as pallet_proxy::Config>::Currency>::deposit_creating(
				&creator,
				deposit + deposit,
			);
			let _ = <T as pallet_proxy::Config>::Currency::reserve(&creator, deposit).unwrap();
			RcProxyAnnouncement { depositor: creator, deposit }
		};

		let messages = (0..n)
			.map(|i| create_proxy_announcement(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ProxyAnnouncements,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_vesting_schedules(n: Linear<1, 255>) {
		let create_vesting_schedule = |n: u8| -> RcVestingSchedule<T> {
			let max_schedule = pallet_vesting::MaxVestingSchedulesGet::<T>::get();
			let schedule = pallet_vesting::VestingInfo::new(n.into(), n.into(), n.into());
			RcVestingSchedule {
				who: [n; 32].into(),
				schedules: vec![schedule; max_schedule as usize].try_into().unwrap(),
			}
		};

		let messages = (0..n)
			.map(|i| create_vesting_schedule(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed { pallet: PalletEventName::Vesting, count_good: n, count_bad: 0 }
				.into(),
		);
	}

	#[benchmark]
	fn receive_nom_pools_messages(n: Linear<1, 255>) {
		let create_nom_sub_pool = |n: u8| -> RcNomPoolsMessage<T> {
			let mut with_era = BoundedBTreeMap::<_, _, _>::new();
			for i in 0..TotalUnbondingPools::<T>::get() {
				let key = i.into();
				with_era
					.try_insert(key, UnbondPool { points: n.into(), balance: n.into() })
					.unwrap();
			}

			RcNomPoolsMessage::SubPoolsStorage {
				sub_pools: (
					n.into(),
					SubPools {
						no_era: UnbondPool { points: n.into(), balance: n.into() },
						with_era,
					},
				),
			}
		};

		let messages =
			(0..n).map(|i| create_nom_sub_pool(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::NomPools,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_fast_unstake_messages(n: Linear<1, 255>) {
		let create_fast_unstake = |n: u8| -> RcFastUnstakeMessage<T> {
			RcFastUnstakeMessage::Queue { member: ([n; 32].into(), n.into()) }
		};

		let messages =
			(0..n).map(|i| create_fast_unstake(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::FastUnstake,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_referenda_values() {
		let referendum_count = 50;
		let mut deciding_count = vec![];
		let mut track_queue = vec![];

		let tracks = <T as pallet_referenda::Config>::Tracks::tracks();
		for (i, (id, _)) in tracks.iter().enumerate() {
			deciding_count.push((id.clone(), (i as u32).into()));

			track_queue.push((
				id.clone(),
				vec![
					(i as u32, (i as u32).into());
					<T as pallet_referenda::Config>::MaxQueued::get() as usize
				],
			));
		}

		#[extrinsic_call]
		_(RawOrigin::Root, referendum_count, deciding_count, track_queue);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ReferendaValues,
				count_good: 1,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark(pov_mode = MaxEncodedLen {
		Preimage::PreimageFor: Measured
	})]
	fn receive_single_active_referendums(m: Linear<1, 4000000>) {
		let create_referendum_info = |m: u32| -> (u32, RcReferendumInfoOf<T, ()>) {
			let id = m;
			let tracks = <T as pallet_referenda::Config>::Tracks::tracks();
			let track_id = tracks.iter().next().unwrap().0;
			let deposit = Deposit { who: [1; 32].into(), amount: m.into() };
			let call: <T as frame_system::Config>::RuntimeCall =
				frame_system::Call::remark { remark: vec![1u8; m as usize] }.into();
			(
				id,
				ReferendumInfo::Ongoing(ReferendumStatus {
					track: track_id,
					origin: Default::default(),
					proposal: <T as pallet_referenda::Config>::Preimages::bound(call).unwrap(),
					enactment: DispatchTime::At(m.into()),
					submitted: m.into(),
					submission_deposit: deposit.clone(),
					decision_deposit: Some(deposit),
					deciding: None,
					tally: TallyOf::<T, ()>::new(track_id),
					in_queue: false,
					alarm: None,
				}),
			)
		};

		let messages = vec![create_referendum_info(m)];

		#[extrinsic_call]
		receive_referendums(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ReferendaReferendums,
				count_good: 1,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_complete_referendums(n: Linear<1, 255>) {
		let mut referendums: Vec<(u32, RcReferendumInfoOf<T, ()>)> = vec![];
		for i in 0..n {
			let i_as_byte: u8 = i.try_into().unwrap();
			let deposit = Deposit { who: [i_as_byte; 32].into(), amount: n.into() };
			referendums.push((
				i,
				ReferendumInfo::Approved(i.into(), Some(deposit.clone()), Some(deposit)),
			));
		}

		#[extrinsic_call]
		receive_referendums(RawOrigin::Root, referendums);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ReferendaReferendums,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark(pov_mode = MaxEncodedLen {
		Preimage::PreimageFor: Measured
	})]
	fn receive_single_scheduler_agenda(m: Linear<1, 4000000>) {
		let m_u8: u8 = (m % 255).try_into().unwrap();
		let call: <T as frame_system::Config>::RuntimeCall =
			frame_system::Call::remark { remark: vec![m_u8; m as usize] }.into();
		let scheduled = RcScheduledOf::<T> {
			maybe_id: Some([m_u8; 32]),
			priority: m_u8,
			call: <T as pallet_referenda::Config>::Preimages::bound(call).unwrap(),
			maybe_periodic: None,
			origin: Default::default(),
		};

		let agendas = vec![(m.into(), vec![Some(scheduled)])];

		#[extrinsic_call]
		receive_scheduler_agenda_messages(RawOrigin::Root, agendas);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::SchedulerAgenda,
				count_good: 1,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_scheduler_lookup(n: Linear<1, 255>) {
		let create_scheduler_lookup = |n: u8| -> RcSchedulerMessageOf<T> {
			RcSchedulerMessage::Lookup(([n; 32], (n.into(), n.into())))
		};

		let lookups = (0..n)
			.map(|i| create_scheduler_lookup(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		receive_scheduler_messages(RawOrigin::Root, lookups);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Scheduler,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_bags_list_messages(n: Linear<1, 255>) {
		let create_bags_list = |n: u8| -> RcBagsListMessage<T> {
			RcBagsListMessage::Node {
				id: [n; 32].into(),
				node: Node {
					id: [n; 32].into(),
					prev: Some([n; 32].into()),
					next: Some([n; 32].into()),
					bag_upper: n.into(),
					score: n.into(),
				},
			}
		};

		let messages = (0..n).map(|i| create_bags_list(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::BagsList,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_indices(n: Linear<1, 255>) {
		let create_indices_index = |n: u8| -> RcIndicesIndexOf<T> {
			return RcIndicesIndex {
				index: n.into(),
				who: [n; 32].into(),
				deposit: n.into(),
				frozen: false,
			}
		};

		let messages =
			(0..n).map(|i| create_indices_index(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed { pallet: PalletEventName::Indices, count_good: n, count_bad: 0 }
				.into(),
		);
	}

	#[benchmark]
	fn receive_conviction_voting_messages(n: Linear<1, 255>) {
		let create_conviction_vote = |n: u8| -> RcConvictionVotingMessageOf<T> {
			let class = <T as pallet_conviction_voting::Config>::Polls::classes()
				.iter()
				.cycle()
				.skip(n as usize)
				.next()
				.unwrap()
				.clone();
			let votes = BoundedVec::<(_, AccountVote<_>), _>::try_from(
				(0..<T as pallet_conviction_voting::Config<()>>::MaxVotes::get())
					.map(|_| {
						(
							n.into(),
							AccountVote::Standard {
								vote: Vote { aye: true, conviction: Default::default() },
								balance: n.into(),
							},
						)
					})
					.collect::<Vec<_>>(),
			)
			.unwrap();
			RcConvictionVotingMessage::VotingFor(
				[n; 32].into(),
				class,
				Voting::Casting(Casting {
					votes,
					delegations: Delegations { votes: n.into(), capital: n.into() },
					prior: Default::default(),
				}),
			)
		};

		let messages = (0..n)
			.map(|i| create_conviction_vote(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ConvictionVoting,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_bounties_messages(n: Linear<1, 255>) {
		let create_bounties = |n: u8| -> RcBountiesMessageOf<T> {
			RcBountiesMessage::Bounties((
				n.into(),
				Bounty {
					proposer: [n; 32].into(),
					value: n.into(),
					fee: n.into(),
					curator_deposit: n.into(),
					bond: n.into(),
					status: BountyStatus::Active { curator: [n; 32].into(), update_due: n.into() },
				},
			))
		};

		let messages = (0..n).map(|i| create_bounties(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Bounties,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_asset_rates(n: Linear<1, 255>) {
		let create_asset_rate =
			|n: u8| -> (<T as pallet_asset_rate::Config>::AssetKind, FixedU128) {
				(
					<T as pallet_asset_rate::Config>::BenchmarkHelper::create_asset_kind(n.into()),
					FixedU128::from_u32(n as u32),
				)
			};

		let messages = (0..n).map(|i| create_asset_rate(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::AssetRates,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_crowdloan_messages(n: Linear<1, 255>) {
		let create_crowdloan = |n: u8| -> RcCrowdloanMessageOf<T> {
			RcCrowdloanMessage::CrowdloanContribution {
				withdraw_block: n.into(),
				contributor: [n.into(); 32].into(),
				para_id: (n as u32).into(),
				amount: n.into(),
				crowdloan_account: [n.into(); 32].into(),
			}
		};

		let messages = (0..n).map(|i| create_crowdloan(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Crowdloan,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_referenda_metadata(n: Linear<1, 255>) {
		let messages = (0..n).map(|i| (i.into(), H256::from([i as u8; 32]))).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::ReferendaMetadata,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_treasury_messages(n: Linear<1, 255>) {
		let create_treasury = |n: u8| -> RcTreasuryMessageOf<T> {
			RcTreasuryMessage::Spends {
				id: n.into(),
				status: SpendStatus {
					asset_kind: VersionedLocatableAsset::V4 {
						location: Location::new(0, [Parachain(1000)]),
						asset_id: Location::new(
							0,
							[PalletInstance(n.into()), GeneralIndex(n.into())],
						)
						.into(),
					},
					amount: n.into(),
					beneficiary: VersionedLocation::V4(Location::new(
						0,
						[xcm::latest::Junction::AccountId32 { network: None, id: [n; 32].into() }],
					)),
					valid_from: n.into(),
					expire_at: n.into(),
					status: PaymentState::Pending,
				},
			}
		};

		let messages = (0..n).map(|i| create_treasury(i.try_into().unwrap())).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::Treasury,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_preimage_legacy_status(n: Linear<1, 255>) {
		let create_preimage_legacy_status = |n: u8| -> RcPreimageLegacyStatusOf<T> {
			let depositor: AccountId32 = [n; 32].into();
			let deposit =
				<<T as pallet_preimage::Config>::Currency as Currency<_>>::minimum_balance();
			let _ = <<T as pallet_preimage::Config>::Currency>::deposit_creating(
				&depositor,
				deposit + deposit,
			);
			let _ =
				<<T as pallet_preimage::Config>::Currency>::reserve(&depositor, deposit).unwrap();

			RcPreimageLegacyStatusOf::<T> { hash: [n; 32].into(), depositor, deposit }
		};

		let messages = (0..n)
			.map(|i| create_preimage_legacy_status(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::PreimageLegacyStatus,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn receive_preimage_request_status(n: Linear<1, 255>) {
		let create_preimage_request_status = |n: u8| -> RcPreimageRequestStatusOf<T> {
			let preimage = vec![n; 512];
			let hash = T::Preimage::note(preimage.into()).unwrap();

			let depositor: AccountId32 = [n; 32].into();
			let old_footprint = Footprint::from_parts(1, 1024);
			<T as pallet_preimage::Config>::Consideration::ensure_successful(
				&depositor,
				old_footprint,
			);
			let consideration =
				<T as pallet_preimage::Config>::Consideration::new(&depositor, old_footprint)
					.unwrap();
			RcPreimageRequestStatusOf::<T> {
				hash,
				request_status: PreimageRequestStatus::Unrequested {
					ticket: (depositor, consideration),
					len: 512, // smaller than old footprint
				},
			}
		};

		let messages = (0..n)
			.map(|i| create_preimage_request_status(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::PreimageRequestStatus,
				count_good: n,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark(pov_mode = MaxEncodedLen {
		Preimage::PreimageFor: Measured
	})]
	fn receive_preimage_chunk(m: Linear<1, 80>) {
		let m_u8: u8 = (m % 255).try_into().unwrap();
		let preimage_len = m * CHUNK_SIZE;
		let preimage = vec![m_u8; preimage_len as usize];
		let hash = <T as frame_system::Config>::Hashing::hash_of(&preimage);
		let preimage_rc_part = preimage[(preimage_len - CHUNK_SIZE) as usize..].to_vec();
		let preimage_ah_part = preimage[..(preimage_len - CHUNK_SIZE) as usize].to_vec();

		if preimage_ah_part.len() > 0 {
			let preimage_ah_part: BoundedVec<u8, ConstU32<MAX_SIZE>> =
				preimage_ah_part.try_into().unwrap();
			PreimageFor::<T>::insert((hash, preimage_len), preimage_ah_part);
		}

		let chunk = RcPreimageChunk {
			preimage_hash: hash,
			preimage_len,
			chunk_byte_offset: preimage_len - CHUNK_SIZE,
			chunk_bytes: preimage_rc_part.try_into().unwrap(),
		};

		#[extrinsic_call]
		receive_preimage_chunks(RawOrigin::Root, vec![chunk]);

		assert_last_event::<T>(
			Event::BatchProcessed {
				pallet: PalletEventName::PreimageChunk,
				count_good: 1,
				count_bad: 0,
			}
			.into(),
		);
	}

	#[benchmark]
	fn force_set_stage() {
		let stage = MigrationStage::DataMigrationOngoing;

		#[extrinsic_call]
		_(RawOrigin::Root, stage.clone());

		assert_last_event::<T>(
			Event::StageTransition { old: MigrationStage::Pending, new: stage }.into(),
		);
	}

	#[benchmark]
	fn start_migration() {
		#[extrinsic_call]
		_(RawOrigin::Root);

		assert_last_event::<T>(
			Event::StageTransition {
				old: MigrationStage::Pending,
				new: MigrationStage::DataMigrationOngoing,
			}
			.into(),
		);
	}

	#[benchmark]
	fn finish_migration() {
		#[extrinsic_call]
		_(RawOrigin::Root, MigrationFinishedData { rc_balance_kept: 100 });

		assert_last_event::<T>(
			Event::StageTransition {
				old: MigrationStage::Pending,
				new: MigrationStage::MigrationDone,
			}
			.into(),
		);
	}

	#[cfg(feature = "std")]
	pub fn test_receive_multisigs<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_multisigs::<T>(n, true /* enable checks */)
	}

	#[cfg(feature = "std")]
	pub fn test_on_finalize<T>()
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_on_finalize::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_proxy_proxies<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_proxy_proxies::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_proxy_announcements<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_proxy_announcements::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_claims<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_claims::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_nom_pools_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_nom_pools_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_vesting_schedules<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_vesting_schedules::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_fast_unstake_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_fast_unstake_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_referenda_values<T>()
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_referenda_values::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_single_active_referendums<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_single_active_referendums::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_complete_referendums<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_complete_referendums::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_accounts<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_accounts::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_liquid_accounts<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_liquid_accounts::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_single_scheduler_agenda<T>(m: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_single_scheduler_agenda::<T>(m, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_scheduler_lookup<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_scheduler_lookup::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_bags_list_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_bags_list_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_indices<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_indices::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_conviction_voting_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_conviction_voting_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_bounties_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_bounties_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_asset_rates<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_asset_rates::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_crowdloan_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_crowdloan_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_referenda_metadata<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_referenda_metadata::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_treasury_messages<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_treasury_messages::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_force_set_stage<T>()
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_force_set_stage::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_start_migration<T>()
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_start_migration::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_finish_migration<T>()
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_finish_migration::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_preimage_legacy_status<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_preimage_legacy_status::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_preimage_request_status<T>(n: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_preimage_request_status::<T>(n, true)
	}

	#[cfg(feature = "std")]
	pub fn test_receive_preimage_chunk<T>(m: u32)
	where
		T: Config,
		ConvictionVotingIndexOf<T>: From<u8>,
	{
		_receive_preimage_chunk::<T>(m, true)
	}
}
