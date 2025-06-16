use codec::{Decode, Encode};
use serde::Deserialize;
use sp_runtime::{
    traits::Block as BlockT,
    transaction_validity::TransactionPriority,
};
use sp_core::H160;

/// Provides transaction details required by `TransactionPriorityModifier`.
pub trait TransactionDetailProvider: Send + Sync {
    type Block: BlockT;
    fn get_transaction_detail(&self, tx: &<Self::Block as BlockT>::Extrinsic) -> Option<TransactionDetail>;
}

/// Implemented for the `Client`, to get tx priorities from the transaction pool.
pub trait TransactionPriorityModifierT {
    type Block: BlockT;
    fn get_priority(&self, tx: &<Self::Block as BlockT>::Extrinsic) -> Option<TransactionPriority>;
}

/// Main struct for getting TX priority from the specified list.
pub struct TransactionPriorityModifier<Block> {
    priority_list: Vec<TransactionPriorityItem>,
    // dynamic dispatch here. This can be changed to use a generic type, but this would require
    // way more changes in the existing code.
    pub tx_detail_provider: Box<dyn TransactionDetailProvider<Block = Block>>,
}

impl<Block: BlockT> TransactionPriorityModifier<Block> {
    /// Parameters:
    ///  `json_data`: JSON string with `Vec<TransactionPriorityItem>`.
    ///     Can be set to `None` if no transaction priorities should be overwritten.
    ///  `tx_detail_provider`: Type providing `TransactionDetail`.
    pub fn new(json_data: Option<&'static str>, tx_detail_provider: Box<dyn TransactionDetailProvider<Block = Block>>) -> Self {
        let data = if let Some(data_str) = json_data {
            // it's up to the implementors to make sure that the list doesn't contain duplicate entries.
            serde_json::from_str(&data_str).unwrap()
        } else {
            Vec::new()
        };

        Self { priority_list: data,
            tx_detail_provider
        }
    }

    pub fn get_priority(&self, tx: &<Block as BlockT>::Extrinsic) -> Option<TransactionPriority> {
        let Some(tx_detail) = self.tx_detail_provider.get_transaction_detail(tx) else {
            return None;
        };

        let priority_item = self.priority_list.iter().find(|item| item.module == tx_detail.module && item.extrinsic == tx_detail.extrinsic);
        let Some(item) = priority_item else { return None };

        if let Some(TransactionTypeDetail::Evm(item_tx_data)) = &item.transaction_data {
            let Some(TransactionTypeDetail::Evm(tx_data)) = tx_detail.transaction_data else {
                // signer is present in the list, but not in the TX we received
                return None
            };
            if item_tx_data.signer != tx_data.signer {
                return None
            }

            if let Some(item_call_address) = item_tx_data.call_address {
                let Some(tx_call_address) = tx_data.call_address else {
                    // call_address is present in the list, but not in the TX we received
                    return None
                };
                if item_call_address != tx_call_address {
                    return None
                }
            }

            return Some(item.priority)
        }

        Some(item.priority)
    }
}

/// Transaction details specific for an EVM transaction
#[derive(Clone, Encode, Decode, Deserialize, Debug)]
pub struct EvmTransactionDetail {
    pub call_address: Option<H160>,
    pub signer: H160,
}

#[derive(Clone, Encode, Decode, Deserialize, Debug)]
pub enum TransactionTypeDetail {
    Evm(EvmTransactionDetail)
}

/// Transaction details used to determine transaction's priority.
#[derive(Clone, Debug)]
pub struct  TransactionDetail {
    pub module: &'static str,
    pub extrinsic: &'static str,
    pub transaction_data: Option<TransactionTypeDetail>,
}

/// Data type of the `tx_priority_list` json file.
#[derive(Clone, Encode, Decode, Deserialize, Debug)]
pub struct TransactionPriorityItem {
    module: String,
    extrinsic: String,
    transaction_data: Option<TransactionTypeDetail>,
    priority: TransactionPriority,
}
