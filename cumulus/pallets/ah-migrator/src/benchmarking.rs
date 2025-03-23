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

//! To run these benchmarks, you will need a modified version of `frame-omni-bencher` that can load
//! snapshots of the relay and asset hub. You can find it on branch `oty-ahm-omni-bencher` of the
//! SDK. Install it with
//! `cargo install --path substrate/utils/frame/omni-bencher --profile production`
//!
//! ```bash
//! frame-omni-bencher v1 benchmark pallet --runtime=target/release/wbuild/asset-hub-polkadot-runtime/asset_hub_polkadot_runtime.wasm --pallet "pallet-ah-migrator" --extrinsic "" --snap=ah-polkadot.snap --rc-snap=polkadot.snap
//! ```

use crate::*;
use frame_benchmarking::v2::*;
use frame_support::{
	traits::{tokens::IdAmount, Currency},
	weights::WeightMeter,
};
use frame_system::RawOrigin;
use pallet_proxy::ProxyDefinition;
use pallet_rc_migrator::proxy::{RcProxy, RcProxyAnnouncement};

/// The minimum amount used for deposits, transfers, etc.
///
/// Equivalent to Polkadot `UNITS`, which is larger than Kusama `UNITS`.
pub const UNITS: u128 = 10_000_000_000;

pub trait ParametersFactory<RcMultisig, RcAccount, RcProxy, RcProxyAnnouncement> {
	fn create_multisig(n: u8) -> RcMultisig;
	fn create_account(n: u8) -> RcAccount;
	fn create_liquid_account(n: u8) -> RcAccount;
	fn create_proxy(n: u8) -> RcProxy;
	fn create_proxy_announcement(n: u8) -> RcProxyAnnouncement;
}

pub struct BenchmarkFactory<T: Config>(PhantomData<T>);
impl<T: Config>
	ParametersFactory<
		RcMultisig<AccountId32, u128>,
		RcAccount<AccountId32, u128, T::RcHoldReason, T::RcFreezeReason>,
		RcProxy<AccountId32, u128, T::RcProxyType, u32>,
		RcProxyAnnouncement<AccountId32, u128>,
	> for BenchmarkFactory<T>
where
	T::AccountId: From<AccountId32>,
	<<T as pallet_multisig::Config>::Currency as Currency<T::AccountId>>::Balance: From<u128>,
	<<T as pallet_proxy::Config>::Currency as Currency<T::AccountId>>::Balance: From<u128>,
{
	fn create_multisig(n: u8) -> RcMultisig<AccountId32, u128> {
		let creator: AccountId32 = [n; 32].into();
		let deposit: u128 = UNITS;
		let _ = <T as pallet_multisig::Config>::Currency::deposit_creating(
			&creator,
			(deposit * 10).into(),
		);
		let _ =
			<T as pallet_multisig::Config>::Currency::reserve(&creator, deposit.into()).unwrap();

		RcMultisig { creator, deposit, details: Some([2u8; 32].into()) }
	}

	fn create_account(n: u8) -> RcAccount<AccountId32, u128, T::RcHoldReason, T::RcFreezeReason> {
		let who: AccountId32 = [n; 32].into();
		let _ = <T as pallet_multisig::Config>::Currency::deposit_creating(
			&who,
			<T as pallet_multisig::Config>::Currency::minimum_balance(),
		);

		let hold_amount = UNITS;
		let holds = vec![IdAmount { id: T::RcHoldReason::default(), amount: hold_amount }];

		let freeze_amount = 2 * UNITS;
		let freezes = vec![IdAmount { id: T::RcFreezeReason::default(), amount: freeze_amount }];

		let lock_amount = 3 * UNITS;
		let locks = vec![pallet_balances::BalanceLock::<u128> {
			id: [1u8; 8],
			amount: lock_amount,
			reasons: pallet_balances::Reasons::All,
		}];

		let unnamed_reserve = 4 * UNITS;

		let free = UNITS + hold_amount + freeze_amount + lock_amount + unnamed_reserve;
		let reserved = hold_amount + unnamed_reserve;
		let frozen = freeze_amount + lock_amount;

		RcAccount {
			who,
			free,
			reserved,
			frozen,
			holds,
			freezes,
			locks,
			unnamed_reserve,
			consumers: 1,
			providers: 1,
		}
	}

	fn create_liquid_account(
		n: u8,
	) -> RcAccount<AccountId32, u128, T::RcHoldReason, T::RcFreezeReason> {
		let who: AccountId32 = [n; 32].into();
		let _ = <T as pallet_multisig::Config>::Currency::deposit_creating(
			&who,
			<T as pallet_multisig::Config>::Currency::minimum_balance(),
		);

		RcAccount {
			who,
			free: UNITS,
			reserved: 0,
			frozen: 0,
			holds: vec![],
			freezes: vec![],
			locks: vec![],
			unnamed_reserve: 0,
			consumers: 1,
			providers: 1,
		}
	}

	fn create_proxy(n: u8) -> RcProxy<AccountId32, u128, T::RcProxyType, u32> {
		let proxy_def = ProxyDefinition {
			proxy_type: T::RcProxyType::default(),
			delegate: [n; 32].into(),
			delay: 100,
		};
		let proxies = vec![proxy_def; T::MaxProxies::get() as usize];

		RcProxy { delegator: [n; 32].into(), deposit: 200, proxies }
	}

	fn create_proxy_announcement(n: u8) -> RcProxyAnnouncement<AccountId32, u128> {
		let creator: AccountId32 = [n; 32].into();
		let deposit: u128 = UNITS;
		let _ = <T as pallet_proxy::Config>::Currency::deposit_creating(
			&creator,
			(deposit * 10).into(),
		);
		let _ =
			<T as pallet_multisig::Config>::Currency::reserve(&creator, deposit.into()).unwrap();

		RcProxyAnnouncement { depositor: creator, deposit }
	}
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks(where T: pallet_balances::Config)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn receive_multisigs_from_snap(n: Linear<1, 255>) {
		verify_snapshot::<T>();
		let (mut messages, _cursor) = relay_snapshot(|| {
			unwrap_no_debug(
				pallet_rc_migrator::multisig::MultisigMigrator::<T, ()>::migrate_out_many(
					None,
					&mut WeightMeter::new(),
					&mut WeightMeter::new(),
				),
			)
		});

		// TODO: unreserve fails since accounts should migrate first to make it successful. we will
		// have a similar issue with the other calls benchmarks.
		// TODO: possible we can truncate to n to have weights based on the number of messages
		// TODO: for calls that have messages with `m` number of variants, we perhaps need to have
		// `m` parameters like `n` parameter in this function. and we filter the returned by
		// `migrate_out_many` `messages` or we pass these parameters to `migrate_out_many`.
		messages.truncate(n as usize);

		#[extrinsic_call]
		receive_multisigs(RawOrigin::Root, messages);

		for event in frame_system::Pallet::<T>::events() {
			let encoded = event.encode();
			log::info!("Event of pallet: {} and event: {}", encoded[0], encoded[1]);
		}
	}

	#[benchmark]
	fn receive_multisigs(n: Linear<1, 255>) {
		let messages = (0..n)
			.map(|i| <<T as Config>::BenchmarkHelper>::create_multisig(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::MultisigBatchProcessed { count_good: n, count_bad: 0 }.into(),
		);
	}

	#[benchmark]
	fn receive_accounts(n: Linear<1, 255>) {
		let messages = (0..n)
			.map(|i| <<T as Config>::BenchmarkHelper>::create_account(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(Event::AccountBatchProcessed { count_good: n, count_bad: 0 }.into());
	}

	#[benchmark]
	fn receive_liquid_accounts(n: Linear<1, 255>) {
		let messages = (0..n)
			.map(|i| <<T as Config>::BenchmarkHelper>::create_liquid_account(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		receive_accounts(RawOrigin::Root, messages);

		assert_last_event::<T>(Event::AccountBatchProcessed { count_good: n, count_bad: 0 }.into());
	}

	// Claims pallet isn't on Westend.
	// #[benchmark]
	// fn receive_claims(n: Linear<1, 255>) {
	// 	let messages = (0..n)
	// 		.map(|i| <<T as Config>::BenchmarkHelper>::create_vesting_msg(i.try_into().unwrap()))
	// 		.collect::<Vec<_>>();

	// 	#[extrinsic_call]
	// 	_(RawOrigin::Root, messages);

	// 	assert_last_event::<T>(Event::ClaimsBatchProcessed { count_good: n, count_bad: 0 }.into());
	// }

	#[benchmark]
	fn receive_proxy_proxies(n: Linear<1, 255>) {
		let messages = (0..n)
			.map(|i| <<T as Config>::BenchmarkHelper>::create_proxy(i.try_into().unwrap()))
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::ProxyProxiesBatchProcessed { count_good: n, count_bad: 0 }.into(),
		);
	}

	#[benchmark]
	fn receive_proxy_announcements(n: Linear<1, 255>) {
		let messages = (0..n)
			.map(|i| {
				<<T as Config>::BenchmarkHelper>::create_proxy_announcement(i.try_into().unwrap())
			})
			.collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Root, messages);

		assert_last_event::<T>(
			Event::ProxyAnnouncementsBatchProcessed { count_good: n, count_bad: 0 }.into(),
		);
	}
}

/// Unwrap something that does not implement Debug. Otherwise we would need to require
/// `pallet_rc_migrator::Config` on out runtime `T`.
pub fn unwrap_no_debug<T, E>(result: Result<T, E>) -> T {
	match result {
		Ok(t) => t,
		Err(_) => panic!("unwrap_no_debug"),
	}
}

/// Check that Oliver's account has some balance on AH and Relay.
///
/// This serves as sanity check that the snapshots were loaded correctly.
fn verify_snapshot<T: Config>() {
	let raw_acc: [u8; 32] =
		hex::decode("6c9e3102dd2c24274667d416e07570ebce6f20ab80ee3fc9917bf4a7568b8fd2")
			.unwrap()
			.try_into()
			.unwrap();
	let acc = AccountId32::from(raw_acc);
	frame_system::Pallet::<T>::reset_events();

	// Sanity check that this is the right account
	let ah_acc = frame_system::Account::<T>::get(&acc);
	if ah_acc.data.free == 0 {
		panic!("No or broken snapshot: account does not have any balance");
	}

	let key = frame_system::Account::<T>::hashed_key_for(&acc);
	let raw_acc = relay_snapshot(|| {
		frame_support::storage::unhashed::get::<
			pallet_balances::AccountData<<T as pallet_balances::Config>::Balance>,
		>(key.as_ref())
	})
	.unwrap();

	if raw_acc.free == 0 {
		panic!("No or broken snapshot: account does not have any balance");
	}
}

/// Read something from the relay chain snapshot instead of the asset hub one.
fn relay_snapshot<R, F: FnOnce() -> R>(f: F) -> R {
	sp_io::storage::get(b"relay_chain_enable");
	let result = f();
	sp_io::storage::get(b"relay_chain_disable");
	result
}
