//! Runtime types for integrating [`pallet-revive`] with the EVM.
use crate::api::{SignerRecovery, TransactionLegacyUnsigned, TransactionUnsigned};
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::ExtrinsicCall,
	CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use pallet_revive::{BalanceOf, Config, EthInstantiateInput, MomentOf};
use scale_info::TypeInfo;
use sp_core::{ecdsa, ed25519, sr25519, Get, U256};
use sp_runtime::{
	generic::{self, CheckedExtrinsic},
	traits::{
		self, Checkable, Convert, DispatchInfoOf, Dispatchable, Extrinsic, ExtrinsicMetadata, Lazy,
		Member, SignedExtension, Verify,
	},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
	AccountId32, MultiSigner, RuntimeDebug,
};

const LOG_TARGET: &str = "runtime::revive::evm";

/// Some way of identifying an account on the chain.
pub type AccountId = AccountId32;

/// The type for looking up accounts. We don't expect more than 4 billion of them.
pub type AccountIndex = u32;

/// The address format for describing accounts.
pub type MultiAddress = sp_runtime::MultiAddress<AccountId, AccountIndex>;

/// This is [`polkadot_sdk::sp_runtime::MultiSignature`], with an extra Ethereum variant
#[derive(Eq, PartialEq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum MultiSignature {
	/// An Ed25519 signature.
	Ed25519(ed25519::Signature),
	/// An Sr25519 signature.
	Sr25519(sr25519::Signature),
	/// An ECDSA/SECP256k1 signature.
	Ecdsa(ecdsa::Signature),
	/// An Ethereum compatible SECP256k1 signature.
	Ethereum(ecdsa::Signature),
}

impl Verify for MultiSignature {
	type Signer = MultiSigner;
	fn verify<L: Lazy<[u8]>>(&self, mut msg: L, signer: &AccountId32) -> bool {
		let who: [u8; 32] = *signer.as_ref();
		match self {
			Self::Ed25519(sig) => sig.verify(msg, &who.into()),
			Self::Sr25519(sig) => sig.verify(msg, &who.into()),
			Self::Ecdsa(sig) => {
				let m = sp_io::hashing::blake2_256(msg.get());
				sp_io::crypto::secp256k1_ecdsa_recover_compressed(sig.as_ref(), &m)
					.map_or(false, |pubkey| sp_io::hashing::blake2_256(&pubkey) == who)
			},
			Self::Ethereum(_) => false,
		}
	}
}

/// Unchecked extrinsic with support for Ethereum signatures.
/// This wraps  [`generic::UncheckedExtrinsic`] to support Ethereum transactions.
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
				log::debug!(target: LOG_TARGET, "Converting Unchecked extrinsic with Ethereum signature...");
				let eth_msg =
					ConvertEthTx::convert((function.clone(), Extra::get_eth_extra_params(&extra)))?;

				let msg = match eth_msg {
					TransactionUnsigned::TransactionLegacyUnsigned(tx) => tx,
					_ => return Err(InvalidTransaction::Call.into()),
				};

				let signer = msg.recover_signer(&sig).ok_or(InvalidTransaction::BadProof).map_err(
					|err| {
						log::debug!(target: LOG_TARGET, "Failed to recover Ethereum signature: {sig:?}. Error: {err:?}");
						err
					},
				)?;

				log::debug!(target: LOG_TARGET, "Signer recovered: {signer:?}");
				let account_id =
					lookup.lookup(MultiAddress::Address20(signer.into())).map_err(|err| {
						log::debug!(target: LOG_TARGET, "Failed to lookup account: {err:?}");
						err
					})?;

				log::debug!(target: LOG_TARGET, "Signer address20 is: {account_id:?}");
				let expected_account_id = lookup.lookup(addr)?;

				if account_id != expected_account_id {
					log::debug!(target: LOG_TARGET, "Account ID should be: {expected_account_id:?}");
					return Err(InvalidTransaction::BadProof.into());
				}

				log::debug!(target: LOG_TARGET, "Valid ethereum message");
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

/// A [`SignedExtension`] that adds the ethereum gas price and gas limit to the transaction.
/// These parameters are used to reconstruct the Ethereum transaction and verify the signature.
/// We also validate that the weight injected by the RPC and not part of the signed Ethereum
/// transaction is aligned with the gas limit.
#[derive(Encode, Decode, DebugNoBound, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckEvmGas<T: Config> {
	/// The Ethereum gas price, specified by the user.
	pub eth_gas_price: BalanceOf<T>,
	/// The Ethereum gas limit, specified by the user.
	pub eth_gas_limit: u64,
}

impl<T: Config> SignedExtension for CheckEvmGas<T>
where
	<T as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	type AccountId = <T as frame_system::Config>::AccountId;
	type Call = <T as frame_system::Config>::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();
	const IDENTIFIER: &'static str = "CheckEvmGas";

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		Ok(())
	}

	fn validate(
		&self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		// TODO make sure that the fees computed with the gas limit and gas price are roughly equal
		// to the fees computed with the weight and the weight price.
		// see substrate/frame/transaction-payment/src/lib.rs
		Ok(ValidTransaction::default())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.validate(who, call, info, len).map(|_| ())
	}
}

/// An implementation of the [`Convert`] trait, used to extract the original [`TransactionUnsigned`]
/// Ethereum transaction from  a call.
#[derive(CloneNoBound, PartialEqNoBound, EqNoBound, DebugNoBound)]
pub struct ConvertEthTx<T: Config>(core::marker::PhantomData<T>);

/// Parameters that are extracted from [`SignedExtension`] and used to reconstruct the Ethereum
/// transaction, as it was signed by the user.
pub struct EthExtraParams {
	/// The transaction nonce.
	pub nonce: U256,
	/// The ethereum gas price, specified by the user.
	pub gas_price: U256,
	/// The ethereum gas limit, specified by the user.
	pub gas_limit: U256,
}

/// EthSignedExtension provides the extra parameters that are required to reconstruct the Ethereum
pub trait EthSignedExtension {
	/// The [`SignedExtension`] used by the runtime.
	type Extension: SignedExtension;
	/// Get the Ethereum transaction nonce, gas price and gas limit
	fn get_eth_extra_params(extra: &Self::Extension) -> EthExtraParams;
}

impl<T, Call> Convert<(Call, EthExtraParams), Result<TransactionUnsigned, InvalidTransaction>>
	for ConvertEthTx<T>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	Call: TryInto<pallet_revive::Call<T>>,
{
	fn convert(
		(call, extra): (Call, EthExtraParams),
	) -> Result<TransactionUnsigned, InvalidTransaction> {
		match call.try_into().map_err(|_| InvalidTransaction::Call)? {
			pallet_revive::Call::instantiate_with_code {
				value,
				gas_limit: _,
				storage_deposit_limit: _,
				code,
				data,
				salt,
			} => {
				let chain_id = T::ChainId::get();
				let tx = TransactionLegacyUnsigned::from_instantiate(
					EthInstantiateInput { code, data, salt },
					value.into(),
					extra.gas_price,
					extra.gas_limit,
					extra.nonce,
					chain_id.into(),
				);
				Ok(TransactionUnsigned::TransactionLegacyUnsigned(tx))
			},
			pallet_revive::Call::call {
				dest,
				value,
				gas_limit: _,
				storage_deposit_limit: _,
				data,
			} => {
				let chain_id = T::ChainId::get();
				let tx = TransactionLegacyUnsigned::from_call(
					dest,
					data,
					value.into(),
					extra.gas_price,
					extra.gas_limit,
					extra.nonce,
					chain_id.into(),
				);
				Ok(TransactionUnsigned::TransactionLegacyUnsigned(tx))
			},
			_ => Err(InvalidTransaction::Call),
		}
	}
}
