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
use frame_support::defensive;
/// Controls validator disabling
pub trait DisablingStrategy<T: Config> {
	/// Make a disabling decision. Returning a [`DisablingDecision`]
	fn decision(
		offender_stash: &T::ValidatorId,
		offender_slash_severity: OffenceSeverity,
		currently_disabled: &Vec<(u32, OffenceSeverity)>,
	) -> DisablingDecision;
}

/// Helper struct representing a decision coming from a given [`DisablingStrategy`] implementing
/// `decision`
///
/// `disable` is the index of the validator to disable,
/// `reenable` is the index of the validator to re-enable.
#[derive(Debug)]
pub struct DisablingDecision {
	pub disable: Option<u32>,
	pub reenable: Option<u32>,
}

impl<T: Config> DisablingStrategy<T> for () {
	fn decision(
		_offender_stash: &T::ValidatorId,
		_offender_slash_severity: OffenceSeverity,
		_currently_disabled: &Vec<(u32, OffenceSeverity)>,
	) -> DisablingDecision {
		DisablingDecision { disable: None, reenable: None }
	}
}
/// Calculate the disabling limit based on the number of validators and the disabling limit factor.
///
/// This is a sensible default implementation for the disabling limit factor for most disabling
/// strategies.
///
/// Disabling limit factor n=2 -> 1/n = 1/2 = 50% of validators can be disabled
fn factor_based_disable_limit(validators_len: usize, disabling_limit_factor: usize) -> usize {
	validators_len
		.saturating_sub(1)
		.checked_div(disabling_limit_factor)
		.unwrap_or_else(|| {
			defensive!("DISABLING_LIMIT_FACTOR should not be 0");
			0
		})
}

/// Implementation of [`DisablingStrategy`] using factor_based_disable_limit which disables
/// validators from the active set up to a threshold. `DISABLING_LIMIT_FACTOR` is the factor of the
/// maximum disabled validators in the active set. E.g. setting this value to `3` means no more than
/// 1/3 of the validators in the active set can be disabled in an era.
///
/// By default a factor of 3 is used which is the byzantine threshold.
pub struct UpToLimitDisablingStrategy<const DISABLING_LIMIT_FACTOR: usize = 3>;

impl<const DISABLING_LIMIT_FACTOR: usize> UpToLimitDisablingStrategy<DISABLING_LIMIT_FACTOR> {
	/// Disabling limit calculated from the total number of validators in the active set. When
	/// reached no more validators will be disabled.
	pub fn disable_limit(validators_len: usize) -> usize {
		factor_based_disable_limit(validators_len, DISABLING_LIMIT_FACTOR)
	}
}

impl<T: Config, const DISABLING_LIMIT_FACTOR: usize> DisablingStrategy<T>
	for UpToLimitDisablingStrategy<DISABLING_LIMIT_FACTOR>
{
	fn decision(
		offender_stash: &T::ValidatorId,
		_offender_slash_severity: OffenceSeverity,
		currently_disabled: &Vec<(u32, OffenceSeverity)>,
	) -> DisablingDecision {
		let active_set = Validators::<T>::get();

		// We don't disable more than the limit
		if currently_disabled.len() >= Self::disable_limit(active_set.len()) {
			log!(
				debug,
				"Won't disable: reached disabling limit {:?}",
				Self::disable_limit(active_set.len())
			);
			return DisablingDecision { disable: None, reenable: None }
		}

		let offender_idx = if let Some(idx) = active_set.iter().position(|i| i == offender_stash) {
			idx as u32
		} else {
			log!(debug, "Won't disable: offender not in active set",);
			return DisablingDecision { disable: None, reenable: None }
		};

		log!(debug, "Will disable {:?}", offender_idx);

		DisablingDecision { disable: Some(offender_idx), reenable: None }
	}
}

/// Implementation of [`DisablingStrategy`] which disables validators from the active set up to a
/// limit (factor_based_disable_limit) and if the limit is reached and the new offender is higher
/// (bigger punishment/severity) then it re-enables the lowest offender to free up space for the new
/// offender.
///
/// This strategy is not based on cumulative severity of offences but only on the severity of the
/// highest offence. Offender first committing a 25% offence and then a 50% offence will be treated
/// the same as an offender committing 50% offence.
///
/// An extension of [`UpToLimitDisablingStrategy`].
pub struct UpToLimitWithReEnablingDisablingStrategy<const DISABLING_LIMIT_FACTOR: usize = 3>;

impl<const DISABLING_LIMIT_FACTOR: usize>
	UpToLimitWithReEnablingDisablingStrategy<DISABLING_LIMIT_FACTOR>
{
	/// Disabling limit calculated from the total number of validators in the active set. When
	/// reached re-enabling logic might kick in.
	pub fn disable_limit(validators_len: usize) -> usize {
		factor_based_disable_limit(validators_len, DISABLING_LIMIT_FACTOR)
	}
}

impl<T: Config, const DISABLING_LIMIT_FACTOR: usize> DisablingStrategy<T>
	for UpToLimitWithReEnablingDisablingStrategy<DISABLING_LIMIT_FACTOR>
{
	fn decision(
		offender_stash: &T::ValidatorId,
		offender_slash_severity: OffenceSeverity,
		currently_disabled: &Vec<(u32, OffenceSeverity)>,
	) -> DisablingDecision {
		let active_set = Validators::<T>::get();

		// We don't disable validators that are not in the active set
		let offender_idx = if let Some(idx) = active_set.iter().position(|i| i == offender_stash) {
			idx as u32
		} else {
			log!(debug, "Won't disable: offender not in active set",);
			return DisablingDecision { disable: None, reenable: None }
		};

		// Check if offender is already disabled
		if let Some((_, old_severity)) =
			currently_disabled.iter().find(|(idx, _)| *idx == offender_idx)
		{
			if offender_slash_severity > *old_severity {
				log!(debug, "Offender already disabled but with lower severity, will disable again to refresh severity of {:?}", offender_idx);
				return DisablingDecision { disable: Some(offender_idx), reenable: None };
			} else {
				log!(debug, "Offender already disabled with higher or equal severity");
				return DisablingDecision { disable: None, reenable: None };
			}
		}

		// We don't disable more than the limit (but we can re-enable a smaller offender to make
		// space)
		if currently_disabled.len() >= Self::disable_limit(active_set.len()) {
			log!(
				debug,
				"Reached disabling limit {:?}, checking for re-enabling",
				Self::disable_limit(active_set.len())
			);

			// Find the smallest offender to re-enable that is not higher than
			// offender_slash_severity
			if let Some((smallest_idx, _)) = currently_disabled
				.iter()
				.filter(|(_, severity)| *severity <= offender_slash_severity)
				.min_by_key(|(_, severity)| *severity)
			{
				log!(debug, "Will disable {:?} and re-enable {:?}", offender_idx, smallest_idx);
				return DisablingDecision {
					disable: Some(offender_idx),
					reenable: Some(*smallest_idx),
				}
			} else {
				log!(debug, "No smaller offender found to re-enable");
				return DisablingDecision { disable: None, reenable: None }
			}
		} else {
			// If we are not at the limit, just disable the new offender and dont re-enable anyone
			log!(debug, "Will disable {:?}", offender_idx);
			return DisablingDecision { disable: Some(offender_idx), reenable: None }
		}
	}
}
