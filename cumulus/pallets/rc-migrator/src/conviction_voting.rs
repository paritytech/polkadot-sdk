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

use crate::*;
use frame_support::traits::{Currency, Polling};
use pallet_conviction_voting::{ClassLocksFor, TallyOf, Voting};
use sp_runtime::traits::Zero;

/// Stage of the scheduler pallet migration.
#[derive(Encode, Decode, Clone, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum ConvictionVotingStage<AccountId, Class> {
	VotingFor(Option<(AccountId, Class)>),
	ClassLocksFor(Option<AccountId>),
	Finished,
}

#[derive(Encode, Decode, RuntimeDebug, Clone, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum RcConvictionVotingMessage<AccountId, Class, Voting, Balance> {
	VotingFor(AccountId, Class, Voting),
	ClassLocksFor(AccountId, Vec<(Class, Balance)>),
}

pub type RcConvictionVotingMessageOf<T> = RcConvictionVotingMessage<
	<T as frame_system::Config>::AccountId,
	alias::ClassOf<T>,
	alias::VotingOf<T>,
	alias::BalanceOf<T>,
>;

pub struct ConvictionVotingMigrator<T> {
	_phantom: sp_std::marker::PhantomData<T>,
}

impl<T: Config> PalletMigration for ConvictionVotingMigrator<T> {
	type Key = ConvictionVotingStage<T::AccountId, alias::ClassOf<T>>;
	type Error = Error<T>;

	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut last_key = last_key.unwrap_or(ConvictionVotingStage::VotingFor(None));
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();
		let mut made_progress = false;

		loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if !made_progress {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if T::MaxAhWeight::get().any_lt(T::AhWeightInfo::receive_conviction_voting_messages(
				(messages.len() + 1) as u32,
			)) {
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
				if !made_progress {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if messages.len() > 10_000 {
				log::warn!(target: LOG_TARGET, "Weight allowed very big batch, stopping");
				break;
			}
			made_progress = true;

			last_key = match last_key {
				ConvictionVotingStage::VotingFor(last_voting_key) => {
					let mut iter = match last_voting_key {
						None => alias::VotingFor::<T>::iter(),
						Some((account_id, class)) => alias::VotingFor::<T>::iter_from(
							alias::VotingFor::<T>::hashed_key_for(account_id, class),
						),
					};
					match iter.next() {
						Some((account_id, class, voting)) => {
							alias::VotingFor::<T>::remove(&account_id, &class);
							if Pallet::<T>::is_empty_conviction_vote(&voting) {
								// from the Polkadot 17.01.2025 snapshot 20575 records
								// issue: https://github.com/paritytech/polkadot-sdk/issues/7458
								log::debug!(target: LOG_TARGET,
									"VotingFor {:?} is ignored since it has zero voting capital",
									(&account_id, &class)
								);
							} else {
								messages.push(RcConvictionVotingMessage::VotingFor(
									account_id.clone(),
									class.clone(),
									voting,
								));
							}
							ConvictionVotingStage::VotingFor(Some((account_id, class)))
						},
						None => ConvictionVotingStage::ClassLocksFor(None),
					}
				},
				ConvictionVotingStage::ClassLocksFor(last_key) => {
					let mut iter = if let Some(last_key) = last_key {
						ClassLocksFor::<T>::iter_from_key(last_key)
					} else {
						ClassLocksFor::<T>::iter()
					};
					match iter.next() {
						Some((account_id, balance_per_class)) => {
							ClassLocksFor::<T>::remove(&account_id);
							let mut balance_per_class = balance_per_class.into_inner();
							balance_per_class.retain(|(class, balance)| {
								if balance.is_zero() {
									// from the Polkadot 17.01.2025 snapshot 8522 records
									// issue: https://github.com/paritytech/polkadot-sdk/issues/7458
									log::debug!(target: LOG_TARGET,
										"ClassLocksFor {:?} is ignored since it has a zero balance",
										(&account_id, &class)
									);
									false
								} else {
									true
								}
							});
							if balance_per_class.len() > 0 {
								messages.push(RcConvictionVotingMessage::ClassLocksFor(
									account_id.clone(),
									balance_per_class,
								));
							}
							ConvictionVotingStage::ClassLocksFor(Some(account_id))
						},
						None => ConvictionVotingStage::Finished,
					}
				},
				ConvictionVotingStage::Finished => {
					break;
				},
			};
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveConvictionVotingMessages { messages },
				|len| T::AhWeightInfo::receive_conviction_voting_messages(len),
			)?;
		}

		if last_key == ConvictionVotingStage::Finished {
			Ok(None)
		} else {
			Ok(Some(last_key))
		}
	}
}

impl<T: Config> Pallet<T> {
	fn is_empty_conviction_vote(voting: &alias::VotingOf<T>) -> bool {
		if !voting.locked_balance().is_zero() {
			return false;
		}
		match voting {
			Voting::Casting(casting) if casting.delegations.capital.is_zero() => true,
			Voting::Delegating(delegating) if delegating.delegations.capital.is_zero() => true,
			_ => false,
		}
	}
}

pub mod alias {
	use super::*;
	use core::fmt;

	/// Copy of [`pallet_conviction_voting::BalanceOf`].
	///
	/// Required since original type is private.
	pub type BalanceOf<T, I = ()> =
		<<T as pallet_conviction_voting::Config<I>>::Currency as Currency<
			<T as frame_system::Config>::AccountId,
		>>::Balance;

	/// Copy of [`pallet_conviction_voting::ClassOf`].
	///
	/// Required since original type is private.
	pub type ClassOf<T, I = ()> =
		<<T as pallet_conviction_voting::Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;

	/// Copy of [`pallet_conviction_voting::PollIndexOf`].
	///
	/// Required since original type is private.
	pub type PollIndexOf<T, I = ()> =
		<<T as pallet_conviction_voting::Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;

	/// Wrapper around the `MaxVotes` since the SDK does not derive Clone correctly.
	pub struct MaxVotes<Inner> {
		_phantom: sp_std::marker::PhantomData<Inner>,
	}

	impl<Inner: Get<u32>> Get<u32> for MaxVotes<Inner> {
		fn get() -> u32 {
			Inner::get()
		}
	}

	impl<Inner> Clone for MaxVotes<Inner> {
		fn clone(&self) -> Self {
			Self { _phantom: sp_std::marker::PhantomData }
		}
	}

	impl<Inner: Get<u32>> fmt::Debug for MaxVotes<Inner> {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(f, "MaxVotes({})", Inner::get())
		}
	}

	impl<Inner: Get<u32>> PartialEq for MaxVotes<Inner> {
		fn eq(&self, _other: &Self) -> bool {
			true // other has same type as us
		}
	}

	impl<Inner: Get<u32>> Eq for MaxVotes<Inner> {}

	/// Copy of [`pallet_conviction_voting::VotingOf`].
	///
	/// Required since original type is private.
	pub type VotingOf<T, I = ()> = Voting<
		BalanceOf<T, I>,
		<T as frame_system::Config>::AccountId,
		BlockNumberFor<T>,
		PollIndexOf<T, I>,
		MaxVotes<<T as pallet_conviction_voting::Config<I>>::MaxVotes>,
	>;

	/// Storage alias of [`pallet_conviction_voting::VotingFor`].
	///
	/// Required to replace the stored private type with the public alias.
	#[frame_support::storage_alias(pallet_name)]
	pub type VotingFor<T: pallet_conviction_voting::Config<()>> = StorageDoubleMap<
		pallet_conviction_voting::Pallet<T, ()>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Twox64Concat,
		ClassOf<T, ()>,
		VotingOf<T, ()>,
		ValueQuery,
	>;
}

impl<T: Config> crate::types::RcMigrationCheck for ConvictionVotingMigrator<T> {
	type RcPrePayload = Vec<RcConvictionVotingMessageOf<T>>;

	fn pre_check() -> Self::RcPrePayload {
		let mut messages = Vec::new();

		// Collect VotingFor
		for (account_id, class, voting) in alias::VotingFor::<T>::iter() {
			if !Pallet::<T>::is_empty_conviction_vote(&voting) {
				messages.push(RcConvictionVotingMessage::VotingFor(account_id, class, voting));
			}
		}

		// Collect ClassLocksFor
		for (account_id, balance_per_class) in pallet_conviction_voting::ClassLocksFor::<T>::iter()
		{
			let mut balance_per_class = balance_per_class.into_inner();
			balance_per_class.retain(|(_, balance)| !balance.is_zero());
			if !balance_per_class.is_empty() {
				messages
					.push(RcConvictionVotingMessage::ClassLocksFor(account_id, balance_per_class));
			}
		}

		messages
	}

	fn post_check(_: Self::RcPrePayload) {
		assert!(
			alias::VotingFor::<T>::iter().next().is_none(),
			"VotingFor::VotingFor::rc_post::empty"
		);
		assert!(
			pallet_conviction_voting::ClassLocksFor::<T>::iter().next().is_none(),
			"VotingFor::ClassLocksFor::rc_post::empty"
		);
	}
}
