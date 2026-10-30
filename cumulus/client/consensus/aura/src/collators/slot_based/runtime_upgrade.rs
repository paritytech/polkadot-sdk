// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Runtime upgrade detection: nothing may build on a block applying a runtime upgrade until that
//! block is included.

use sp_runtime::{traits::Header as HeaderT, DigestItem};

/// Whether `header` applies a runtime upgrade.
pub(crate) fn has_runtime_upgrade_digest<H: HeaderT>(header: &H) -> bool {
	header
		.digest()
		.logs
		.iter()
		.any(|it| matches!(it, DigestItem::RuntimeEnvironmentUpdated))
}
