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

//! Defines context trait for PVQ executor.

use polkavm::Linker;

/// Provides host integration for [`crate::PvqExecutor`].
///
/// A context is responsible for registering host functions into the [`Linker`] and for providing
/// the mutable user data value passed to guest calls.
pub trait PvqExecutorContext {
	/// The user data passed to host functions.
	///
	/// This is the `T` parameter of [`Linker<T, E>`].
	type UserData;
	/// The user-defined error type returned by host functions.
	///
	/// This is the `E` parameter of [`Linker<T, E>`] and becomes [`crate::PvqExecutorError::User`].
	type UserError;

	/// Registers host functions with the given [`Linker`].
	///
	/// This is called by [`crate::PvqExecutor::new`] exactly once during construction.
	fn register_host_functions(&mut self, linker: &mut Linker<Self::UserData, Self::UserError>);

	/// Returns a mutable reference to the user data.
	///
	/// The executor calls this right before invoking the guest entrypoint, and passes the returned
	/// reference to PolkaVM so it is accessible to host functions.
	fn data(&mut self) -> &mut Self::UserData;
}
