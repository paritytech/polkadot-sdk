//! # Polkadot SDK Docs
//!
//! The Polkadot SDK Developer Documentation.
//!
//! This crate is a *minimal*, but *always-accurate* source of information for those wishing to
//! build on the Polkadot SDK.
//!
//! ## Getting Started
//!
//! We suggest the following reading sequence:
//!
//! - Start by learning about the the [`polkadot_sdk`], its structure and context.
//! - Then, head over to the [`guides`]. This modules contains in-depth guides about the most
//!   important user-journeys of the Polkadot SDK.
//! 	- Whilst reading the guides, you might find back-links to [`reference_docs`].
//! - [`external_resources`] for a list of 3rd party guides and tutorials.
//! - Finally, <https://paritytech.github.io> is the parent website of this crate that contains the
//!   list of further tools related to the Polkadot SDK.
//!
//! ## Information Architecture
//!
//! This section paints a picture over the high-level information architecture of this crate.
#![doc = simple_mermaid::mermaid!("../../mermaid/IA.mmd")]
#![warn(rustdoc::broken_intra_doc_links)]
#![warn(rustdoc::private_intra_doc_links)]
#![doc(html_favicon_url = "https://polkadot.com/favicon.ico")]
#![doc(
	html_logo_url = "https://raw.githubusercontent.com/paritytech/polkadot-sdk/master/docs/images/Polkadot_Logo_Horizontal_Pink_White.png"
)]
#![doc(issue_tracker_base_url = "https://github.com/paritytech/polkadot-sdk/issues")]

/// Meta information about this crate, how it is built, what principles dictates its evolution and
/// how one can contribute to it.
pub mod meta_contributing;

/// A list of external resources and learning material about Polkadot SDK.
pub mod external_resources;

/// In-depth guides about the most common components of the Polkadot SDK. They are slightly more
/// high level and broad than [`reference_docs`].
pub mod guides;

/// An introduction to the Polkadot SDK. Read this module to learn about the structure of the SDK,
/// the tools that are provided as a part of it, and to gain a high level understanding of each.
pub mod polkadot_sdk;
/// Reference documents covering in-depth topics across the Polkadot SDK. It is suggested to read
/// these on-demand, while you are going through the [`guides`] or other content.
pub mod reference_docs;
