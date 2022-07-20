// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Primitives of messages module.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{fmt::Debug, marker::PhantomData};

/// Reward payment procedure.
pub trait PaymentProcedure<Relayer, Reward> {
	/// Error that may be returned by the procedure.
	type Error: Debug;

	/// Pay reward to the relayer.
	fn pay_reward(relayer: &Relayer, reward: Reward) -> Result<(), Self::Error>;
}

/// Reward payment procedure that is simply minting given amount of tokens.
pub struct MintReward<T, Relayer>(PhantomData<(T, Relayer)>);

impl<T, Relayer> PaymentProcedure<Relayer, T::Balance> for MintReward<T, Relayer>
where
	T: frame_support::traits::fungible::Mutate<Relayer>,
{
	type Error = sp_runtime::DispatchError;

	fn pay_reward(relayer: &Relayer, reward: T::Balance) -> Result<(), Self::Error> {
		T::mint_into(relayer, reward)
	}
}
