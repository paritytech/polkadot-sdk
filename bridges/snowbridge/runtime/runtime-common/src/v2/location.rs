// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::traits::{Contains, Get};
use sp_std::marker::PhantomData;
use xcm::prelude::*;

/// Disallow a specific origin location.
///
/// This provides defense-in-depth by rejecting AliasOrigin claiming to be the
/// specified `ExcludedLocation`.
pub struct DisallowOrigin<ExcludedLocation>(PhantomData<ExcludedLocation>);
impl<ExcludedLocation: Get<Location>> Contains<Location> for DisallowOrigin<ExcludedLocation> {
	fn contains(l: &Location) -> bool {
		l != &ExcludedLocation::get()
	}
}
