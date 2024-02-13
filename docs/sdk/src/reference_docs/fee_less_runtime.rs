//! # Fee-Less Runtime
//!
//!
//! Notes:
//!
//! - An extension of [`runtime_vs_smart_contract`], showcasing the tools needed to build a safe
//!   runtime that is fee-less.
//! - Would need to use unsigned origins, custom validate_unsigned, check the existence of some NFT
//!   and some kind of rate limiting (eg. any account gets 5 free tx per day).
//! - The rule of thumb is that as long as the unsigned validate does one storage read, similar to
//!   nonce, it is fine.
//! - This could possibly be a good guide/template, rather than a reference doc.
