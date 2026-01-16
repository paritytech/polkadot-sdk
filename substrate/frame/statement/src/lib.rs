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

//! Supporting pallet for the statement store.
//!
//! - [`Pallet`]
//!
//! ## Overview
//!
//! The Statement pallet provides means to create statements for the statement store.
//! Statement validation is performed node-side using direct signature verification with
//! configurable allowance limits.
//!
//! This pallet also contains an offchain worker that turns on-chain statement events into
//! statements. These statements are placed in the store and propagated over the network.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::fungible::Inspect};
use frame_system::pallet_prelude::*;
use sp_statement_store::{Proof, Statement};

#[cfg(test)]
// We do not declare all features used by `construct_runtime`
#[allow(unexpected_cfgs)]
mod mock;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::statement";

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	pub type BalanceOf<T> =
		<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

	#[pallet::config]
	pub trait Config: frame_system::Config
	where
		<Self as frame_system::Config>::AccountId: From<sp_statement_store::AccountId>,
	{
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The currency which is used to calculate account limits.
		type Currency: Inspect<Self::AccountId>;
		/// Min balance for priority statements.
		#[pallet::constant]
		type StatementCost: Get<BalanceOf<Self>>;
		/// Cost of data byte used for priority calculation.
		#[pallet::constant]
		type ByteCost: Get<BalanceOf<Self>>;
		/// Minimum number of statements allowed per account.
		#[pallet::constant]
		type MinAllowedStatements: Get<u32>;
		/// Maximum number of statements allowed per account.
		#[pallet::constant]
		type MaxAllowedStatements: Get<u32>;
		/// Minimum data bytes allowed per account.
		#[pallet::constant]
		type MinAllowedBytes: Get<u32>;
		/// Maximum data bytes allowed per account.
		#[pallet::constant]
		type MaxAllowedBytes: Get<u32>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(core::marker::PhantomData<T>);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config>
	where
		<T as frame_system::Config>::AccountId: From<sp_statement_store::AccountId>,
	{
		/// A new statement is submitted
		NewStatement { account: T::AccountId, statement: Statement },
		/// Statement allowance set for an account
		AllowanceSet { account: T::AccountId, max_count: u32, max_size: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to convert account ID to 32 bytes
		InvalidAccountId,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		<T as frame_system::Config>::AccountId: From<sp_statement_store::AccountId>,
		sp_statement_store::AccountId: From<<T as frame_system::Config>::AccountId>,
		<T as frame_system::Config>::RuntimeEvent: From<pallet::Event<T>>,
		<T as frame_system::Config>::RuntimeEvent: TryInto<pallet::Event<T>>,
		sp_statement_store::BlockHash: From<<T as frame_system::Config>::Hash>,
	{
		fn offchain_worker(now: BlockNumberFor<T>) {
			log::trace!(target: LOG_TARGET, "Collecting statements at #{:?}", now);
			Pallet::<T>::collect_statements();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		<T as frame_system::Config>::AccountId: From<sp_statement_store::AccountId>,
	{
		/// Set statement allowance for a specific account.
		///
		/// This is a root-only call intended for test networks to manually configure
		/// per-account statement allowances.
		///
		/// ## Parameters
		/// - `origin`: Must be root
		/// - `who`: The account to set allowance for
		/// - `max_count`: Maximum number of statements allowed
		/// - `max_size`: Maximum total size of statements in bytes
		///
		/// ## Weight
		/// - 1 storage write to well-known key
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_statement_allowance(
			origin: OriginFor<T>,
			who: T::AccountId,
			max_count: u32,
			max_size: u32,
		) -> DispatchResult {
			use codec::Encode;
			use sp_io;
			use sp_statement_store::{statement_allowance_key, StatementAllowance};

			ensure_root(origin)?;

			let account_bytes: [u8; 32] =
				who.encode().as_slice().try_into().map_err(|_| Error::<T>::InvalidAccountId)?;

			let key = statement_allowance_key(&account_bytes);
			let allowance = StatementAllowance::new(max_count, max_size);
			sp_io::storage::set(&key, &allowance.encode());

			log::debug!(
				target: LOG_TARGET,
				"Set statement allowance for account: max_count={}, max_size={}",
				max_count,
				max_size
			);

			Self::deposit_event(Event::AllowanceSet { account: who, max_count, max_size });

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T>
where
	<T as frame_system::Config>::AccountId: From<sp_statement_store::AccountId>,
	sp_statement_store::AccountId: From<<T as frame_system::Config>::AccountId>,
	<T as frame_system::Config>::RuntimeEvent: From<pallet::Event<T>>,
	<T as frame_system::Config>::RuntimeEvent: TryInto<pallet::Event<T>>,
	sp_statement_store::BlockHash: From<<T as frame_system::Config>::Hash>,
{
	/// Submit a statement event. The statement will be picked up by the offchain worker and
	/// broadcast to the network.
	pub fn submit_statement(account: T::AccountId, statement: Statement) {
		Self::deposit_event(Event::NewStatement { account, statement });
	}

	fn collect_statements() {
		// Find `NewStatement` events and submit them to the store
		for (index, event) in frame_system::Pallet::<T>::read_events_no_consensus().enumerate() {
			if let Ok(Event::<T>::NewStatement { account, mut statement }) = event.event.try_into()
			{
				if statement.proof().is_none() {
					let proof = Proof::OnChain {
						who: account.into(),
						block_hash: frame_system::Pallet::<T>::parent_hash().into(),
						event_index: index as u64,
					};
					statement.set_proof(proof);
				}
				sp_statement_store::runtime_api::statement_store::submit_statement(statement);
			}
		}
	}
}
