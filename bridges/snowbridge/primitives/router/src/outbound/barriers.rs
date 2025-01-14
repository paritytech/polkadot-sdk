// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use core::{marker::PhantomData, ops::ControlFlow};
use frame_support::traits::{Contains, ProcessMessageError};
use xcm::prelude::{ExportMessage, Instruction, Location, NetworkId, Weight};
use xcm_builder::{CreateMatcher, MatchXcm};
use xcm_executor::traits::{Properties, ShouldExecute};

pub struct DenyFirstExportMessageFrom<FromOrigin, ToGlobalConsensus>(
	PhantomData<(FromOrigin, ToGlobalConsensus)>,
);

impl<FromOrigin, ToGlobalConsensus> ShouldExecute
	for DenyFirstExportMessageFrom<FromOrigin, ToGlobalConsensus>
where
	FromOrigin: Contains<Location>,
	ToGlobalConsensus: Contains<NetworkId>,
{
	fn should_execute<RuntimeCall>(
		origin: &Location,
		message: &mut [Instruction<RuntimeCall>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		message.matcher().match_next_inst_while(
			|_| true,
			|inst| match inst {
				ExportMessage { network, .. } =>
					if ToGlobalConsensus::contains(network) && FromOrigin::contains(origin) {
						return Err(ProcessMessageError::Unsupported)
					} else {
						Ok(ControlFlow::Continue(()))
					},
				_ => Ok(ControlFlow::Continue(())),
			},
		)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		assert_err, parameter_types,
		traits::{Equals, Everything, EverythingBut},
	};
	use xcm::prelude::*;
	use xcm_builder::{DenyReserveTransferToRelayChain, DenyThenTry, TakeWeightCredit};

	parameter_types! {
		pub AssetHubLocation: Location = Location::new(1, Parachain(1000));
		pub EthereumNetwork: NetworkId = Ethereum { chain_id: 1 };
	}

	#[test]
	fn deny_export_message_from_source() {
		let mut xcm: Vec<Instruction<()>> =
			vec![ExportMessage { network: EthereumNetwork::get(), destination: Here, xcm: Default::default() }];

		let result = DenyFirstExportMessageFrom::<
			EverythingBut<Equals<AssetHubLocation>>,
			Everything,
		>::should_execute(
			&Location::parent(),
			&mut xcm,
			Weight::zero(),
			&mut Properties { weight_credit: Weight::zero(), message_id: None },
		);
		assert_err!(result, ProcessMessageError::Unsupported);
	}

	#[test]
	fn allow_export_message_from_source() {
		let mut xcm: Vec<Instruction<()>> =
			vec![ExportMessage { network: Polkadot, destination: Here, xcm: Default::default() }];

		let result = DenyFirstExportMessageFrom::<
			EverythingBut<Equals<AssetHubLocation>>,
			Everything,
		>::should_execute(
			&AssetHubLocation::get(),
			&mut xcm,
			Weight::zero(),
			&mut Properties { weight_credit: Weight::zero(), message_id: None },
		);
		assert!(result.is_ok());
	}

	#[test]
	fn allow_xcm_without_export_message() {
		let mut xcm: Vec<Instruction<()>> = vec![ClearOrigin];

		let result = DenyFirstExportMessageFrom::<Everything, Everything>::should_execute(
			&Location::parent(),
			&mut xcm,
			Weight::zero(),
			&mut Properties { weight_credit: Weight::zero(), message_id: None },
		);
		assert!(result.is_ok());
	}
}
