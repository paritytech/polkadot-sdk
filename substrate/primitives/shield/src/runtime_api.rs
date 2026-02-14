//! Runtime API definition for the MEV Shield.

extern crate alloc;

use crate::ShieldedTransaction;
use sp_runtime::traits::Block as BlockT;

type ExtrinsicOf<Block> = <Block as BlockT>::Extrinsic;

sp_api::decl_runtime_apis! {
    pub trait ShieldApi {
        fn try_decode_shielded_tx(uxt: ExtrinsicOf<Block>) -> Option<ShieldedTransaction>;
        fn try_unshield_tx(shielded_tx: ShieldedTransaction) -> Option<ExtrinsicOf<Block>>;
    }
}
