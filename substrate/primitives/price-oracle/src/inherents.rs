// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Inherent of the price oracle: the signed reports a block author includes in a block.

use crate::SignedPriceReport;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use sp_inherents::{InherentData, InherentIdentifier};

/// Identifier of the price oracle inherent.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"poracle0";

/// The inherent data: the signed reports a block author includes in a block.
pub type PriceOracleInherentData<Id, Signature> = Vec<SignedPriceReport<Id, Signature>>;

/// Access to the price oracle inherent data.
pub trait PriceOracleInherentDataExt<Id, Signature> {
	/// The reports included in the inherent data, if any.
	fn price_reports(
		&self,
	) -> Result<Option<PriceOracleInherentData<Id, Signature>>, sp_inherents::Error>;
}

impl<Id: Decode, Signature: Decode> PriceOracleInherentDataExt<Id, Signature> for InherentData {
	fn price_reports(
		&self,
	) -> Result<Option<PriceOracleInherentData<Id, Signature>>, sp_inherents::Error> {
		self.get_data(&INHERENT_IDENTIFIER)
	}
}

/// Node side provider of the price oracle inherent data.
#[cfg(feature = "std")]
pub struct InherentDataProvider<Id, Signature> {
	reports: PriceOracleInherentData<Id, Signature>,
}

#[cfg(feature = "std")]
impl<Id, Signature> InherentDataProvider<Id, Signature> {
	/// Provide the given reports.
	pub fn new(reports: PriceOracleInherentData<Id, Signature>) -> Self {
		Self { reports }
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl<Id, Signature> sp_inherents::InherentDataProvider for InherentDataProvider<Id, Signature>
where
	Id: Encode + Send + Sync,
	Signature: Encode + Send + Sync,
{
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(INHERENT_IDENTIFIER, &self.reports)
	}

	async fn try_handle_error(
		&self,
		_identifier: &InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		// The price oracle inherent never causes a block to be rejected.
		None
	}
}
