//! Runtime API definition for the MEV Shield.

extern crate alloc;

use crate::ShieldedTransaction;
use alloc::vec::Vec;
use sp_runtime::traits::Block as BlockT;

type ExtrinsicOf<Block> = <Block as BlockT>::Extrinsic;

sp_api::decl_runtime_apis! {
	pub trait ShieldApi {
		/// Try to decode a shielded transaction from an extrinsic.
		fn try_decode_shielded_tx(uxt: ExtrinsicOf<Block>) -> Option<ShieldedTransaction>;

		/// Check if a transaction is shielded using the current key.
		fn is_shielded_using_current_key(key_hash: &[u8; 16]) -> bool;

		/// Try to unshield a transaction using a decapsulation key.
		fn try_unshield_tx(dec_key_bytes: Vec<u8>, shielded_tx: ShieldedTransaction) -> Option<ExtrinsicOf<Block>>;
	}
}
