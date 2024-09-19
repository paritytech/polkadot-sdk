#![allow(missing_docs, unused, unused_imports)]
//! TODO document
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchInfo, DebugNoBound};
use pallet_revive::{BalanceOf, Config};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_runtime::{
    traits::{DispatchInfoOf, Dispatchable, SignedExtension},
    transaction_validity::TransactionValidityError,
};

/// TODO document
#[derive(Encode, Decode, DebugNoBound, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckEvmGas<T: Config> {
    pub eth_gas_price: BalanceOf<T>,
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
