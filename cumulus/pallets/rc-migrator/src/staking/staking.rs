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

//! Pallet staking migration.

pub use crate::staking::message::{
	AhEquivalentStakingMessageOf, RcStakingMessage, RcStakingMessageOf,
};
use crate::{staking::IntoAh, *};
use codec::{EncodeLike, HasCompact};
use core::fmt::Debug;
pub use frame_election_provider_support::PageIndex;
use frame_support::traits::DefensiveTruncateInto;
use pallet_staking::{
	slashing::{SlashingSpans, SpanIndex, SpanRecord},
	ActiveEraInfo, EraRewardPoints, Forcing, Nominations, RewardDestination, StakingLedger,
	ValidatorPrefs,
};
use sp_runtime::{Perbill, Percent};
use sp_staking::{EraIndex, ExposurePage, Page, PagedExposureMetadata, SessionIndex};

pub struct StakingMigrator<T> {
	_phantom: PhantomData<T>,
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Default,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum StakingStage<AccountId> {
	#[default]
	Values,
	Invulnerables,
	Bonded(Option<AccountId>),
	Ledger(Option<AccountId>),
	Payee(Option<AccountId>),
	Validators(Option<AccountId>),
	Nominators(Option<AccountId>),
	VirtualStakers(Option<AccountId>),
	ErasStakersOverview(Option<(EraIndex, AccountId)>),
	ErasStakersPaged(Option<(EraIndex, AccountId, Page)>),
	ClaimedRewards(Option<(EraIndex, AccountId)>),
	ErasValidatorPrefs(Option<(EraIndex, AccountId)>),
	ErasValidatorReward(Option<EraIndex>),
	ErasRewardPoints(Option<EraIndex>),
	ErasTotalStake(Option<EraIndex>),
	UnappliedSlashes(Option<EraIndex>),
	BondedEras,
	ValidatorSlashInEra(Option<(EraIndex, AccountId)>),
	NominatorSlashInEra(Option<(EraIndex, AccountId)>),
	SlashingSpans(Option<AccountId>),
	SpanSlash(Option<(AccountId, SpanIndex)>),
	Finished,
}

pub type StakingStageOf<T> = StakingStage<<T as frame_system::Config>::AccountId>;

pub type BalanceOf<T> = <T as pallet_staking::Config>::CurrencyBalance;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

impl<T: Config> PalletMigration for StakingMigrator<T> {
	type Key = StakingStageOf<T>;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or_default();
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

		loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}

			if messages.len() > 10_000 {
				log::warn!("Weight allowed very big batch, stopping");
				break;
			}

			inner_key = match inner_key {
				StakingStage::Values => {
					let values = Self::take_values();
					messages.push(RcStakingMessage::Values(values));
					StakingStage::Invulnerables
				},
				StakingStage::Invulnerables => {
					let invulnerables = pallet_staking::Invulnerables::<T>::take();
					messages.push(RcStakingMessage::Invulnerables(invulnerables));
					StakingStage::Bonded(None)
				},
				StakingStage::Bonded(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Bonded::<T>::iter_from_key(who)
					} else {
						pallet_staking::Bonded::<T>::iter()
					};

					match iter.next() {
						Some((stash, controller)) => {
							pallet_staking::Bonded::<T>::remove(&stash);
							messages.push(RcStakingMessage::Bonded {
								stash: stash.clone(),
								controller,
							});
							StakingStage::Bonded(Some(stash))
						},
						None => StakingStage::Ledger(None),
					}
				},
				StakingStage::Ledger(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Ledger::<T>::iter_from_key(who)
					} else {
						pallet_staking::Ledger::<T>::iter()
					};

					match iter.next() {
						Some((controller, ledger)) => {
							pallet_staking::Ledger::<T>::remove(&controller);
							messages.push(RcStakingMessage::Ledger {
								controller: controller.clone(),
								ledger,
							});
							StakingStage::Ledger(Some(controller))
						},
						None => StakingStage::Payee(None),
					}
				},
				StakingStage::Payee(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Payee::<T>::iter_from_key(who)
					} else {
						pallet_staking::Payee::<T>::iter()
					};

					match iter.next() {
						Some((stash, payment)) => {
							pallet_staking::Payee::<T>::remove(&stash);
							messages
								.push(RcStakingMessage::Payee { stash: stash.clone(), payment });
							StakingStage::Payee(Some(stash))
						},
						None => StakingStage::Validators(None),
					}
				},
				StakingStage::Validators(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Validators::<T>::iter_from(
							pallet_staking::Validators::<T>::hashed_key_for(who),
						)
					} else {
						pallet_staking::Validators::<T>::iter()
					};

					match iter.next() {
						Some((stash, validators)) => {
							pallet_staking::Validators::<T>::remove(&stash);
							messages.push(RcStakingMessage::Validators {
								stash: stash.clone(),
								validators,
							});
							StakingStage::Validators(Some(stash))
						},
						None => StakingStage::Nominators(None),
					}
				},
				StakingStage::Nominators(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Nominators::<T>::iter_from(
							pallet_staking::Nominators::<T>::hashed_key_for(who),
						)
					} else {
						pallet_staking::Nominators::<T>::iter()
					};

					match iter.next() {
						Some((stash, nominations)) => {
							pallet_staking::Nominators::<T>::remove(&stash);
							messages.push(RcStakingMessage::Nominators {
								stash: stash.clone(),
								nominations,
							});
							StakingStage::Nominators(Some(stash))
						},
						None => StakingStage::VirtualStakers(None),
					}
				},
				StakingStage::VirtualStakers(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::VirtualStakers::<T>::iter_from(
							// Counted maps dont have the convenience function here
							pallet_staking::VirtualStakers::<T>::hashed_key_for(who),
						)
					} else {
						pallet_staking::VirtualStakers::<T>::iter()
					};

					match iter.next() {
						Some((staker, ())) => {
							pallet_staking::VirtualStakers::<T>::remove(&staker);
							messages.push(RcStakingMessage::VirtualStakers(staker.clone()));
							StakingStage::VirtualStakers(Some(staker))
						},
						None => StakingStage::ErasStakersOverview(None),
					}
				},
				StakingStage::ErasStakersOverview(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ErasStakersOverview::<T>::iter_from(
							pallet_staking::ErasStakersOverview::<T>::hashed_key_for(
								progress.0, progress.1,
							),
						)
					} else {
						pallet_staking::ErasStakersOverview::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, exposure)) => {
							pallet_staking::ErasStakersOverview::<T>::remove(&era, &validator);
							messages.push(RcStakingMessage::ErasStakersOverview {
								era,
								validator: validator.clone(),
								exposure,
							});
							StakingStage::ErasStakersOverview(Some((era, validator)))
						},
						None => StakingStage::ErasStakersPaged(None),
					}
				},
				StakingStage::ErasStakersPaged(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ErasStakersPaged::<T>::iter_from(
							pallet_staking::ErasStakersPaged::<T>::hashed_key_for(progress),
						)
					} else {
						pallet_staking::ErasStakersPaged::<T>::iter()
					};

					match iter.next() {
						Some(((era, validator, page), exposure)) => {
							pallet_staking::ErasStakersPaged::<T>::remove((
								&era, &validator, &page,
							));
							messages.push(RcStakingMessage::ErasStakersPaged {
								era,
								validator: validator.clone(),
								page,
								exposure,
							});
							StakingStage::ErasStakersPaged(Some((era, validator, page)))
						},
						None => StakingStage::ClaimedRewards(None),
					}
				},
				StakingStage::ClaimedRewards(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ClaimedRewards::<T>::iter_from(
							pallet_staking::ClaimedRewards::<T>::hashed_key_for(
								progress.0, progress.1,
							),
						)
					} else {
						pallet_staking::ClaimedRewards::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, rewards)) => {
							pallet_staking::ClaimedRewards::<T>::remove(&era, &validator);
							messages.push(RcStakingMessage::ClaimedRewards {
								era,
								validator: validator.clone(),
								rewards,
							});
							StakingStage::ClaimedRewards(Some((era, validator)))
						},
						None => StakingStage::ErasValidatorPrefs(None),
					}
				},
				StakingStage::ErasValidatorPrefs(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ErasValidatorPrefs::<T>::iter_from(
							pallet_staking::ErasValidatorPrefs::<T>::hashed_key_for(
								progress.0, progress.1,
							),
						)
					} else {
						pallet_staking::ErasValidatorPrefs::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, prefs)) => {
							pallet_staking::ErasValidatorPrefs::<T>::remove(&era, &validator);
							messages.push(RcStakingMessage::ErasValidatorPrefs {
								era,
								validator: validator.clone(),
								prefs,
							});
							StakingStage::ErasValidatorPrefs(Some((era, validator)))
						},
						None => StakingStage::ErasValidatorReward(None),
					}
				},
				StakingStage::ErasValidatorReward(era) => {
					let mut iter = resume::<pallet_staking::ErasValidatorReward<T>, _, _>(era);

					match iter.next() {
						Some((era, reward)) => {
							pallet_staking::ErasValidatorReward::<T>::remove(&era);
							messages.push(RcStakingMessage::ErasValidatorReward { era, reward });
							StakingStage::ErasValidatorReward(Some(era))
						},
						None => StakingStage::ErasRewardPoints(None),
					}
				},
				StakingStage::ErasRewardPoints(era) => {
					let mut iter = resume::<pallet_staking::ErasRewardPoints<T>, _, _>(era);

					match iter.next() {
						Some((era, points)) => {
							pallet_staking::ErasRewardPoints::<T>::remove(&era);
							messages.push(RcStakingMessage::ErasRewardPoints { era, points });
							StakingStage::ErasRewardPoints(Some(era))
						},
						None => StakingStage::ErasTotalStake(None),
					}
				},
				StakingStage::ErasTotalStake(era) => {
					let mut iter = resume::<pallet_staking::ErasTotalStake<T>, _, _>(era);

					match iter.next() {
						Some((era, total_stake)) => {
							pallet_staking::ErasTotalStake::<T>::remove(&era);
							messages.push(RcStakingMessage::ErasTotalStake { era, total_stake });
							StakingStage::ErasTotalStake(Some(era))
						},
						None => StakingStage::UnappliedSlashes(None),
					}
				},
				StakingStage::UnappliedSlashes(era) => {
					let mut iter = resume::<pallet_staking::UnappliedSlashes<T>, _, _>(era);

					match iter.next() {
						Some((era, slashes)) => {
							pallet_staking::UnappliedSlashes::<T>::remove(&era);

							if slashes.len() > 1000 {
								defensive!("Lots of unapplied slashes for era, this is odd");
							}

							// Translate according to https://github.com/paritytech/polkadot-sdk/blob/43ea306f6307dff908551cb91099ef6268502ee0/substrate/frame/staking/src/migrations.rs#L94-L108
							for slash in slashes.into_iter().take(1000) {
								// First 1000 slashes should be enough, just to avoid unbound loop
								messages.push(RcStakingMessage::UnappliedSlashes { era, slash });
							}
							StakingStage::UnappliedSlashes(Some(era))
						},
						None => StakingStage::BondedEras,
					}
				},
				StakingStage::BondedEras => {
					let bonded_eras = pallet_staking::BondedEras::<T>::take();
					messages.push(RcStakingMessage::BondedEras(bonded_eras));
					StakingStage::ValidatorSlashInEra(None)
				},
				StakingStage::ValidatorSlashInEra(next) => {
					let mut iter = if let Some(next) = next {
						pallet_staking::ValidatorSlashInEra::<T>::iter_from(
							pallet_staking::ValidatorSlashInEra::<T>::hashed_key_for(
								next.0, next.1,
							),
						)
					} else {
						pallet_staking::ValidatorSlashInEra::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, slash)) => {
							pallet_staking::ValidatorSlashInEra::<T>::remove(&era, &validator);
							messages.push(RcStakingMessage::ValidatorSlashInEra {
								era,
								validator: validator.clone(),
								slash,
							});
							StakingStage::ValidatorSlashInEra(Some((era, validator)))
						},
						None => StakingStage::NominatorSlashInEra(None),
					}
				},
				StakingStage::NominatorSlashInEra(next) => {
					let mut iter = if let Some(next) = next {
						pallet_staking::NominatorSlashInEra::<T>::iter_from(
							pallet_staking::NominatorSlashInEra::<T>::hashed_key_for(
								next.0, next.1,
							),
						)
					} else {
						pallet_staking::NominatorSlashInEra::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, slash)) => {
							pallet_staking::NominatorSlashInEra::<T>::remove(&era, &validator);
							messages.push(RcStakingMessage::NominatorSlashInEra {
								era,
								validator: validator.clone(),
								slash,
							});
							StakingStage::NominatorSlashInEra(Some((era, validator)))
						},
						None => StakingStage::SlashingSpans(None),
					}
				},
				StakingStage::SlashingSpans(account) => {
					let mut iter = resume::<pallet_staking::SlashingSpans<T>, _, _>(account);

					match iter.next() {
						Some((account, spans)) => {
							pallet_staking::SlashingSpans::<T>::remove(&account);
							messages.push(RcStakingMessage::SlashingSpans {
								account: account.clone(),
								spans,
							});
							StakingStage::SlashingSpans(Some(account))
						},
						None => StakingStage::SpanSlash(None),
					}
				},
				StakingStage::SpanSlash(next) => {
					let mut iter = resume::<pallet_staking::SpanSlash<T>, _, _>(next);

					match iter.next() {
						Some(((account, span), slash)) => {
							pallet_staking::SpanSlash::<T>::remove((&account, &span));
							messages.push(RcStakingMessage::SpanSlash {
								account: account.clone(),
								span,
								slash,
							});
							StakingStage::SpanSlash(Some((account, span)))
						},
						None => StakingStage::Finished,
					}
				},
				StakingStage::Finished => {
					break;
				},
			};
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveStakingMessages { messages },
				|_len| Weight::from_all(1),
			)?;
		}

		if inner_key == StakingStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}

use codec::{FullCodec, FullEncode};
fn resume<Map: frame_support::IterableStorageMap<K, V>, K: FullEncode, V: FullCodec>(
	key: Option<K>,
) -> impl Iterator<Item = (K, V)> {
	if let Some(key) = key {
		Map::iter_from(Map::hashed_key_for(key))
	} else {
		Map::iter()
	}
}
