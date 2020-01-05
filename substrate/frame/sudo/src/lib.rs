// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! # Sudo Module
//!
//! - [`sudo::Trait`](./trait.Trait.html)
//! - [`Call`](./enum.Call.html)
//!
//! ## Overview
//!
//! The Sudo module allows for a single account (called the "sudo key")
//! to execute dispatchable functions that require a `Root` call
//! or designate a new account to replace them as the sudo key.
//! Only one account can be the sudo key at a time.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! Only the sudo key can call the dispatchable functions from the Sudo module.
//!
//! * `sudo` - Make a `Root` call to a dispatchable function.
//! * `set_key` - Assign a new account to be the sudo key.
//!
//! ## Usage
//!
//! ### Executing Privileged Functions
//!
//! The Sudo module itself is not intended to be used within other modules.
//! Instead, you can build "privileged functions" (i.e. functions that require `Root` origin) in other modules.
//! You can execute these privileged functions by calling `sudo` with the sudo key account.
//! Privileged functions cannot be directly executed via an extrinsic.
//!
//! Learn more about privileged functions and `Root` origin in the [`Origin`] type documentation.
//!
//! ### Simple Code Snippet
//!
//! This is an example of a module that exposes a privileged function:
//!
//! ```
//! use frame_support::{decl_module, dispatch};
//! use frame_system::{self as system, ensure_root};
//!
//! pub trait Trait: frame_system::Trait {}
//!
//! decl_module! {
//!     pub struct Module<T: Trait> for enum Call where origin: T::Origin {
//!         pub fn privileged_function(origin) -> dispatch::DispatchResult {
//!             ensure_root(origin)?;
//!
//!             // do something...
//!
//!             Ok(())
//!         }
//!     }
//! }
//! # fn main() {}
//! ```
//!
//! ## Genesis Config
//!
//! The Sudo module depends on the [`GenesisConfig`](./struct.GenesisConfig.html).
//! You need to set an initial superuser account as the sudo `key`.
//!
//! ## Related Modules
//!
//! * [Consensus](../frame_consensus/index.html)
//! * [Democracy](../pallet_democracy/index.html)
//!
//! [`Call`]: ./enum.Call.html
//! [`Trait`]: ./trait.Trait.html
//! [`Origin`]: https://docs.substrate.dev/docs/substrate-types

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::prelude::*;
use sp_runtime::{traits::{StaticLookup, Dispatchable}, DispatchError};

use frame_support::{
	Parameter, decl_module, decl_event, decl_storage, decl_error, ensure,
	weights::SimpleDispatchInfo,
};
use frame_system::{self as system, ensure_signed};

pub trait Trait: frame_system::Trait {
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

	/// A sudo-able call.
	type Proposal: Parameter + Dispatchable<Origin=Self::Origin>;
}

decl_module! {
	// Simple declaration of the `Module` type. Lets the macro know what it's working on.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		fn deposit_event() = default;

		/// Authenticates the sudo key and dispatches a function call with `Root` origin.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// # <weight>
		/// - O(1).
		/// - Limited storage reads.
		/// - One DB write (event).
		/// - Unknown weight of derivative `proposal` execution.
		/// # </weight>
		#[weight = SimpleDispatchInfo::FreeOperational]
		fn sudo(origin, proposal: Box<T::Proposal>) {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			ensure!(sender == Self::key(), Error::<T>::RequireSudo);

			let res = match proposal.dispatch(frame_system::RawOrigin::Root.into()) {
				Ok(_) => true,
				Err(e) => {
					let e: DispatchError = e.into();
					sp_runtime::print(e);
					false
				}
			};

			Self::deposit_event(RawEvent::Sudid(res));
		}

		/// Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo key.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// # <weight>
		/// - O(1).
		/// - Limited storage reads.
		/// - One DB change.
		/// # </weight>
		fn set_key(origin, new: <T::Lookup as StaticLookup>::Source) {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			ensure!(sender == Self::key(), Error::<T>::RequireSudo);
			let new = T::Lookup::lookup(new)?;

			Self::deposit_event(RawEvent::KeyChanged(Self::key()));
			<Key<T>>::put(new);
		}

		/// Authenticates the sudo key and dispatches a function call with `Signed` origin from
		/// a given account.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// # <weight>
		/// - O(1).
		/// - Limited storage reads.
		/// - One DB write (event).
		/// - Unknown weight of derivative `proposal` execution.
		/// # </weight>
		#[weight = SimpleDispatchInfo::FixedOperational(0)]
		fn sudo_as(origin, who: <T::Lookup as StaticLookup>::Source, proposal: Box<T::Proposal>) {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			ensure!(sender == Self::key(), Error::<T>::RequireSudo);

			let who = T::Lookup::lookup(who)?;

			let res = match proposal.dispatch(frame_system::RawOrigin::Signed(who).into()) {
				Ok(_) => true,
				Err(e) => {
					let e: DispatchError = e.into();
					sp_runtime::print(e);
					false
				}
			};

			Self::deposit_event(RawEvent::SudoAsDone(res));
		}
	}
}

decl_event!(
	pub enum Event<T> where AccountId = <T as frame_system::Trait>::AccountId {
		/// A sudo just took place.
		Sudid(bool),
		/// The sudoer just switched identity; the old key is supplied.
		KeyChanged(AccountId),
		/// A sudo just took place.
		SudoAsDone(bool),
	}
);

decl_storage! {
	trait Store for Module<T: Trait> as Sudo {
		/// The `AccountId` of the sudo key.
		Key get(fn key) config(): T::AccountId;
	}
}

decl_error! {
	/// Error for the Sudo module
	pub enum Error for Module<T: Trait> {
		/// Sender must be the Sudo account
		RequireSudo,
	}
}
