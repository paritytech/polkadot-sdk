//! TODO: Add a description
use super::{signature::MultiSignature, EthExtraParams, EthSignedExtension};
use crate::api::{SignerRecovery, TransactionUnsigned};
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::ExtrinsicCall,
};
use scale_info::TypeInfo;
use sp_runtime::{
	generic::{self, CheckedExtrinsic},
	traits::{self, Checkable, Convert, Extrinsic, ExtrinsicMetadata, Member, SignedExtension},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	AccountId32, RuntimeDebug,
};

/// Some way of identifying an account on the chain.
pub type AccountId = AccountId32;

/// The type for looking up accounts. We don't expect more than 4 billion of them.
pub type AccountIndex = u32;

/// The address format for describing accounts.
pub type MultiAddress = sp_runtime::MultiAddress<AccountId, AccountIndex>;

/// Unchecked extrinsic with support for Ethereum signatures.
/// This is a wrapper on top of [`generic::UncheckedExtrinsic`] to support Ethereum
/// transactions.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(ConvertEthTx))]
pub struct UncheckedExtrinsic<Call, Extra: EthSignedExtension, ConvertEthTx>(
	pub generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, Extra::Extension>,
	PhantomData<ConvertEthTx>,
);

impl<Call: TypeInfo, Extra: EthSignedExtension, ConvertEthTx> Extrinsic
	for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
{
	type Call = Call;

	type SignaturePayload = (MultiAddress, MultiSignature, Extra::Extension);

	fn is_signed(&self) -> Option<bool> {
		self.0.is_signed()
	}

	fn new(function: Call, signed_data: Option<Self::SignaturePayload>) -> Option<Self> {
		Some(if let Some((address, signature, extra)) = signed_data {
			Self(
				generic::UncheckedExtrinsic::new_signed(function, address, signature, extra),
				PhantomData,
			)
		} else {
			Self(generic::UncheckedExtrinsic::new_unsigned(function), PhantomData)
		})
	}
}

impl<Call, Extra: EthSignedExtension, ConvertEthTx> ExtrinsicMetadata
	for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
{
	const VERSION: u8 = generic::UncheckedExtrinsic::<
		MultiAddress,
		Call,
		MultiSignature,
		Extra::Extension,
	>::VERSION;
	type SignedExtensions = Extra::Extension;
}

impl<Call: TypeInfo, Extra: EthSignedExtension, ConvertEthTx> ExtrinsicCall
	for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
{
	fn call(&self) -> &Self::Call {
		self.0.call()
	}
}

impl<Call, Extra, ConvertEthTx, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
where
	Call: Encode + Member,
	Extra: EthSignedExtension,
	Extra::Extension: SignedExtension<AccountId = AccountId32>,
	ConvertEthTx: Convert<(Call, EthExtraParams), Result<TransactionUnsigned, InvalidTransaction>>,
	Lookup: traits::Lookup<Source = MultiAddress, Target = AccountId32>,
{
	type Checked = CheckedExtrinsic<AccountId32, Call, Extra::Extension>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		let function = self.0.function.clone();

		match self.0.signature {
			Some((addr, MultiSignature::Ethereum(sig), extra)) => {
				log::trace!(target: "evm", "Checking extrinsic with  ethereum signature...");
				let eth_msg =
					ConvertEthTx::convert((function.clone(), Extra::get_eth_extra_params(&extra)))?;

				let msg = match eth_msg {
					TransactionUnsigned::TransactionLegacyUnsigned(tx) => tx,
					_ => return Err(InvalidTransaction::Call.into()),
				};
				log::trace!(target: "evm", "Received ethereum transaction: {msg:#?}");

				let signer = msg.recover_signer(&sig).ok_or(InvalidTransaction::BadProof).map_err(
					|err| {
						log::trace!(target: "evm", "Failed to recover from sig: {sig:?}. Error: {err:?}");
						err
					},
				)?;

				log::trace!(target: "evm", "Signer recovered: {signer:?}");
				let account_id =
					lookup.lookup(MultiAddress::Address20(signer.into())).map_err(|err| {
						log::trace!(target: "evm", "Failed to lookup account: {err:?}");
						err
					})?;

				log::trace!(target: "evm", "Signer address20 is: {account_id:?}");
				let expected_account_id = lookup.lookup(addr)?;
				if account_id != expected_account_id {
					log::trace!(target: "evm", "Account ID should be: {expected_account_id:?}");
					return Err(InvalidTransaction::BadProof.into());
				}

				log::trace!(target: "evm", "Valid ethereum message");
				Ok(CheckedExtrinsic { signed: Some((account_id, extra)), function })
			},
			_ => self.0.check(lookup),
		}
	}
}

impl<Call, Extra, ConvertEthTx> GetDispatchInfo for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
where
	Call: GetDispatchInfo,
	Extra: EthSignedExtension,
{
	fn get_dispatch_info(&self) -> DispatchInfo {
		self.0.get_dispatch_info()
	}
}

impl<Call: Encode, Extra: EthSignedExtension, ConvertEthTx> serde::Serialize
	for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.0.serialize(seq)
	}
}

impl<'a, Call: Decode, Extra: EthSignedExtension, ConvertEthTx> serde::Deserialize<'a>
	for UncheckedExtrinsic<Call, Extra, ConvertEthTx>
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		let r = sp_core::bytes::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| serde::de::Error::custom(sp_runtime::format!("Decode error: {}", e)))
	}
}
