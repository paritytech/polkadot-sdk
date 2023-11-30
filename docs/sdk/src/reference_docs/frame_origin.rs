//! # FRAME Origin
//!
//! Notes:
//!
//! - Def talk about account abstraction and how it is a solved issue in frame. See Gav's talk in
//!   Protocol Berg 2023
//! - system's raw origin, how it is amalgamated with other origins into one type
//! [`frame_composite_enums`]
//! - signed origin
//! - unsigned origin, link to [`fee_less_runtime`]
//! - Root origin, how no one can obtain it.
//! - Abstract origin: how FRAME allows you to express "origin is 2/3 of the this body or 1/2 of
//!   that body or half of the token holders".
//! - `type CustomOrigin: EnsureOrigin<_>` in pallets.
