// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Primitives that may be used for creating signed extensions for indirect runtimes.

use codec::{Compact, Decode, Encode};
use impl_trait_for_tuples::impl_for_tuples;
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{DispatchInfoOf, SignedExtension},
	transaction_validity::TransactionValidityError,
};
use sp_std::{fmt::Debug, marker::PhantomData};

/// Trait that describes some properties of a `SignedExtension` that are needed in order to send a
/// transaction to the chain.
pub trait SignedExtensionSchema: Encode + Decode + Debug + Eq + Clone + StaticTypeInfo {
	/// A type of the data encoded as part of the transaction.
	type Payload: Encode + Decode + Debug + Eq + Clone + StaticTypeInfo;
	/// Parameters which are part of the payload used to produce transaction signature,
	/// but don't end up in the transaction itself (i.e. inherent part of the runtime).
	type AdditionalSigned: Encode + Debug + Eq + Clone + StaticTypeInfo;
}

impl SignedExtensionSchema for () {
	type Payload = ();
	type AdditionalSigned = ();
}

/// An implementation of `SignedExtensionSchema` using generic params.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, TypeInfo)]
pub struct GenericSignedExtensionSchema<P, S>(PhantomData<(P, S)>);

impl<P, S> SignedExtensionSchema for GenericSignedExtensionSchema<P, S>
where
	P: Encode + Decode + Debug + Eq + Clone + StaticTypeInfo,
	S: Encode + Debug + Eq + Clone + StaticTypeInfo,
{
	type Payload = P;
	type AdditionalSigned = S;
}

/// The `SignedExtensionSchema` for `frame_system::CheckNonZeroSender`.
pub type CheckNonZeroSender = GenericSignedExtensionSchema<(), ()>;

/// The `SignedExtensionSchema` for `frame_system::CheckSpecVersion`.
pub type CheckSpecVersion = GenericSignedExtensionSchema<(), u32>;

/// The `SignedExtensionSchema` for `frame_system::CheckTxVersion`.
pub type CheckTxVersion = GenericSignedExtensionSchema<(), u32>;

/// The `SignedExtensionSchema` for `frame_system::CheckGenesis`.
pub type CheckGenesis<Hash> = GenericSignedExtensionSchema<(), Hash>;

/// The `SignedExtensionSchema` for `frame_system::CheckEra`.
pub type CheckEra<Hash> = GenericSignedExtensionSchema<sp_runtime::generic::Era, Hash>;

/// The `SignedExtensionSchema` for `frame_system::CheckNonce`.
pub type CheckNonce<TxNonce> = GenericSignedExtensionSchema<Compact<TxNonce>, ()>;

/// The `SignedExtensionSchema` for `frame_system::CheckWeight`.
pub type CheckWeight = GenericSignedExtensionSchema<(), ()>;

/// The `SignedExtensionSchema` for `pallet_transaction_payment::ChargeTransactionPayment`.
pub type ChargeTransactionPayment<Balance> = GenericSignedExtensionSchema<Compact<Balance>, ()>;

/// The `SignedExtensionSchema` for `polkadot-runtime-common::PrevalidateAttests`.
pub type PrevalidateAttests = GenericSignedExtensionSchema<(), ()>;

/// The `SignedExtensionSchema` for `BridgeRejectObsoleteHeadersAndMessages`.
pub type BridgeRejectObsoleteHeadersAndMessages = GenericSignedExtensionSchema<(), ()>;

/// The `SignedExtensionSchema` for `RefundBridgedParachainMessages`.
/// This schema is dedicated for `RefundBridgedParachainMessages` signed extension as
/// wildcard/placeholder, which relies on the scale encoding for `()` or `((), ())`, or `((), (),
/// ())` is the same. So runtime can contains any kind of tuple:
/// `(BridgeRefundBridgeHubRococoMessages)`
/// `(BridgeRefundBridgeHubRococoMessages, BridgeRefundBridgeHubWestendMessages)`
/// `(BridgeRefundParachainMessages1, ..., BridgeRefundParachainMessagesN)`
pub type RefundBridgedParachainMessagesSchema = GenericSignedExtensionSchema<(), ()>;

#[impl_for_tuples(1, 12)]
impl SignedExtensionSchema for Tuple {
	for_tuples!( type Payload = ( #( Tuple::Payload ),* ); );
	for_tuples!( type AdditionalSigned = ( #( Tuple::AdditionalSigned ),* ); );
}

/// A simplified version of signed extensions meant for producing signed transactions
/// and signed payloads in the client code.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub struct GenericSignedExtension<S: SignedExtensionSchema> {
	/// A payload that is included in the transaction.
	pub payload: S::Payload,
	#[codec(skip)]
	// It may be set to `None` if extensions are decoded. We are never reconstructing transactions
	// (and it makes no sense to do that) => decoded version of `SignedExtensions` is only used to
	// read fields of the `payload`. And when resigning transaction, we're reconstructing
	// `SignedExtensions` from scratch.
	additional_signed: Option<S::AdditionalSigned>,
}

impl<S: SignedExtensionSchema> GenericSignedExtension<S> {
	/// Create new `GenericSignedExtension` object.
	pub fn new(payload: S::Payload, additional_signed: Option<S::AdditionalSigned>) -> Self {
		Self { payload, additional_signed }
	}
}

impl<S> SignedExtension for GenericSignedExtension<S>
where
	S: SignedExtensionSchema,
	S::Payload: Send + Sync,
	S::AdditionalSigned: Send + Sync,
{
	const IDENTIFIER: &'static str = "Not needed.";
	type AccountId = ();
	type Call = ();
	type AdditionalSigned = S::AdditionalSigned;
	type Pre = ();

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		// we shall not ever see this error in relay, because we are never signing decoded
		// transactions. Instead we're constructing and signing new transactions. So the error code
		// is kinda random here
		self.additional_signed.clone().ok_or(
			frame_support::unsigned::TransactionValidityError::Unknown(
				frame_support::unsigned::UnknownTransaction::Custom(0xFF),
			),
		)
	}

	fn pre_dispatch(
		self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}
}
