//! # Offchain Workers
//!
//! This reference document explains how offchain workers work in Substrate and FRAME. The main
//! focus is upon FRAME's implementation of this functionality. Nonetheless, offchain workers are a
//! Substrate-provided feature and can be used with possible alternatives to [`frame`] as well.
//!
//! Offchain workers are a commonly misunderstood topic, therefore we explain them bottom-up,
//! starting at the fundamentals and then describing the developer interface.
//!
//! ## Context
//!
//! Recall from [`crate::reference_docs::wasm_meta_protocol`] that the node and the runtime
//! communicate with one another via host functions and runtime APIs. Many of these interactions
//! contribute to the actual state transition of the blockchain. For example [`sp_api::Core`] is the
//! main runtime API that is called to execute new blocks.
//!
//! Offchain workers are in principle not different in any way: It is a runtime API exposed by the
//! wasm blob ([`sp_offchain::OffchainWorkerApi`]), and the node software calls into it when it
//! deems fit. But, crucially, this API call is different in that:
//!
//! 1. It can have no impact on the state ie. it is _OFF (the) CHAIN_. If any state is altered
//!    during the execution of this API call, it is discarded.
//! 2. It has access to an extended set of host functions that allow the wasm blob to do more. For
//!    example, call into HTTP requests.
//!
//! > The main way through which an offchain worker can interact with the state is by submitting an
//! > extrinsic to the chain. This is the ONLY way to alter the state from an offchain worker.
//! > [`pallet_example_offchain_worker`] provides an example of this.
//!
//!
//! Given the "Off Chain" nature of this API, it is important to remember that calling this API is
//! entirely optional. Some nodes might call into it, some might not, and it would have no impact on
//! the execution of your blockchain because no state is altered no matter the execution of the
//! offchain worker API.
//!
//! Substrate's CLI allows some degree of configuration about this, allowing node operators to
//! specify when they want to run the offchain worker API. See
//! [`sc_cli::RunCmd::offchain_worker_params`].
//!
//! ## Nondeterministic Execution
//!
//! Needless to say, given the above description, the code in your offchain worker API can be
//! nondeterministic, as it is not part of the blockchain's STF, so it can be executed at unknown
//! times, by unknown nodes, and has no impact on the state. This is why an HTTP
//! ([`sp_runtime::offchain::http`]) API is readily provided to the offchain worker APIs. Because
//! there is no need for determinism in this context.
//!
//! > A common mistake here is for novice developers to see this HTTP API, and imagine that
//! > `polkadot-sdk` somehow magically solved the determinism in blockchains, and now a blockchain
//! > can make HTTP calls and it will all work. This is absolutely NOT the case. An HTTP call made
//! > by the offchain worker is non-deterministic by design. Blockchains can't and always won't be
//! > able to perform non-deterministic operations such as making HTTP calls to a foreign server.
//!
//! ## FRAME's API
//!
//! [`frame`] provides a simple API through which pallets can define offchain worker functions. This
//! is part of [`frame::traits::Hooks`], which is implemented as a part of
//! [`frame::pallet_macros::hooks`].
//!
//! ```
//! 
//! #[frame::pallet]
//! pub mod pallet {
//! 	use frame::prelude::*;
//!
//! 	#[pallet::config]
//! 	pub trait Config: frame_system::Config {}
//!
//! 	#[pallet::pallet]
//! 	pub struct Pallet<T>(_);
//!
//! 	#[pallet::hooks]
//! 	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
//! 		fn offchain_worker(block_number: BlockNumberFor<T>) {
//! 			// ...
//! 		}
//! 	}
//! }
//! ```
//!
//! Additionally, [`sp_runtime::offchain`] provides a set of utilities that can be used to moderate
//! the execution of offchain workers.
//!
//! ## Think Twice: Why Use Substrate's Offchain Workers?
//!
//! Consider the fact that in principle, an offchain worker code written using the above API is no
//! different than an equivalent written with an _actual offchain interaction library_, such as
//! [Polkadot-JS](https://polkadot.js.org/docs/), or any of the other ones listed [here](https://github.com/substrate-developer-hub/awesome-substrate?tab=readme-ov-file#client-libraries).
//!
//! They can both read from the state, and have no means of updating the state, other than the route
//! of submitting an extrinsic to the chain. Therefore, it is worth thinking twice before embedding
//! a logic as a part of Substrate's offchain worker API. Does it have to be there? can it not be a
//! simple, actual offchain application that lives outside of the chain's WASM blob?
//!
//! Some of the reasons why you might want to do the opposite, and actually embed an offchain worker
//! API into the WASM blob are:
//!
//! * Accessing the state is easier within the `offchain_worker` function, as it is already a part
//!   of the runtime, and [`frame::pallet_macros::storage`] provides all the tools needed to read
//!   the state. Other client libraries might provide varying degrees of capability here.
//! * It will be updated in synchrony with the runtime. A Substrate's offchain application is part
//!   of the same WASM blob, and is therefore guaranteed to be up to date.
//!
//! For example, imagine you have modified a storage item to have a new type. This will possibly
//! require a [`crate::reference_docs::frame_runtime_upgrades_and_migrations`], and any offchain
//! code, such as a Polkadot-JS application, will have to be updated to reflect this change. Whereas
//! the WASM offchain worker code is guaranteed to already be updated, or else the runtime code will
//! not even compile.
//!
//!
//! ## Further References
//!
//! - <https://forum.polkadot.network/t/offchain-workers-design-assumptions-vulnerabilities/2548>
//! - <https://substrate.stackexchange.com/questions/11058/how-can-i-create-ocw-that-wont-activates-every-block-but-will-activates-only-w/11060#11060>
//! - [Offchain worker example](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/examples/offchain-worker)
