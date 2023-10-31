//! # Developer Hub
//!
//! The Polkadot SDK Developer Hub.
//!
//! This crate is meant to be a *minimal*, but *always-accurate* source of information for those
//! wishing to build on the Polkadot SDK.
//!
//! ## Getting Started
//!
//! We suggest the following reading sequence:
//!
//! - Start by learning about the structure of the [`polkadot_sdk`] and its context.
//! - Then, head over the [`tutorial`] to get more hand-on practice.
//! - Whilst reading the tutorial, you might find back-links to [`reference_docs`].
//! - Finally, <https://paritytech.github.io> is the parent website of this crate that hosts the
//!   documentation of other related projects.
//!
//! ## Information Architecture
//!
//! This section paints a picture over the information architecture of this crate. In short, the
//! list of modules below is the starting point. Each module has a short description of what it is
//! covering.
//!
//! In a more visual representation, the information architecture of this crate is as follows:
#![doc = simple_mermaid::mermaid!("../../docs/mermaid/IA.mmd")]
//!
//! ## Contribution
//!
//! The following sections cover more detailed information about this crate and how it should be
//! maintained.
//!
//! ### Checklist
//!
//! TODO
//!
//! ### Note on `crates.io` and Publishing
//!
//! TODO: This crate cannot be published for now, and that is fine. We use `paritytech.github.io` as
//! the entry point.
//! TODO: link checker.
//!
//! ### Why Rust Docs?
//!
//! We acknowledge that blockchain based systems, particularly a cutting-edge one like the
//! Polkadot-Sdk is a software artifact that is complex, and rapidly evolving. This makes the task
//! of documenting it externally extremely difficult, especially with regards to making sure it is
//! up-to-date.
//!
//! Consequently, we argue that the best hedge against this is to move as much of the documentation
//! near the source code as possible. This would further incentivizes developers to keep the
//! documentation up-to-date, as the overhead is reduced by making sure everything is in one
//! repository, and everything being in `.rs` files.
//!
//! > This is not say that a more visually appealing version of this crate (for example as an
//! > `md-book`) cannot exist, but it would be the outside the scope of this crate.
//!
//! Moreover, we acknowledge that a major pain-pint of the past has been not only outdated
//! *concepts*, but also *outdated code*. For this, we commit to making sure no code-snippet in this
//! crate is left as `"/`/`/`ignore"`, making sure all code snippets are self-contained,
//! compile-able, and correct at every single revision of the entire repository. This also allows us
//! to have a clear versioning on the entire content of this crate. For every commit of the
//! Polkadot-Sdk, there would be one version of this crate that is guaranteed to be correct.
//!
//! > To achieve this, we often use [`docify`](https://github.com/sam0x17/docify), a nifty invention
//! > of `@sam0x17`.
//!
//! Also see: <https://github.com/paritytech/polkadot-sdk/issues/991>.
//!
//! ### Scope
//!
//! The above would NOT be unattainable if we don't acknowledge that the scope of this crate MUST be
//! limited, or else its maintenance burden would be infeasible or not worthwhile. In short, future
//! maintainers should always strive to keep the content of this repository as minimal as possible.
//! Some of the following principles are specifically there to be the guidance for this.
//!
//! ## Principles
//!
//! The following guidelines are meant to be the guiding torch of those who contribute to this
//! crate.
//!
//! 1. 🔺 Ground Up: Information should be layed out in the most ground-up fashion. The lowest level
//!    (ie. "ground") is Rust-docs. The highest level (ie "up") is "outside of this crate". In
//!    between lies [`reference_docs`] and [`tutorial`], from low to high. The point of this
//!    principle is to document as much of the information is possible in the lower lever mediums,
//!    as it is easier to maintain. Then, use excessive linking to back-link when writing in a more
//!    high level. Moreover, lower level mediums are often accessible to more readers.
//!
//! > A prime example of this, the details of the FRAME storage APIs should NOT be explained in a
//! > high level tutorial. They should be explained in the rust-doc of the corresponding type or
//! > macro.
//!
//! 2. 🧘 Less is More: For reasons mentioned [above](#crate::why-rust-docs), the more concise this
//!    crate is, the better.
//! 3. √ Don’t Repeat Yourself – DRY: A summary of the above two points. Authors should always
//!    strive to avoid any duplicate information. Every concept should ideally be documented in
//!    *ONE* place and one place only. This makes the task of maintaining topics significantly
//!    easier.
//!
//! > A prime example of this, the list of CLI arguments of a particular binary should not be
//! > documented in multiple places across this crate. It should be only be documented in the
//! > corresponding crate (eg. `sc_cli`).
//!
//! For more details about documenting guidelines, see:
//! <https://github.com/paritytech/polkadot-sdk/master/docs/DOCUMENTATION_GUIDELINES.md>
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

/// An introduction to the Polkadot SDK. Read this module to learn about the structure of the SDK,
/// the tools that are provided as a part of it, and to gain a high level understanding of each.
pub mod polkadot_sdk;
/// Reference documents covering in-depth topics across the Polkadot SDK. It is suggested to read
/// these on-demand, while you are going through the [`tutorial`] or other content.
pub mod reference_docs;
/// The main polkadot-sdk tutorial, targeted toward those who wish to build parachains FRAME and
/// Cumulus.
pub mod tutorial;
