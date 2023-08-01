// Copyright (C) 2023 Parity Technologies (UK) Ltd. (admin@parity.io)
// This file is a part of the scale-encode crate.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//         http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use syn::{
    parse::Parse, spanned::Spanned, Attribute, Meta, NestedMeta
};

fn find_meta_item<'a, F, R, I, M>(mut itr: I, mut pred: F) -> Option<R>
where
	F: FnMut(M) -> Option<R> + Clone,
	I: Iterator<Item = &'a Attribute>,
	M: Parse,
{
	itr.find_map(|attr| {
		attr.path.is_ident("codec").then(|| pred(attr.parse_args().ok()?)).flatten()
	})
}

/// Look for a `#[codec(skip)]` in the given attributes.
pub fn should_skip(attrs: &[Attribute]) -> bool {
    find_meta_item(attrs.iter(), |meta| {
        if let NestedMeta::Meta(Meta::Path(ref path)) = meta {
            if path.is_ident("skip") {
                return Some(path.span())
            }
        }

        None
    })
    .is_some()
}

