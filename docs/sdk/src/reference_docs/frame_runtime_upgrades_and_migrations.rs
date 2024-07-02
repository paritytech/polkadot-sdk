//! # Runtime Upgrades
//!
//! At their core, blockchain logic consists of
//!
//! 1. on-chain state and
//! 2. a state transition function
//!
//! In Substrate-based blockchains, state transition functions are referred to as
//! [runtimes](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/blockchain_state_machines/index.html).
//!
//! Traditionally, before Substrate, upgrading state transition functions required node
//! operators to download new software and restart their nodes in a process called
//! [forking](https://en.wikipedia.org/wiki/Fork_(blockchain)).
//!
//! Substrate-based blockchains do not require forking, and instead upgrade runtimes
//! in a process called "Runtime Upgrades".
//!
//! Forkless runtime upgrades are a defining feature of the Substrate framework. Updating the
//! runtime logic without forking the code base enables your blockchain to seamlessly evolve
//! over time in a deterministic, rules-based manner. It also removes ambiguity for node operators
//! and other participants in the network about what is the canonical runtime.
//!
//! This capability is possible due to the runtime of a blockchain existing in on-chain storage.
//!
//! ## Performing a Runtime Upgrade
//!
//! To upgrade a runtime, an [`Origin`](frame_system::RawOrigin) with the necessary permissions
//! (usually via governance) changes the `:code` storage. Usually, this is performed via a call to
//! [`set_code`] (or [`set_code_without_checks`]) with the desired new runtime blob, scheduled
//! using [`pallet_scheduler`].
//!
//! Prior to building the new runtime, don't forget to update the
//! [`RuntimeVersion`](sp_version::RuntimeVersion).
//!
//! # Migrations
//!
//! It is often desirable to define logic to execute immediately after runtime upgrades (see
//! [this diagram](frame::traits::Hooks)).
//!
//! Self-contained pieces of logic that execute after a runtime upgrade are called "Migrations".
//!
//! The typical use case of a migration is to 'migrate' pallet storage from one layout to another,
//! for example when the encoding of a storage item is changed. However, they can also execute
//! arbitrary logic such as:
//!
//! - Calling arbitrary pallet methods
//! - Mutating arbitrary on-chain state
//! - Cleaning up some old storage items that are no longer needed
//!
//! ## Single Block Migrations
//!
//! - Execute immediately and entirely at the beginning of the block following
//! a runtime upgrade.
//! - Are suitable for migrations which are guaranteed to not exceed the block weight.
//! - Are simply implementations of [`OnRuntimeUpgrade`].
//!
//! To learn best practices for writing single block pallet storage migrations, see the
//! [Single Block Migration Example Pallet](pallet_example_single_block_migrations).
//!
//! ### Scheduling the Single Block Migrations to Run Next Runtime Upgrade
//!
//! Schedule migrations to run next runtime upgrade passing them as a generic parameter to your
//! [`Executive`](frame_executive) pallet:
//!
//! ```ignore
//! /// Tuple of migrations (structs that implement `OnRuntimeUpgrade`)
//! type Migrations = (
//! 	pallet_example_storage_migration::migrations::v1::versioned::MigrateV0ToV1,
//! 	MyCustomMigration,
//! 	// ...more migrations here
//! );
//! pub type Executive = frame_executive::Executive<
//! 	Runtime,
//! 	Block,
//! 	frame_system::ChainContext<Runtime>,
//! 	Runtime,
//! 	AllPalletsWithSystem,
//! 	Migrations, // <-- pass your migrations to Executive here
//! >;
//! ```
//!
//! ### Ensuring Single Block Migration Safety
//!
//! "My migration unit tests pass, so it should be safe to deploy right?"
//!
//! No! Unit tests execute the migration in a very simple test environment, and cannot account
//! for the complexities of a real runtime or real on-chain state.
//!
//! Prior to deploying migrations, it is critical to perform additional checks to ensure that when
//! run in our real runtime they will not brick the chain due to:
//! - Panicking
//! - Touching too many storage keys and resulting in an excessively large PoV
//! - Taking too long to execute
//!
//! [`try-runtime-cli`](https://github.com/paritytech/try-runtime-cli) has a sub-command
//! [`on-runtime-upgrade`](https://paritytech.github.io/try-runtime-cli/try_runtime_core/commands/enum.Action.html#variant.OnRuntimeUpgrade)
//! which is designed to help with exactly this.
//!
//! Developers MUST run this command before deploying migrations to ensure they will not
//! inadvertently result in a bricked chain.
//!
//! It is recommended to run as part of your CI pipeline. See the
//! [polkadot-sdk check-runtime-migration job](https://github.com/paritytech/polkadot-sdk/blob/4a293bc5a25be637c06ce950a34490706597615b/.gitlab/pipeline/check.yml#L103-L124)
//! for an example of how to configure this.
//!
//! ### Note on the Manipulability of PoV Size and Execution Time
//!
//! While [`try-runtime-cli`](https://github.com/paritytech/try-runtime-cli) can help ensure with
//! very high certainty that a migration will succeed given **existing** on-chain state, it cannot
//! prevent a malicious actor from manipulating state in a way that will cause the migration to take
//! longer or produce a PoV much larger than previously measured.
//!
//! Therefore, it is important to write migrations in such a way that the execution time or PoV size
//! it adds to the block cannot be easily manipulated. e.g., do not iterate over storage that can
//! quickly or cheaply be bloated.
//!
//! If writing your migration in such a way is not possible, a multi block migration should be used
//! instead.
//!
//! ### Other useful tools
//!
//! [`Chopsticks`](https://github.com/AcalaNetwork/chopsticks) is another tool in the Substrate
//! ecosystem which developers may find useful to use in addition to `try-runtime-cli` when testing
//! their single block migrations.
//!
//! ## Multi Block Migrations
//!
//! Safely and easily execute long-running migrations across multiple blocks.
//!
//! Suitable for migrations which could use arbitrary amounts of block weight.
//!
//! TODO: Link to multi block migration example/s once PR is merged (<https://github.com/paritytech/polkadot-sdk/pull/2119>).
//!
//! [`OnRuntimeUpgrade`]: frame_support::traits::OnRuntimeUpgrade
//! [`StorageVersion`]: frame_support::traits::StorageVersion
//! [`set_code`]: frame_system::Call::set_code
//! [`set_code_without_checks`]: frame_system::Call::set_code_without_checks
