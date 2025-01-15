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

use super::ExecuteInstruction;
use crate::{config, XcmExecutor, traits::VersionChangeNotifier};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;
use frame_support::ensure;

impl<Config: config::Config> ExecuteInstruction<Config> for SubscribeVersion {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let SubscribeVersion { query_id, max_response_weight } = self;
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		// We don't allow derivative origins to subscribe since it would otherwise pose a
		// DoS risk.
		ensure!(&executor.original_origin == origin, XcmError::BadOrigin);
		Config::SubscriptionService::start(
			origin,
			query_id,
			max_response_weight,
			&executor.context,
		)
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for UnsubscribeVersion {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		ensure!(&executor.original_origin == origin, XcmError::BadOrigin);
		Config::SubscriptionService::stop(origin, &executor.context)
	}
}
