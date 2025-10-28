use alloc::vec::Vec;
use impl_trait_for_tuples::impl_for_tuples;
use sp_runtime::{DispatchError, DispatchResult};
use crate::InherentHandlerMessage;

/// Defines the interface for a pallet capable of handling a specific type of inherent system message.
///
/// Pallets implementing this trait register themselves as potential handlers for messages
/// identified by a unique `name`.
pub trait InherentHandler {
    /// A unique identifier for this handler
    fn handler_name() -> &'static [u8];

    /// The function that processes the raw  message byte intended for this handler.
    fn handle_message(message: Vec<u8>) -> DispatchResult;
}

/// Defines the interface for a collection(tuple) of `InherentHandler`
///
/// This trait provides functions to validate handler names and dispatch mesaages to the
/// appropriate handler within the collection.
pub trait InherentHandlers {
    /// Checks if a handler with the given `name` exists within this collection.
    ///
    /// Used during unsigned transaction validation(`ValidateUnsigned`) to ensure the targe handler is known before accepting the extrinsic.
    fn is_valid_handler(name: &[u8]) -> bool;

    /// Finds the handler matching the `handler_name` within the message and call its `handle_message`
    fn dispatch_message(message: InherentHandlerMessage) -> DispatchResult;
}


/// Base case implementation for an empty tuple `()`
impl InherentHandlers for () {
    fn is_valid_handler(_name: &[u8]) -> bool {
        false
    }

    fn dispatch_message(_message: InherentHandlerMessage) -> DispatchResult {
        Err(DispatchError::Other("No matching inherent handler found"))
    }
}

/// Recursive implementation for a non-empty tuple up to 8 elements .
#[impl_for_tuples(1, 8)]
#[tuple_types_custom_trait_bound(InherentHandler)]
impl InherentHandlers for Tuple {
    fn is_valid_handler(name: &[u8]) -> bool {
        for_tuples!( #( if Tuple::handler_name() == name { return true; } )* );
        false
    }

    fn dispatch_message(message: InherentHandlerMessage) -> DispatchResult {
        let handler_name = message.handler_name.as_slice();
        for_tuples!( #( if Tuple::handler_name() == handler_name {
            return Tuple::handle_message(message.raw_message);
        } )* );
        Err(DispatchError::Other("No matching inherent handler found"))
    }
}