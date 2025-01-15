// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]
use bridge_hub_common::DenyExportMessageFrom;
use frame_support::{
	assert_err, assert_ok, parameter_types,
	traits::{Equals, Everything, EverythingBut, ProcessMessageError},
};
use xcm::prelude::{
	AliasOrigin, All, AssetFilter, DepositReserveAsset, Ethereum, ExportMessage, Here, Instruction,
	Location, NetworkId, NetworkId::Polkadot, Parachain, Weight, Wild,
};
use xcm_builder::{DenyReserveTransferToRelayChain, DenyThenTry, TakeWeightCredit};
use xcm_executor::traits::{Properties, ShouldExecute};

parameter_types! {
	pub AssetHubLocation: Location = Location::new(1, Parachain(1000));
	pub ParachainLocation: Location = Location::new(1, Parachain(2000));
	pub EthereumNetwork: NetworkId = Ethereum { chain_id: 1 };
}

#[test]
fn deny_export_message_from_source_other_than_asset_hub_should_work() {
	pub type Barrier = DenyThenTry<
		(
			DenyReserveTransferToRelayChain,
			DenyExportMessageFrom<EverythingBut<Equals<AssetHubLocation>>, Equals<EthereumNetwork>>,
		),
		TakeWeightCredit,
	>;

	let mut xcm: Vec<Instruction<()>> = vec![
		AliasOrigin(Here.into()),
		ExportMessage {
			network: EthereumNetwork::get(),
			destination: Here,
			xcm: Default::default(),
		},
	];

	let result = Barrier::should_execute(
		&ParachainLocation::get(),
		&mut xcm,
		Weight::zero(),
		&mut Properties { weight_credit: Weight::zero(), message_id: None },
	);

	assert_err!(result, ProcessMessageError::Unsupported);
}

#[test]
fn allow_export_message_from_asset_hub_should_work() {
	pub type Barrier = DenyThenTry<
		(
			DenyReserveTransferToRelayChain,
			DenyExportMessageFrom<EverythingBut<Equals<AssetHubLocation>>, Equals<EthereumNetwork>>,
		),
		TakeWeightCredit,
	>;

	let mut xcm: Vec<Instruction<()>> = vec![
		AliasOrigin(Here.into()),
		ExportMessage {
			network: EthereumNetwork::get(),
			destination: Here,
			xcm: Default::default(),
		},
	];

	let result = Barrier::should_execute(
		&AssetHubLocation::get(),
		&mut xcm,
		Weight::zero(),
		&mut Properties { weight_credit: Weight::zero(), message_id: None },
	);

	assert_ok!(result);
}

#[test]
fn allow_export_message_from_source_other_than_asset_hub_if_destination_other_than_ethereum() {
	pub type Barrier = DenyThenTry<
		(
			DenyReserveTransferToRelayChain,
			DenyExportMessageFrom<EverythingBut<Equals<AssetHubLocation>>, Equals<EthereumNetwork>>,
		),
		TakeWeightCredit,
	>;

	let mut xcm: Vec<Instruction<()>> = vec![
		AliasOrigin(Here.into()),
		ExportMessage { network: Polkadot, destination: Here, xcm: Default::default() },
	];

	let result = Barrier::should_execute(
		&ParachainLocation::get(),
		&mut xcm,
		Weight::zero(),
		&mut Properties { weight_credit: Weight::zero(), message_id: None },
	);

	assert_ok!(result);
}

#[test]
fn deny_reserver_transfer_to_relaychain_does_not_break() {
	pub type Barrier = DenyThenTry<
		(DenyReserveTransferToRelayChain, DenyExportMessageFrom<Everything, Everything>),
		TakeWeightCredit,
	>;

	let mut xcm: Vec<Instruction<()>> = vec![DepositReserveAsset {
		assets: AssetFilter::try_from(Wild(All)).unwrap(),
		dest: Location { parents: 1, interior: Here },
		xcm: Default::default(),
	}];

	let result = Barrier::should_execute(
		&Here.into(),
		&mut xcm,
		Weight::zero(),
		&mut Properties { weight_credit: Weight::zero(), message_id: None },
	);

	assert_err!(result, ProcessMessageError::Unsupported);
}
