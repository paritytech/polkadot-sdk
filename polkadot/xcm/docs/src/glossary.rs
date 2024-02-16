//! # Glossary
//!
//! ## XCM (Cross-Consensus Messaging)
//!
//! A messaging format meant to communicate intentions between consensus systems.
//! XCM could also refer to a single message.
//!
//! ## Instructions
//!
//! XCMs are composed of a sequence of instructions.
//! Each instruction aims to convey a particular intention.
//! There are instructions for transferring and locking assets, handling fees, calling arbitrary
//! blobs, and more.
//!
//! ## Consensus system
//!
//! A system that can reach any kind of consensus.
//! For example, relay chains, parachains, smart contracts.
//!
//! ## MultiLocation
//!
//! A way of addressing consensus systems.
//! These could be relative or absolute.
//!
//! ## Junction
//!
//! The different ways of descending down a `MultiLocation` hierarchy.
//! A junction can be a Parachain, an Account, or more.
//!
//! ## MultiAsset
//!
//! A way of identifying assets in the same or another consensus system, by using a `MultiLocation`.
//!
//! ## Sovereign account
//!
//! An account in a consensus system that is controlled by an account in another consensus system.
//!
//! ## Teleport
//!
//! A way of transferring assets between two consensus systems without the need of a third party.
//! It consists of the sender system burning the asset that wants to be sent over and the recipient
//! minting an equivalent amount of that asset. It requires a lot of trust between the two systems,
//! since failure to mint or burn will reduce or increase the total issuance of the token.
//!
//! ## Reserve asset transfer
//!
//! A way of transferring assets between two consensus systems that don't trust each other, by using
//! a third system they both trust, called the reserve. The real asset only exists on the reserve,
//! both sender and recipient only deal with derivatives. It consists of the sender burning a
//! certain amount of derivatives, telling the reserve to move real assets from its sovereign
//! account to the destination's sovereign account, and then telling the recipient to mint the right
//! amount of derivatives.
//!
//! ## XCVM
//!
//! The virtual machine behind XCM.
//! Every XCM is an XCVM programme.
//! Holds state in registers.
//!
//! ## Holding register
//!
//! An XCVM register used to hold arbitrary `Asset`s during the execution of an XCVM programme.
//!
//! ## Barrier
//!
//! An XCM executor configuration item that works as a firewall for incoming XCMs.
//! All XCMs have to pass the barrier to be executed, else they are dropped.
//! It can be used for whitelisting only certain types or messages or messages from certain senders.
//!
//! ## UMP (Upward Message Passing)
//!
//! Transport-layer protocol that allows parachains to send messages upwards to their relay chain.
//!
//! ## DMP (Downward Message Passing)
//!
//! Transport-layer protocol that allows the relay chain to send messages downwards to one of their
//! parachains.
//!
//! ## XCMP (Cross-Consensus Message Passing)
//!
//! Transport-layer protocol that allows parachains to send messages between themselves, without
//! going through the relay chain.
//!
//! ## HRMP (Horizontal Message Passing)
//!
//! Transport-layer protocol that allows a parachain to send messages to a sibling parachain going
//! through the relay chain. It's a precursor to XCMP, also known as XCMP-lite.
//! It uses a mixture of UMP and VMP.
