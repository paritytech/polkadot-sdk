// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::dispatch::DispatchResult;
use xcm::prelude::Location;

pub trait Registry {
	fn register_agent(location: &Location) -> DispatchResult;

	fn register_token(location: &Location) -> DispatchResult;
}
