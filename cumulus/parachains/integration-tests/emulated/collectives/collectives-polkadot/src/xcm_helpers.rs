// Copyright Parity Technologies (UK) Ltd.
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

//! XCM helpers for getting delivery fees for tests

use xcm::latest::prelude::*;

pub fn query_response_delivery_fees<S: SendXcm>(querier: MultiLocation) -> u128 {
	// Message to calculate delivery fees, it's encoded size is what's important.
	// This message reports that there was no error, if an error is reported, the encoded size would
	// be different.
	let message = Xcm(vec![
		QueryResponse {
			query_id: 0, // Dummy query id
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(querier),
		},
		SetTopic([0u8; 32]), // Dummy topic
	]);
	let Ok((_, delivery_fees)) = validate_send::<S>(querier, message) else { unreachable!("message can be sent; qed") };
	let Fungible(delivery_fees_amount) = delivery_fees.inner()[0].fun else { unreachable!("Asset is fungible") };
	delivery_fees_amount
}
