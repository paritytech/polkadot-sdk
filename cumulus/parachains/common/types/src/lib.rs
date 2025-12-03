#![cfg_attr(not(feature = "std"), no_std)]

pub mod opaque;

use sp_runtime::traits::{IdentifyAccount, Verify};

/// An index to a block.
pub type BlockNumber = u32;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = sp_runtime::MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// The type for looking up accounts. We don't expect more than 4 billion of them, but you
/// never know...
pub type AccountIndex = u32;

/// Balance of an account.
pub type Balance = u128;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// Digest item type.
pub type DigestItem = sp_runtime::generic::DigestItem;

// Aura consensus authority.
pub type AuraId = sp_consensus_aura::sr25519::AuthorityId;

// Aura consensus authority used by Asset Hub Polkadot.
//
// Because of registering the authorities with an ed25519 key before switching from Shell
// to Asset Hub Polkadot, we were required to deploy a hotfix that changed Asset Hub Polkadot's
// Aura keys to ed22519. In the future that may change again.
pub type AssetHubPolkadotAuraId = sp_consensus_aura::ed25519::AuthorityId;

// Id used for identifying assets.
pub type AssetIdForTrustBackedAssets = u32;

// Id used for identifying non-fungible collections.
pub type CollectionId = u32;

// Id used for identifying non-fungible items.
pub type ItemId = u32;
