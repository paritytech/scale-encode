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

use crate::{
    error::{Error, ErrorKind, Kind},
    EncodeAsType,
};
use alloc::{format, vec::Vec};
use scale_type_resolver::{visitor, TypeResolver};

impl EncodeAsType for scale_bits::Bits {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), crate::Error> {
        let type_id = super::find_single_entry_with_same_repr(type_id, types);

        let v = visitor::new((type_id.clone(), out), |(type_id, _out), _| {
            Err(wrong_shape(type_id))
        })
        .visit_bit_sequence(|(_type_id, out), store, order| {
            let format = scale_bits::Format { store, order };
            scale_bits::encode_using_format_to(self.iter(), format, out);
            Ok(())
        });

        super::resolve_type_and_encode(types, type_id, v)
    }
}

fn wrong_shape(type_id: impl core::fmt::Debug) -> Error {
    Error::new(ErrorKind::WrongShape {
        actual: Kind::BitSequence,
        expected_id: format!("{type_id:?}"),
    })
}
