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

//! Types

use super::*;
use pallet_referenda::{ReferendumInfoOf, TrackIdOf};
use sp_runtime::FixedU128;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Asset Hub Pallet list with indexes.
#[derive(Encode, Decode)]
pub enum AssetHubPalletConfig<T: Config> {
	#[codec(index = 255)]
	AhmController(AhMigratorCall<T>),
}

/// Call encoding for the calls needed from the ah-migrator pallet.
#[derive(Encode, Decode)]
pub enum AhMigratorCall<T: Config> {
	#[codec(index = 0)]
	ReceiveAccounts { accounts: Vec<accounts::AccountFor<T>> },
	#[codec(index = 1)]
	ReceiveMultisigs { multisigs: Vec<multisig::RcMultisigOf<T>> },
	#[codec(index = 2)]
	ReceiveProxyProxies { proxies: Vec<proxy::RcProxyLocalOf<T>> },
	#[codec(index = 3)]
	ReceiveProxyAnnouncements { announcements: Vec<RcProxyAnnouncementOf<T>> },
	#[codec(index = 4)]
	ReceivePreimageChunks { chunks: Vec<preimage::RcPreimageChunk> },
	#[codec(index = 5)]
	ReceivePreimageRequestStatus { request_status: Vec<preimage::RcPreimageRequestStatusOf<T>> },
	#[codec(index = 6)]
	ReceivePreimageLegacyStatus { legacy_status: Vec<preimage::RcPreimageLegacyStatusOf<T>> },
	#[codec(index = 7)]
	ReceiveNomPoolsMessages { messages: Vec<staking::nom_pools::RcNomPoolsMessage<T>> },
	#[codec(index = 8)]
	ReceiveVestingSchedules { messages: Vec<vesting::RcVestingSchedule<T>> },
	#[codec(index = 9)]
	ReceiveFastUnstakeMessages { messages: Vec<staking::fast_unstake::RcFastUnstakeMessage<T>> },
	#[codec(index = 10)]
	ReceiveReferendaValues {
		referendum_count: u32,
		deciding_count: Vec<(TrackIdOf<T, ()>, u32)>,
		track_queue: Vec<(TrackIdOf<T, ()>, Vec<(u32, u128)>)>,
	},
	#[codec(index = 11)]
	ReceiveReferendums { referendums: Vec<(u32, ReferendumInfoOf<T, ()>)> },
	// Claims pallet not on Westend.
	// #[codec(index = 12)]
	// ReceiveClaimsMessages { messages: Vec<claims::RcClaimsMessageOf<T>> },
	#[codec(index = 13)]
	ReceiveBagsListMessages { messages: Vec<staking::bags_list::RcBagsListMessage<T>> },
	#[codec(index = 14)]
	ReceiveSchedulerMessages { messages: Vec<scheduler::RcSchedulerMessageOf<T>> },
	#[codec(index = 15)]
	ReceiveIndices { indices: Vec<indices::RcIndicesIndexOf<T>> },
	#[codec(index = 16)]
	ReceiveConvictionVotingMessages {
		messages: Vec<conviction_voting::RcConvictionVotingMessageOf<T>>,
	},
	// Bounties pallet not on Westend.
	// #[codec(index = 17)]
	// ReceiveBountiesMessages { messages: Vec<bounties::RcBountiesMessageOf<T>> },
	#[codec(index = 18)]
	ReceiveAssetRates { asset_rates: Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)> },
	#[codec(index = 19)]
	ReceiveCrowdloanMessages { messages: Vec<crowdloan::RcCrowdloanMessageOf<T>> },
	#[codec(index = 20)]
	ReceiveStakingMessages { messages: Vec<staking::RcStakingMessageOf<T>> },
	#[codec(index = 101)]
	StartMigration,
}

/// Copy of `ParaInfo` type from `paras_registrar` pallet.
///
/// From: https://github.com/paritytech/polkadot-sdk/blob/b7afe48ed0bfef30836e7ca6359c2d8bb594d16e/polkadot/runtime/common/src/paras_registrar/mod.rs#L50-L59
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
pub struct ParaInfo<AccountId, Balance> {
	/// The account that has placed a deposit for registering this para.
	pub manager: AccountId,
	/// The amount reserved by the `manager` account for the registration.
	pub deposit: Balance,
	/// Whether the para registration should be locked from being controlled by the manager.
	/// None means the lock had not been explicitly set, and should be treated as false.
	pub locked: Option<bool>,
}

pub trait PalletMigration {
	type Key: codec::MaxEncodedLen;
	type Error;

	/// Migrate until the weight is exhausted. The give key is the last one that was migrated.
	///
	/// Should return the last key that was migrated. This will then be passed back into the next
	/// call.
	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error>;
}

/// Trait to run some checks on the Relay Chain before and after a pallet migration.
///
/// This needs to be called by the test harness.
pub trait RcMigrationCheck {
	/// Relay Chain payload which is exported for migration checks.
	type RcPrePayload: Clone;

	/// Run some checks on the relay chain before the migration and store intermediate payload.
	/// The expected output should contain the data being transferred out of the relay chain and it
	/// will .
	fn pre_check() -> Self::RcPrePayload;

	/// Run some checks on the relay chain after the migration and use the intermediate payload.
	/// The expected input should contain the data just transferred out of the relay chain, to allow
	/// the check that data has been removed from the relay chain.
	fn post_check(rc_pre_payload: Self::RcPrePayload);
}

#[impl_trait_for_tuples::impl_for_tuples(16)]
impl RcMigrationCheck for Tuple {
	for_tuples! { type RcPrePayload = (#( Tuple::RcPrePayload ),* ); }

	fn pre_check() -> Self::RcPrePayload {
		(for_tuples! { #(
			Tuple::pre_check()
		),* })
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload) {
		(for_tuples! { #(
			Tuple::post_check(rc_pre_payload.Tuple)
		),* });
	}
}

pub trait MigrationStatus {
	/// Whether the migration is finished.
	///
	/// This is **not** the same as `!self.is_ongoing()` since it may not have started.
	fn is_finished() -> bool;
	/// Whether the migration is ongoing.
	///
	/// This is **not** the same as `!self.is_finished()` since it may not have started.
	fn is_ongoing() -> bool;
}

/// A weight that is zero if the migration is ongoing, otherwise it is the default weight.
pub struct ZeroWeightOr<Status, Default>(PhantomData<(Status, Default)>);
impl<Status: MigrationStatus, Default: Get<Weight>> Get<Weight> for ZeroWeightOr<Status, Default> {
	fn get() -> Weight {
		Status::is_ongoing().then(Weight::zero).unwrap_or_else(Default::get)
	}
}
