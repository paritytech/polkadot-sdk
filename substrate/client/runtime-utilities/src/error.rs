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
//! Errors types of runtime utilities.

/// Generic result for the runtime utilities.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for the runtime utilities.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
	#[error("Scale codec error: {0}")]
	ScaleCodec(#[from] codec::Error),
	#[error("Opaque metadata not found")]
	OpaqueMetadataNotFound,
	#[error("Stable metadata version not found")]
	StableMetadataVersionNotFound,
	#[error("WASM executor error: {0}")]
	Executor(#[from] sc_executor_common::error::Error),
}
