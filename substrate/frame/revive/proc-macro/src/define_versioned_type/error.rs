// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

macro_rules! syn_error {
    (
        $first_span: expr => $first_message: expr
        $(, $span: expr => $message: expr)* $(,)?
    ) => {
        {
            #[allow(unused_mut)]
            let mut error = ::syn::Error::new($first_span, $first_message);
            $(
                error.combine(::syn::Error::new($span, $message));
            )*
            error
        }
    };
}

macro_rules! bail {
    ($($tt: tt)*) => {
        return core::result::Result::Err($crate::define_versioned_type::syn_error!($($tt)*))
    };
}

pub(crate) use bail;
pub(crate) use syn_error;
