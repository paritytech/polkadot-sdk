// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::ExecuteInstruction;
use crate::{
	config, traits::OnResponse, FeeReason, PalletInfo, QueryResponseInfo, Response, XcmExecutor,
};
use xcm::latest::instructions::*;
use xcm::latest::{Error as XcmError, Xcm};
use frame_support::traits::PalletsInfoAccess;
use xcm::traits::IntoInstruction;

impl<Config: config::Config> ExecuteInstruction<Config> for QueryResponse {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let QueryResponse { query_id, response, max_weight, querier, .. } = self;
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		Config::ResponseHandler::on_response(
			origin,
			query_id,
			querier.as_ref(),
			response,
			max_weight,
			&executor.context,
		);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for QueryPallet {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let QueryPallet { module_name, response_info } = self;
		let pallets = Config::PalletInstancesInfo::infos()
			.into_iter()
			.filter(|x| x.module_name.as_bytes() == &module_name[..])
			.map(|x| {
				PalletInfo::new(
					x.index as u32,
					x.name.as_bytes().into(),
					x.module_name.as_bytes().into(),
					x.crate_version.major as u32,
					x.crate_version.minor as u32,
					x.crate_version.patch as u32,
				)
			})
			.collect::<Result<Vec<_>, XcmError>>()?;
		let QueryResponseInfo { destination, query_id, max_weight } = response_info;
		let response = Response::PalletsInfo(pallets.try_into().map_err(|_| XcmError::Overflow)?);
		let querier = XcmExecutor::<Config>::to_querier(executor.cloned_origin(), &destination)?;
		let instruction = QueryResponse { query_id, response, max_weight, querier }.into_instruction();
		let message = Xcm::new(vec![instruction]);
		executor.send(destination, message, FeeReason::QueryPallet)?;
		Ok(())
	}
}
