// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Helpers for implementing runtime api

use crate::Config;
use snowbridge_core::{AgentId, AgentIdOf};
use xcm::{prelude::*, VersionedLocation};
use xcm_executor::traits::ConvertLocation;

pub fn agent_id<Runtime>(location: VersionedLocation) -> Option<AgentId>
where
	Runtime: Config,
{
	let location: Location = location.try_into().ok()?;
	AgentIdOf::convert_location(&location)
}
