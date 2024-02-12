//! # Contribution
//!
//! The following sections cover more detailed information about this crate and how it should be
//! maintained.
//!
//! ## Why Rust Docs?
//!
//! We acknowledge that blockchain based systems, particularly a cutting-edge one like Polkadot SDK
//! is a software artifact that is complex, and rapidly evolving. This makes the task of documenting
//! it externally extremely difficult, especially with regards to making sure it is up-to-date.
//!
//! Consequently, we argue that the best hedge against this is to move as much of the documentation
//! near the source code as possible. This would further incentivize developers to keep the
//! documentation up-to-date, as the overhead is reduced by making sure everything is in one
//! repository, and everything being in `.rs` files.
//!
//! > This is not say that a more visually appealing version of this crate (for example as an
//! > `md-book`) cannot exist, but it would be outside the scope of this crate.
//!
//! Moreover, we acknowledge that a major pain point has been not only outdated *concepts*, but also
//! *outdated code*. For this, we commit to making sure no code-snippet in this crate is left as
//! `///ignore` or `///no_compile`, making sure all code snippets are self-contained, compile-able,
//! and correct at every single revision of the entire repository.
//!
//! > This also allows us to have a clear versioning on the entire content of this crate. For every
//! commit of the Polkadot SDK, there would be one version of this crate that is guaranteed to be
//! correct.
//!
//! > To achieve this, we often use [`docify`](https://github.com/sam0x17/docify), a nifty invention
//! > of `@sam0x17`.
//!
//! Also see: <https://github.com/paritytech/polkadot-sdk/issues/991>.
//!
//! ## Scope
//!
//! The above would NOT be attainable if we don't acknowledge that the scope of this crate MUST be
//! limited, or else its maintenance burden would be infeasible or not worthwhile. In short, future
//! maintainers should always strive to keep the content of this repository as minimal as possible.
//! Some of the following principles are specifically there to be the guidance for this.
//!
//! ## Principles
//!
//! The following guidelines are meant to be the guiding torch of those who contribute to this
//! crate.
//!
//! 1. 🔺 Ground Up: Information should be laid out in the most ground-up fashion. The lowest level
//!    (i.e. "ground") is Rust-docs. The highest level (i.e. "up") is "outside of this crate". In
//!    between lies [`reference_docs`] and [`guides`], from low to high. The point of this principle
//!    is to document as much of the information as possible in the lower level media, as it is
//!    easier to maintain and more reachable. Then, use excessive linking to back-link when writing
//!    in a more high level.
//!
//! > A prime example of this, the details of the FRAME storage APIs should NOT be explained in a
//! > high level tutorial. They should be explained in the rust-doc of the corresponding type or
//! > macro.
//!
//! 2. 🧘 Less is More: For reasons mentioned [above](#why-rust-docs), the more concise this crate
//!    is, the better.
//! 3. √ Don’t Repeat Yourself – DRY: A summary of the above two points. Authors should always
//!    strive to avoid any duplicate information. Every concept should ideally be documented in
//!    *ONE* place and one place only. This makes the task of maintaining topics significantly
//!    easier.
//!
//! > A prime example of this, the list of CLI arguments of a particular binary should not be
//! > documented in multiple places across this crate. It should be only be documented in the
//! > corresponding crate (e.g. `sc_cli`).
//!
//! > Moreover, this means that as a contributor, **it is your responsibility to have a grasp over
//! > what topics are already covered in this crate, and how you can build on top of the information
//! > that they already pose, rather than repeating yourself**.
//!
//! For more details see the [latest documenting guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/DOCUMENTATION_GUIDELINES.md).
//!
//! #### Example: Explaining `#[pallet::call]`
//!
//! <details>
//! <summary>
//! Let's consider the seemingly simple example of explaining to someone dead-simple code of a FRAME
//! call and see how we can use the above principles.
//! </summary>
//!
//!
//! ```
//! #[frame::pallet(dev_mode)]
//! pub mod pallet {
//! #   use frame::prelude::*;
//! #   #[pallet::config]
//! #   pub trait Config: frame_system::Config {}
//! #   #[pallet::pallet]
//! #   pub struct Pallet<T>(_);
//!     #[pallet::call]
//!     impl<T: Config> Pallet<T> {
//!         pub fn a_simple_call(origin: OriginFor<T>, data: u32) -> DispatchResult {
//!             ensure!(data > 10, "SomeStaticString");
//!             todo!();
//!         }
//!     }
//! }
//! ```
//!
//! * Before even getting started, what is with all of this `<T: Config>`? We link to
//! [`crate::reference_docs::trait_based_programming`].
//! * First, the name. Why is this called `pallet::call`? This goes back to `enum Call`, which is
//! explained in [`crate::reference_docs::frame_composite_enums`]. Build on top of this!
//! * Then, what is `origin`? Just an account id? [`crate::reference_docs::frame_origin`].
//! * Then, what is `DispatchResult`? Why is this called *dispatch*? Probably something that can be
//! explained in the documentation of [`frame::prelude::DispatchResult`].
//! * Why is `"SomeStaticString"` a valid error? Because there is implementation for it that you can
//!   see [here](frame::prelude::DispatchError#impl-From<%26'static+str>-for-DispatchError).
//!
//!
//! All of these are examples of underlying information that a contributor should:
//!
//! 1. Try and create and they are going along.
//! 2. Back-link to if they already exist.
//!
//! Of course, all of this is not set in stone as a either/or rule. Sometimes, it is necessary to
//! rephrase a concept in a new context.
//!
//! </details>
//!
//! ## `docs.substrate.io`
//!
//! This crate is meant to gradually replace `docs.substrate.io`. As any content is added here, the
//! corresponding counter-part should be marked as deprecated, as described
//! [here](https://github.com/paritytech/polkadot-sdk-docs/issues/26).
//!
//! ## `crates.io` and Publishing
//!
//! As it stands now, this crate cannot be published to crates.io because of its use of
//! [workspace-level `docify`](https://github.com/sam0x17/docify/issues/22). For now, we accept this
//! compromise, but in the long term, we should work towards finding a way to maintain different
//! revisions of this crate.
//!
//! ## How to Develop Locally
//!
//! To view the docs specific [`crate`] locally for development, including the correct HTML headers
//! injected, run:
//!
//! ```sh
//! SKIP_WASM_BUILD=1 RUSTDOCFLAGS="--html-in-header $(pwd)/docs/sdk/headers/toc.html" cargo doc -p polkadot-sdk-docs --no-deps --open
//! ```
//!
//! If even faster build time for docs is needed, you can temporarily remove most of the
//! substrate/cumulus dependencies that are only used for linking purposes.
//!
//! For more on local development, see [`crate::reference_docs::development_environment_advice`].
