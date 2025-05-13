use alloy_core::{
	primitives::{Address, U256 as EU256},
	sol_types::*,
};
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::Zero,
	traits::{fungible::Inspect, OriginTrait},
};
use pallet_revive::{AddressMapper, ContractResult, DepositLimit, MomentOf, erc20::IERC20};
use sp_core::{Get, H160, H256, U256};
use sp_runtime::Weight;
use xcm::latest::prelude::*;
use xcm_executor::{
	traits::{ConvertLocation, Error as MatchError, MatchesFungibles, TransactAsset},
	AssetsInHolding,
};

type BalanceOf<T> = <<T as pallet_revive::Config>::Currency as Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

pub struct ERC20Transactor<T, Matcher, AccountIdConverter, GasLimit, AccountId, CheckingAccount>(
	PhantomData<(T, Matcher, AccountIdConverter, GasLimit, AccountId, CheckingAccount)>,
);

impl<
		AccountId: Eq + Clone,
		T: pallet_revive::Config<AccountId = AccountId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		Matcher: MatchesFungibles<H160, u128>,
		GasLimit: Get<Weight>,
		CheckingAccount: Get<AccountId>,
	> TransactAsset for ERC20Transactor<T, Matcher, AccountIdConverter, GasLimit, AccountId, CheckingAccount>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	fn can_check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// TODO.
		Ok(())
	}

	fn check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) {
		// TODO.
	}

	fn can_check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// TODO.
		Ok(())
	}

	fn check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) {
		// TODO.
	}

	fn withdraw_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(AssetsInHolding, Weight), XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc20::withdraw",
			?what, ?who,
		);
		let (asset_id, amount) = Matcher::matches_fungibles(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let checking_account_eth = T::AddressMapper::to_address(&CheckingAccount::get());
		let checking_address = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
		let gas_limit = GasLimit::get();
		let data = IERC20::transferCall { to: checking_address, value: EU256::from(amount) }.abi_encode();
		let ContractResult { result, gas_consumed, .. } = pallet_revive::Pallet::<T>::bare_call(
			T::RuntimeOrigin::signed(who.clone()),
			asset_id,
			BalanceOf::<T>::zero(),
			gas_limit,
			DepositLimit::Unchecked,
			data,
		);
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = gas_limit.saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?gas_consumed, ?surplus, "Gas consumed");
		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?return_value, "Return value by withdraw_asset");
			if return_value.did_revert() {
				tracing::debug!(target: "xcm::transactor::erc20::withdraw", "ERC20 contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC20 contract reverted"))
			} else {
				let is_success = bool::abi_decode(&return_value.data, false).map_err(|error| {
					tracing::debug!(target: "xcm::transactor::erc20::withdraw", ?error, "ERC20 contract result couldn't decode");
					XcmError::FailedToTransactAsset("ERC20 contract result couldn't decode")
				})?;
				if is_success {
					Ok((what.clone().into(), surplus))
				} else {
					tracing::debug!(target: "xcm::transactor::erc20::withdraw", "contract transfer failed");
					Err(XcmError::FailedToTransactAsset("ERC20 contract transfer failed"))
				}
			}
		} else {
			tracing::debug!(target: "xcm::transactor::erc20::withdraw", ?result, "Error");
			// This error could've been duplicate smart contract, out of gas, etc.
			// If the issue is gas, there's nothing the user can change in the XCM
			// that will make this work since there's a hardcoded gas limit.
			Err(XcmError::FailedToTransactAsset("ERC20 contract execution errored"))
		}
	}

	fn deposit_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<Weight, XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc20::deposit",
			?what, ?who,
		);
		let (asset_id, amount) = Matcher::matches_fungibles(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let eth_address = T::AddressMapper::to_address(&who);
		let address = Address::from(Into::<[u8; 20]>::into(eth_address));
		let data = IERC20::transferCall { to: address, value: EU256::from(amount) }.abi_encode();
		let gas_limit = GasLimit::get();
		let ContractResult { result, gas_consumed, .. } = pallet_revive::Pallet::<T>::bare_call(
			T::RuntimeOrigin::signed(CheckingAccount::get()),
			asset_id,
			BalanceOf::<T>::zero(),
			gas_limit,
			DepositLimit::Unchecked,
			data,
		);
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = gas_limit.saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20::deposit", ?gas_consumed, ?surplus, "Gas consumed");
		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc20::deposit", ?return_value, "Return value");
			if return_value.did_revert() {
				tracing::debug!(target: "xcm::transactor::erc20::deposit", "Contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC20 contract reverted"))
			} else {
				let is_success = bool::abi_decode(&return_value.data, false).map_err(|error| {
					tracing::debug!(target: "xcm::transactor::erc20::deposit", ?error, "ERC20 contract result couldn't decode");
					XcmError::FailedToTransactAsset("ERC20 contract result couldn't decode")
				})?;
				if is_success {
					Ok(surplus)
				} else {
					tracing::debug!(target: "xcm::transactor::erc20::deposit", "contract transfer failed");
					Err(XcmError::FailedToTransactAsset("ERC20 contract transfer failed"))
				}
			}
		} else {
			tracing::debug!(target: "xcm::transactor::erc20::deposit", ?result, "Error");
			// This error could've been duplicate smart contract, out of gas, etc.
			// If the issue is gas, there's nothing the user can change in the XCM
			// that will make this work since there's a hardcoded gas limit.
			Err(XcmError::FailedToTransactAsset("ERC20 contract execution errored"))
		}
	}
}
