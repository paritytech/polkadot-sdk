//! MEV Shield primitives for Subtensor
#![cfg_attr(not(feature = "std"), no_std)]

use sp_inherents::InherentIdentifier;
use sp_runtime::{BoundedVec, traits::ConstU32};

mod keystore;
mod runtime_api;
mod shielded_tx;

pub use keystore::*;
pub use runtime_api::*;
pub use shielded_tx::*;

pub const LOG_TARGET: &str = "mev-shield";

// The inherent identifier for the next MEV-Shield public key.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"shieldpk";

// The public key type for the MEV-Shield.
pub type ShieldPublicKey = BoundedVec<u8, ConstU32<2048>>;

// The inherent type for the MEV-Shield.
pub type InherentType = Option<ShieldPublicKey>;
