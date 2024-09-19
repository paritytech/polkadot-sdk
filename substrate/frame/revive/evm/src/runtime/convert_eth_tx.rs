//! TODO document
#![allow(unused)]
use crate::api::{TransactionLegacyUnsigned, TransactionUnsigned};
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use pallet_revive::{BalanceOf, Call, Config, EthInstantiateInput, MomentOf};
use sp_core::{Get, H160, U256};
use sp_runtime::{
	traits::{Convert, SignedExtension},
	transaction_validity::InvalidTransaction,
};

/// An implementation of the [`Convert`] trait, used to extract an [`TransactionUnsigned`] Ethereum
/// transaction and a source [`H160`] address from an extrinsic.
/// This is used to check that an UncheckedExtrinsic that carry an Ethereum signature is valid.
#[derive(CloneNoBound, PartialEqNoBound, EqNoBound, DebugNoBound)]
pub struct ConvertEthTx<T: Config>(core::marker::PhantomData<T>);

/// TODO document
pub struct EthExtraParams {
	/// TODO
	pub nonce: U256,
	/// TODO
	pub gas_price: U256,
	/// TODO
	pub gas_limit: U256,
}

/// TODO
pub trait EthSignedExtension {
	/// TODO
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
				log::debug!("convert instantiate_with_code");
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
				log::debug!("convert call");
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
