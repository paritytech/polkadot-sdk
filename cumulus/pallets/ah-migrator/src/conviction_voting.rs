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
use frame_support::traits::{ClassCountOf, DefensiveTruncateFrom};
use pallet_conviction_voting::TallyOf;
use pallet_rc_migrator::conviction_voting::{
	alias, ConvictionVotingMigrator, RcConvictionVotingMessage, RcConvictionVotingMessageOf,
};

impl<T: Config> Pallet<T> {
	pub fn do_receive_conviction_voting_messages(
		messages: Vec<RcConvictionVotingMessageOf<T>>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Processing {} conviction voting messages", messages.len());
		let count = messages.len() as u32;
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::ConvictionVoting,
			count,
		});

		for message in messages {
			Self::do_receive_conviction_voting_message(message);
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::ConvictionVoting,
			count_good: count,
			count_bad: 0,
		});

		Ok(())
	}

	pub fn do_receive_conviction_voting_message(message: RcConvictionVotingMessageOf<T>) {
		match message {
			RcConvictionVotingMessage::VotingFor(account_id, class, voting) => {
				Self::do_process_voting_for(account_id, class, voting);
			},
			RcConvictionVotingMessage::ClassLocksFor(account_id, balance_per_class) => {
				Self::do_process_class_locks_for(account_id, balance_per_class);
			},
		};
	}

	pub fn do_process_voting_for(
		account_id: T::AccountId,
		class: alias::ClassOf<T>,
		voting: alias::VotingOf<T>,
	) {
		log::debug!(target: LOG_TARGET, "Processing VotingFor record for: {:?}", &account_id);
		alias::VotingFor::<T>::insert(account_id, class, voting);
	}

	pub fn do_process_class_locks_for(
		account_id: T::AccountId,
		balance_per_class: Vec<(alias::ClassOf<T>, alias::BalanceOf<T>)>,
	) {
		log::debug!(target: LOG_TARGET, "Processing ClassLocksFor record for: {:?}", &account_id);
		let balance_per_class =
			BoundedVec::<_, ClassCountOf<T::Polls, TallyOf<T, ()>>>::defensive_truncate_from(
				balance_per_class,
			);
		pallet_conviction_voting::ClassLocksFor::<T>::insert(account_id, balance_per_class);
	}
}

impl<T: Config> crate::types::AhMigrationCheck for ConvictionVotingMigrator<T> {
	type RcPrePayload = Vec<RcConvictionVotingMessageOf<T>>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		assert!(
			alias::VotingFor::<T>::iter().next().is_none(),
			"Assert storage 'ConvictionVoting::VotingFor::ah_pre::empty'"
		);
		assert!(
			pallet_conviction_voting::ClassLocksFor::<T>::iter().next().is_none(),
			"Assert storage 'ConvictionVoting::ClassLocksFor::ah_pre::empty'"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		assert!(!rc_pre_payload.is_empty(), "RC pre-payload should not be empty during post_check");
		let mut ah_messages = Vec::new();

		for (account_id, class, voting) in alias::VotingFor::<T>::iter() {
			ah_messages.push(RcConvictionVotingMessage::VotingFor(account_id, class, voting));
		}

		for (account_id, balance_per_class) in pallet_conviction_voting::ClassLocksFor::<T>::iter()
		{
			let balance_per_class: Vec<_> = balance_per_class.into_iter().collect();
			ah_messages
				.push(RcConvictionVotingMessage::ClassLocksFor(account_id, balance_per_class));
		}

		// Assert storage "ConvictionVoting::VotingFor::ah_post::length"
		// Assert storage "ConvictionVoting::ClassLocksFor::ah_post::length"
		assert_eq!(
            rc_pre_payload.len(), ah_messages.len(),
            "Conviction voting length mismatch: Asset Hub length differs from original Relay Chain data"
        );

		// Assert storage "ConvictionVoting::VotingFor::ah_post::correct"
		// Assert storage "ConvictionVoting::VotingFor::ah_post::consistent"
		// Assert storage "ConvictionVoting::ClassLocksFor::ah_post::correct"
		// Assert storage "ConvictionVoting::ClassLocksFor::ah_post::consistent"
		assert_eq!(
            rc_pre_payload, ah_messages,
            "Conviction voting data mismatch: Asset Hub data differs from original Relay Chain data"
        );
	}
}
