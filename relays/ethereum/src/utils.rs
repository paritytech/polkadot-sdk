// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

/// Macro that returns (client, Err(error)) tuple from function if result is Err(error).
#[macro_export]
macro_rules! bail_on_error {
	($result: expr) => {
		match $result {
			(client, Ok(result)) => (client, result),
			(client, Err(error)) => return (client, Err(error)),
			}
	};
}

/// Macro that returns (client, Err(error)) tuple from function if result is Err(error).
#[macro_export]
macro_rules! bail_on_arg_error {
	($result: expr, $client: ident) => {
		match $result {
			Ok(result) => result,
			Err(error) => return ($client, Err(error)),
			}
	};
}
