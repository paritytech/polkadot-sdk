// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Helpers for implementing runtime api

use snowbridge_core::AgentId;
use xcm::{prelude::*, VersionedMultiLocation};

use crate::{agent_id_of, Config};

pub fn agent_id<Runtime>(location: VersionedMultiLocation) -> Option<AgentId>
where
	Runtime: Config,
{
	let location: MultiLocation = location.try_into().ok()?;
	agent_id_of::<Runtime>(&location).ok()
}
