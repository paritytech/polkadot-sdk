//! # Cumulus
//!
//! Substrate provides a framework through which a blockchain node and runtime can easily be
//! created. Cumulus aims to extend the same approach to creation of Polkadot parachains.
//!
//! > Cumulus clouds are shaped sort of like dots; together they form a system that is intricate,
//! > beautiful and functional.
//!
//! ## Example: Runtime
//!
//! A cumulus based runtime is fairly similar to a normal [FRAME]-based runtime. Most notably, the
//! following changes are applied to a normal FRAME-based runtime to make it a cumulus-based
//! runtime:
//!
//! #### Cumulus Pallets
//!
//! A parachain runtime should use a number of pallets that are provided by Cumulus. Notably:
//!
//! - [`frame-system`], like all FRAME-based runtimes.
//! - [`cumulus-pallet-parachain-system`]
//! - [`parachain-info`]
#![doc = docify::embed!("./src/polkadot_sdk/cumulus.rs", system_pallets)]
//!
//! Given that all cumulus-based runtimes use a simple aura-based consensus mechanism, the following
//! pallets also need to be added:
//!
//! - [`pallet-timestamp`]
//! - [`pallet-aura`]
//! - [`cumulus-pallet-aura-ext`]
// #![doc = docify::embed!("./src/lib.rs", consensus_pallets)]
//!
//!
//! Finally, a separate macro, similar to `impl_runtime_api`, which create the default set of
//! runtime apis, will generate the parachain runtime's main additional runtime api, also known as
//! PVF.
#![doc = docify::embed!("./src/polkadot_sdk/cumulus.rs", validate_block)]
//!
//!
//! ## Example: Running a node
//!
//! TODO
//!
//! ---
//!
//!
//! [FRAME]: https://paritytech.github.io/polkadot-sdk/master/frame/
//! [`frame_system`]: https://paritytech.github.io/polkadot-sdk/master/frame_system/

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

#[cfg(test)]
mod tests {
	mod runtime {
		pub use frame::{
			deps::sp_consensus_aura::sr25519::AuthorityId as AuraId, prelude::*,
			runtime::prelude::*, testing_prelude::*,
		};

		#[docify::export(CR)]
		construct_runtime!(
			pub struct Runtime {
				// system-level pallets.
				System: frame_system,
				Timestamp: pallet_timestamp,
				ParachainSystem: cumulus_pallet_parachain_system,
				ParachainInfo: parachain_info,

				// parachain consensus support -- mandatory.
				Aura: pallet_aura,
				AuraExt: cumulus_pallet_aura_ext,
			}
		);

		#[docify::export]
		mod system_pallets {
			use super::*;

			#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
			impl frame_system::Config for Runtime {
				type Block = MockBlock<Self>;
				type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
			}

			impl cumulus_pallet_parachain_system::Config for Runtime {
				type RuntimeEvent = RuntimeEvent;
				type OnSystemEvent = ();
				type SelfParaId = parachain_info::Pallet<Runtime>;
				type OutboundXcmpMessageSource = ();
				type DmpMessageHandler = ();
				type ReservedDmpWeight = ();
				type XcmpMessageHandler = ();
				type ReservedXcmpWeight = ();
				type CheckAssociatedRelayNumber =
					cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
				type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
					Runtime,
					6000, // relay chain block time
					1,
					1,
				>;
			}

			impl parachain_info::Config for Runtime {}
		}

		#[docify::export]
		mod consensus_pallets {
			use super::*;

			impl pallet_aura::Config for Runtime {
				type AuthorityId = AuraId;
				type DisabledValidators = ();
				type MaxAuthorities = ConstU32<100_000>;
				type AllowMultipleBlocksPerSlot = ConstBool<false>;
				#[cfg(feature = "experimental")]
				type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Self>;
			}

			#[docify::export(timestamp)]
			#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig as pallet_timestamp::DefaultConfig)]
			impl pallet_timestamp::Config for Runtime {}

			impl cumulus_pallet_aura_ext::Config for Runtime {}
		}

		#[docify::export(validate_block)]
		cumulus_pallet_parachain_system::register_validate_block! {
			Runtime = Runtime,
			BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
		}
	}
}
