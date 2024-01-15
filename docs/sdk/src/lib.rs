//! # Polkadot SDK Docs
//!
//! The Polkadot SDK Developer Documentation.
//!
//! This crate is a *minimal*, but *always-accurate* source of information for those wishing to
//! build on the Polkadot SDK.
//!
//! > **Work in Progress**: This crate is under heavy development. Expect content to be moved and
//! > changed. Do not use links to this crate yet. See [`meta_contributing`] for more information.
//!
//! ## Getting Started
//!
//! We suggest the following reading sequence:
//!
//! - Start by learning about the the [`polkadot_sdk`], its structure and context.
//! - Then, head over the [`guides`]. This modules contains in-depth guides about the most important
//!   user-journeys of the Polkadot SDK.
//! - Whilst reading the guides, you might find back-links to [`crate::reference_docs`].
//! - Finally, <https://paritytech.github.io> is the parent website of this crate that contains the
//!   list of further tools related to the Polkadot SDK.
//!
//! ## Information Architecture
//!
//! This section paints a picture over the high-level information architecture of this crate.
#![doc = simple_mermaid::mermaid!("../../mermaid/IA.mmd")]
#![warn(rustdoc::broken_intra_doc_links)]
#![warn(rustdoc::private_intra_doc_links)]

/// Meta information about this crate, how it is built, what principles dictates its evolution and
/// how one can contribute to it.
pub mod meta_contributing;

/// In-depth guides about the most common components of the Polkadot SDK. They are slightly more
/// high level and broad than reference docs.
pub mod guides;
/// An introduction to the Polkadot SDK. Read this module to learn about the structure of the SDK,
/// the tools that are provided as a part of it, and to gain a high level understanding of each.
pub mod polkadot_sdk;
/// Reference documents covering in-depth topics across the Polkadot SDK. It is suggested to read
/// these on-demand, while you are going through the [`guides`] or other content.
pub mod reference_docs;
