// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::dispatch::DispatchResult;
use xcm::prelude::Location;

pub trait TokenRegistry {
	fn register(location: Location) -> DispatchResult;
}

pub trait AgentRegistry {
	fn register(location: Location) -> DispatchResult;
}
