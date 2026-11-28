// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! A set of common definitions that are needed for defining execution engines.

#![warn(missing_docs)]
#![deny(unused_crate_dependencies)]

pub mod error;
pub mod runtime_blob;
pub mod util;
pub mod wasm_runtime;

/// Extracts the runtime version of a given runtime code.
pub trait RuntimeVersionOf {
	/// Extract [`RuntimeVersion`](sp_version::RuntimeVersion) of the given `runtime_code`.
	fn runtime_version(
		&self,
		ext: &mut dyn sp_externalities::Externalities,
		runtime_code: &sp_core::traits::RuntimeCode,
	) -> error::Result<sp_version::RuntimeVersion>;
}
