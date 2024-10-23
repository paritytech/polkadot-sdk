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

//! The [DispatchTransaction] trait.

use crate::{traits::AsTransactionAuthorizedOrigin, transaction_validity::InvalidTransaction};

use super::*;

/// Single-function utility trait with a blanket impl over [`TransactionExtension`] in order to
/// provide transaction dispatching functionality. We avoid implementing this directly on the trait
/// since we never want it to be overriden by the trait implementation.
pub trait DispatchTransaction<Call: Dispatchable> {
	/// The origin type of the transaction.
	type Origin;
	/// The info type.
	type Info;
	/// The resultant type.
	type Result;
	/// The `Val` of the extension.
	type Val;
	/// The `Pre` of the extension.
	type Pre;
	/// Just validate a transaction.
	///
	/// The is basically the same as [validate](TransactionExtension::validate), except that there
	/// is no need to supply the bond data.
	fn validate_only(
		&self,
		origin: Self::Origin,
		call: &Call,
		info: &Self::Info,
		len: usize,
	) -> Result<(ValidTransaction, Self::Val, Self::Origin), TransactionValidityError>;
	/// Validate and prepare a transaction, ready for dispatch.
	fn validate_and_prepare(
		self,
		origin: Self::Origin,
		call: &Call,
		info: &Self::Info,
		len: usize,
	) -> Result<(Self::Pre, Self::Origin), TransactionValidityError>;
	/// Dispatch a transaction with the given base origin and call.
	fn dispatch_transaction(
		self,
		origin: Self::Origin,
		call: Call,
		info: &Self::Info,
		len: usize,
	) -> Self::Result;
	/// Do everything which would be done in a [dispatch_transaction](Self::dispatch_transaction),
	/// but instead of executing the call, execute `substitute` instead. Since this doesn't actually
	/// dispatch the call, it doesn't need to consume it and so `call` can be passed as a reference.
	fn test_run(
		self,
		origin: Self::Origin,
		call: &Call,
		info: &Self::Info,
		len: usize,
		substitute: impl FnOnce(
			Self::Origin,
		) -> crate::DispatchResultWithInfo<<Call as Dispatchable>::PostInfo>,
	) -> Self::Result;
}

impl<T: TransactionExtension<Call>, Call: Dispatchable + Encode> DispatchTransaction<Call> for T
where
	<Call as Dispatchable>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
{
	type Origin = <Call as Dispatchable>::RuntimeOrigin;
	type Info = DispatchInfoOf<Call>;
	type Result = crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Call>>;
	type Val = T::Val;
	type Pre = T::Pre;

	fn validate_only(
		&self,
		origin: Self::Origin,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> Result<(ValidTransaction, T::Val, Self::Origin), TransactionValidityError> {
		match self.validate(origin, call, info, len, self.implicit()?, call) {
			// After validation, some origin must have been authorized.
			Ok((_, _, origin)) if !origin.is_transaction_authorized() =>
				Err(InvalidTransaction::UnknownOrigin.into()),
			res => res,
		}
	}
	fn validate_and_prepare(
		self,
		origin: Self::Origin,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> Result<(T::Pre, Self::Origin), TransactionValidityError> {
		let (_, val, origin) = self.validate_only(origin, call, info, len)?;
		let pre = self.prepare(val, &origin, &call, info, len)?;
		Ok((pre, origin))
	}
	fn dispatch_transaction(
		self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		call: Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> Self::Result {
		let (pre, origin) = self.validate_and_prepare(origin, &call, info, len)?;
		let mut res = call.dispatch(origin);
		let pd_res = res.map(|_| ()).map_err(|e| e.error);
		let post_info = match &mut res {
			Ok(info) => info,
			Err(err) => &mut err.post_info,
		};
		post_info.set_extension_weight(info);
		T::post_dispatch(pre, info, post_info, len, &pd_res)?;
		Ok(res)
	}
	fn test_run(
		self,
		origin: Self::Origin,
		call: &Call,
		info: &Self::Info,
		len: usize,
		substitute: impl FnOnce(
			Self::Origin,
		) -> crate::DispatchResultWithInfo<<Call as Dispatchable>::PostInfo>,
	) -> Self::Result {
		let (pre, origin) = self.validate_and_prepare(origin, &call, info, len)?;
		let mut res = substitute(origin);
		let pd_res = res.map(|_| ()).map_err(|e| e.error);
		let post_info = match &mut res {
			Ok(info) => info,
			Err(err) => &mut err.post_info,
		};
		post_info.set_extension_weight(info);
		T::post_dispatch(pre, info, post_info, len, &pd_res)?;
		Ok(res)
	}
}
