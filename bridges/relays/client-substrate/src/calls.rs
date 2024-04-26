// Copyright 2019-2023 Parity Technologies (UK) Ltd.
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

//! Basic runtime calls.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec::Vec};

use xcm::{VersionedLocation, VersionedXcm};

/// A minimized version of `frame-system::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum SystemCall {
	/// `frame-system::Call::remark`
	#[codec(index = 1)]
	remark(Vec<u8>),
}

/// A minimized version of `pallet-utility::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum UtilityCall<Call> {
	/// `pallet-utility::Call::batch_all`
	#[codec(index = 2)]
	batch_all(Vec<Call>),
}

/// A minimized version of `pallet-sudo::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum SudoCall<Call> {
	/// `pallet-sudo::Call::sudo`
	#[codec(index = 0)]
	sudo(Box<Call>),
}

/// A minimized version of `pallet-xcm::Call`, that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum XcmCall {
	/// `pallet-xcm::Call::send`
	#[codec(index = 0)]
	send(Box<VersionedLocation>, Box<VersionedXcm<()>>),
}
