//! This crate contains the definitions of the Polkadot Facade Runtime APIs.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sp_facade_apis_macro::define_facade_apis;

/// Some custom type.
pub type MyCustomType = bool;
/// Another custom type.
pub type CustomThing = bool;
/// String type.
pub type String = alloc::string::String;

define_facade_apis! { 
    /// An example facade API. Traits are defined the same way
    /// as with `decl_runtime_apis` with some restrictions.
    pub trait FacadeExample {
        /// Method docs are required.
        fn foo(bar: u32, other: Option<String>) -> MyCustomType;

        /// api_version is supported on methods, but not on the
        /// top level trait (because all versions should be defined).
        #[api_version(2)]
        fn bar(something: String, more: CustomThing);

        /// We'll get a compile error if we see a version number N
        /// where N-1 isn't an existing version of another method.
        #[api_version(3)]
        fn wibble(something: String, more: CustomThing);
    }
}
