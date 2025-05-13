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
use hex_literal::hex;

/// These multisigs have historical issues where the deposit is missing for the creator.
const KNOWN_BAD_MULTISIGS: &[AccountId32] = &[
	AccountId32::new(hex!("e64d5c0de81b9c960c1dd900ad2a5d9d91c8a683e60dd1308e6bc7f80ea3b25f")),
	AccountId32::new(hex!("d55ec415b6703ddf7bec9d5c02a0b642f1f5bd068c6b3c50c2145544046f1491")),
	AccountId32::new(hex!("c2ff4f84b7fcee1fb04b4a97800e72321a4bc9939d456ad48d971127fc661c48")),
	AccountId32::new(hex!("0a8933d3f2164648399cc48cb8bb8c915abb94a2164c40ad6b48cee005f1cb6e")),
	AccountId32::new(hex!("ebe3cd53e580c4cd88acec1c952585b50a44a9b697d375ff648fee582ae39d38")),
	AccountId32::new(hex!("e64d5c0de81b9c960c1dd900ad2a5d9d91c8a683e60dd1308e6bc7f80ea3b25f")),
	AccountId32::new(hex!("caafae0aaa6333fcf4dc193146945fe8e4da74aa6c16d481eef0ca35b8279d73")),
	AccountId32::new(hex!("d429458e57ba6e9b21688441ff292c7cf82700550446b061a6c5dec306e1ef05")),
];

impl<T: Config> Pallet<T> {
	pub fn do_receive_multisigs(multisigs: Vec<RcMultisigOf<T>>) -> Result<(), Error<T>> {
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Multisig,
			count: multisigs.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating {} multisigs", multisigs.len());

		for multisig in multisigs {
			match Self::do_receive_multisig(multisig) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating multisig: {:?}", e);
				},
			}
		}
		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Multisig,
			count_good,
			count_bad,
		});

		Ok(())
	}

	pub fn do_receive_multisig(multisig: RcMultisigOf<T>) -> Result<(), Error<T>> {
		log::debug!(target: LOG_TARGET, "Integrating multisig {}, {:?}", multisig.creator.to_ss58check(), multisig.deposit);

		let missing = <T as pallet_multisig::Config>::Currency::unreserve(
			&multisig.creator,
			multisig.deposit,
		);

		if missing != Default::default() {
			if KNOWN_BAD_MULTISIGS.contains(&multisig.creator) {
				log::warn!(
					target: LOG_TARGET,
					"Failed to unreserve deposit for known bad multisig {}, missing: {:?}, account: {:?}",
					multisig.creator.to_ss58check(),
					missing,
					frame_system::Account::<T>::get(&multisig.creator)
				);
			} else {
				log::error!(
					target: LOG_TARGET,
					"Failed to unreserve deposit for multisig {}, missing: {:?}, details: {:?}",
					multisig.creator.to_ss58check(),
					missing,
					multisig.details
				);
			}

			return Err(Error::<T>::FailedToUnreserveDeposit);
		}

		Ok(())
	}
}
