//! # FRAME Benchmarking and Weights.
//!
//! Notes:
//!
//! On Weight as a concept.
//!
//! - Why we need it. Super important. People hate this. We need to argue why it is worth it.
//! - Axis of weight: PoV + Time.
//! - pre dispatch weight vs. metering and post dispatch correction.
//! 	- mention that we will do this for PoV
//! 	- you can manually refund using `DispatchResultWithPostInfo`.
//! - Technically you can have weights with any benchmarking framework. You just need one number to
//!   be computed pre-dispatch. But FRAME gives you a framework for this.
//! - improve documentation of `#[weight = ..]` and `#[pallet::weight(..)]`. All syntax variation
//!   should be covered.
//!
//! on FRAME benchmarking machinery:
//!
//! - component analysis, why everything must be linear.
//! - how to write benchmarks, how you must think of worst case.
//! - how to run benchmarks.
//!
//! - <https://www.shawntabrizi.com/assets/presentations/substrate-storage-deep-dive.pdf>
