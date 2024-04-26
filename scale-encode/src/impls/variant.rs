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

use super::composite::{Composite, CompositeField};
use crate::error::{Error, ErrorKind, Kind};
use alloc::{format, string::ToString, vec::Vec};
use codec::Encode;
use scale_type_resolver::{visitor, TypeResolver};

/// This type represents named or unnamed composite values, and can be used
/// to help generate `EncodeAsType` impls. It's primarily used by the exported
/// macros to do just that.
///
/// ```rust
/// use scale_encode::{
///     Error, EncodeAsType, Composite, CompositeField, Variant, TypeResolver
/// };
///
/// enum MyType {
///    SomeField(bool),
///    OtherField { foo: u64, bar: String }
/// }
///
/// impl EncodeAsType for MyType {
///     fn encode_as_type_to<R: TypeResolver>(
///         &self,
///         type_id: R::TypeId,
///         types: &R,
///         out: &mut Vec<u8>
///     ) -> Result<(), Error> {
///         match self {
///             MyType::SomeField(b) => Variant {
///                 name: "SomeField",
///                 fields: Composite::new([
///                     (None, CompositeField::new(b)),
///                 ].into_iter())
///             }.encode_variant_as_type_to(type_id, types, out),
///             MyType::OtherField { foo, bar } => Variant {
///                 name: "OtherField",
///                 fields: Composite::new([
///                     (Some("foo"), CompositeField::new(foo)),
///                     (Some("bar"), CompositeField::new(bar))
///                 ].into_iter())
///             }.encode_variant_as_type_to(type_id, types, out)
///         }
///     }
/// }
/// ```
pub struct Variant<'a, R, Vals> {
    /// The name of the variant we'll try to encode into.
    pub name: &'a str,
    /// The fields of the variant that we wish to encode.
    pub fields: Composite<R, Vals>,
}

impl<'a, R, Vals> Variant<'a, R, Vals>
where
    R: TypeResolver + 'a,
    Vals: ExactSizeIterator<Item = (Option<&'a str>, CompositeField<'a, R>)> + Clone,
{
    /// A shortcut for [`Self::encode_variant_as_type_to()`] which internally
    /// allocates a [`Vec`] and returns it.
    pub fn encode_variant_as_type(&self, type_id: R::TypeId, types: &R) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        self.encode_variant_as_type_to(type_id, types, &mut out)?;
        Ok(out)
    }

    /// Encode the variant as the provided type to the output bytes.
    pub fn encode_variant_as_type_to(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let type_id = super::find_single_entry_with_same_repr(type_id, types);

        let v = visitor::new(type_id.clone(), |type_id, _| {
            Err(Error::new(ErrorKind::WrongShape {
                actual: Kind::Str,
                expected_id: format!("{type_id:?}"),
            }))
        })
        .visit_variant(|type_id, _, vars| {
            let mut res = None;
            for var in vars {
                if var.name == self.name {
                    res = Some(var);
                    break;
                }
            }

            let Some(mut var) = res else {
                return Err(Error::new(ErrorKind::CannotFindVariant {
                    name: self.name.to_string(),
                    expected_id: format!("{type_id:?}"),
                }));
            };

            var.index.encode_to(out);
            self.fields
                .encode_composite_fields_to(&mut var.fields, types, out)
        });

        super::resolve_type_and_encode(types, type_id, v)
    }
}
