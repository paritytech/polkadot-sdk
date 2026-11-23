// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

//! What backs the parachain: a relay chain, or the Parachain Service on JAM (`--cfg jam`).
//!
//! Both modules define the same names, so the rest of the runtime is written once against them:
//! the session keys, the consensus hook and its velocity and capacity, the maximum block length,
//! the `OnSetCode`, the Aura, session and authorship wiring, the XCM transport, and the authority
//! queries of the runtime APIs. Each also configures the pallets only its backing composes.

#[cfg(not(jam))]
mod relay;
#[cfg(not(jam))]
pub use relay::*;

#[cfg(jam)]
mod jam;
#[cfg(jam)]
pub use jam::*;
