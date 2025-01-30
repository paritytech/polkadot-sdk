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
	parameter_types,
	traits::{Equals, EverythingBut, ProcessMessageError::Unsupported},
};
use xcm::prelude::{
	AliasOrigin, ByGenesis, ExportMessage, Here, Instruction, Location, NetworkId, Parachain,
	Weight,
};
use xcm_executor::traits::{DenyExecution, Properties};

#[test]
fn test_deny_export_message_from() {
	parameter_types! {
		pub Source1: Location = Location::new(1, Parachain(1));
		pub Source2: Location = Location::new(1, Parachain(2));
		pub Remote1: NetworkId = ByGenesis([1;32]);
		pub Remote2: NetworkId = ByGenesis([2;32]);
	}

	// Deny ExportMessage when both of the conditions met:
	// 1: source != Source1
	// 2: network == Remote1
	pub type Denied = DenyExportMessageFrom<EverythingBut<Equals<Source1>>, Equals<Remote1>>;

	let assert_deny_execution = |mut xcm: Vec<Instruction<()>>, origin, expected_result| {
		assert_eq!(
			Denied::deny_execution(
				&origin,
				&mut xcm,
				Weight::zero(),
				&mut Properties { weight_credit: Weight::zero(), message_id: None }
			),
			expected_result
		);
	};

	// A message without an `ExportMessage` should pass
	assert_deny_execution(vec![AliasOrigin(Here.into())], Source1::get(), Ok(()));

	// `ExportMessage` from source1 and remote1 should pass
	assert_deny_execution(
		vec![ExportMessage { network: Remote1::get(), destination: Here, xcm: Default::default() }],
		Source1::get(),
		Ok(()),
	);

	// `ExportMessage` from source1 and remote2 should pass
	assert_deny_execution(
		vec![ExportMessage { network: Remote2::get(), destination: Here, xcm: Default::default() }],
		Source1::get(),
		Ok(()),
	);

	// `ExportMessage` from source2 and remote2 should pass
	assert_deny_execution(
		vec![ExportMessage { network: Remote2::get(), destination: Here, xcm: Default::default() }],
		Source2::get(),
		Ok(()),
	);

	// `ExportMessage` from source2 and remote1 should be banned
	assert_deny_execution(
		vec![ExportMessage { network: Remote1::get(), destination: Here, xcm: Default::default() }],
		Source2::get(),
		Err(Unsupported),
	);
}
