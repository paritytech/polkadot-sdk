//! MEV Shield primitives for Subtensor
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sp_inherents::InherentIdentifier;
use sp_runtime::{traits::ConstU32, BoundedVec};

mod keystore;
mod runtime_api;
mod shielded_tx;

pub use keystore::*;
pub use runtime_api::*;
pub use shielded_tx::*;

pub const LOG_TARGET: &str = "mev-shield";

// The inherent identifier for the next MEV-Shield encapsulation key.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"shieldpk";

// ML-KEM-768 encapsulation key length in bytes.
pub const MLKEM768_ENC_KEY_LEN: usize = 1184;

// The encapsulation key type for the MEV-Shield.
pub type ShieldEncKey = BoundedVec<u8, ConstU32<2048>>;

// The inherent type for the MEV-Shield.
pub type InherentType = Option<ShieldEncKey>;
