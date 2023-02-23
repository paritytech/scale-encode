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
    error::{Error, ErrorKind, Kind, Location},
    EncodeAsFields, EncodeAsType,
};
use codec::{Compact, Encode};
use scale_info::{PortableRegistry, TypeDef};
use std::collections::HashMap;

/// This type represents named or unnamed composite values, and can be used
/// to help generate `EncodeAsType` impls. It's primarily used by the exported
/// macros to do just that.
///
/// ```rust
/// use scale_encode::{ Error, EncodeAsType, Composite, PortableRegistry };
///
/// struct MyType {
///    foo: bool,
///    bar: u64,
///    wibble: String
/// }
///
/// impl EncodeAsType for MyType {
///     fn encode_as_type_to(&self, type_id: u32, types: &PortableRegistry, out: &mut Vec<u8>) -> Result<(), Error> {
///         Composite([
///             (Some("foo"), &self.foo as &dyn EncodeAsType),
///             (Some("bar"), &self.bar as &dyn EncodeAsType),
///             (Some("wibble"), &self.wibble as &dyn EncodeAsType)
///         ].into_iter()).encode_as_type_to(type_id, types, out)
///     }
/// }
/// ```
pub struct Composite<Vals>(pub Vals);

impl<'a, Vals> EncodeAsType for Composite<Vals>
where
    Vals: ExactSizeIterator<Item = (Option<&'a str>, &'a dyn EncodeAsType)> + Clone,
{
    fn encode_as_type_to(
        &self,
        type_id: u32,
        types: &PortableRegistry,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let mut vals_iter = self.0.clone();
        let vals_iter_len = vals_iter.len();

        // If the source or target type are tuples or composites with one field, "unwrap"
        // that field and just try encoding the inner content.
        let type_id = super::find_single_entry_with_same_repr(type_id, types);
        if vals_iter_len == 1 {
            return vals_iter
                .next()
                .unwrap()
                .1
                .encode_as_type_to(type_id, types, out);
        }

        let ty = types
            .resolve(type_id)
            .ok_or_else(|| Error::new(ErrorKind::TypeNotFound(type_id)))?;

        match ty.type_def() {
            TypeDef::Tuple(tuple) => {
                let fields = tuple.fields();
                self.encode_as_field_ids_to(fields, types, out)
            }
            TypeDef::Composite(composite) => {
                let fields = composite.fields();
                self.encode_as_fields_to(fields, types, out)
            }
            TypeDef::Array(array) => {
                let array_len = array.len() as usize;

                if vals_iter_len != array_len {
                    return Err(Error::new(ErrorKind::WrongLength {
                        actual_len: vals_iter_len,
                        expected_len: array_len,
                    }));
                }

                for (idx, (name, val)) in vals_iter.enumerate() {
                    let loc = if let Some(name) = name {
                        Location::field(name.to_string())
                    } else {
                        Location::idx(idx)
                    };
                    val.encode_as_type_to(array.type_param().id(), types, out)
                        .map_err(|e| e.at(loc))?;
                }
                Ok(())
            }
            TypeDef::Sequence(seq) => {
                // sequences start with compact encoded length:
                Compact(vals_iter_len as u32).encode_to(out);
                for (idx, (name, val)) in vals_iter.enumerate() {
                    let loc = if let Some(name) = name {
                        Location::field(name.to_string())
                    } else {
                        Location::idx(idx)
                    };
                    val.encode_as_type_to(seq.type_param().id(), types, out)
                        .map_err(|e| e.at(loc))?;
                }
                Ok(())
            }
            _ => {
                // Is there exactly one item to iterate over?
                let (Some((_name, item)), None) = (vals_iter.next(), vals_iter.next()) else {
                    return Err(Error::new(ErrorKind::WrongShape { actual: Kind::Tuple, expected: type_id }));
                };
                // Tuple with 1 entry? before giving up, try encoding the inner entry instead:
                item.encode_as_type_to(type_id, types, out)
                    .map_err(|e| e.at_idx(0))?;
                Ok(())
            }
        }
    }
}

impl<'a, Vals> EncodeAsFields for Composite<Vals>
where
    Vals: ExactSizeIterator<Item = (Option<&'a str>, &'a dyn EncodeAsType)> + Clone,
{
    fn encode_as_fields_to(
        &self,
        fields: &[crate::PortableField],
        types: &PortableRegistry,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let vals_iter = self.0.clone();

        // Both the target and source type have to have named fields for us to use
        // names to line them up.
        let is_named = {
            let is_target_named = fields.iter().any(|f| f.name().is_some());
            let is_source_named = vals_iter.clone().any(|(name, _)| name.is_some());
            is_target_named && is_source_named
        };

        if is_named {
            // target + source fields are named, so hash source values by name and
            // then encode to the target type by matching the names. If fields are
            // named, we don't even mind if the number of fields doesn't line up;
            // we just ignore any fields we provided that aren't needed.
            let source_fields_by_name: HashMap<&str, &dyn EncodeAsType> = vals_iter
                .map(|(name, val)| (name.unwrap_or(""), val))
                .collect();

            for field in fields {
                // Find the field in our source type:
                let name = field.name().map(|n| &**n).unwrap_or("");
                let Some(value) = source_fields_by_name.get(name) else {
                    return Err(Error::new(ErrorKind::CannotFindField { name: name.to_string() }))
                };

                // Encode the value to the output:
                value
                    .encode_as_type_to(field.ty().id(), types, out)
                    .map_err(|e| e.at_field(name.to_string()))?;
            }

            Ok(())
        } else {
            // target fields aren't named, so encode by order only. We need the field length
            // to line up for this to work.
            if fields.len() != vals_iter.len() {
                return Err(Error::new(ErrorKind::WrongLength {
                    actual_len: vals_iter.len(),
                    expected_len: fields.len(),
                }));
            }

            for (idx, (field, (name, val))) in fields.iter().zip(vals_iter).enumerate() {
                let loc = if let Some(name) = name {
                    Location::field(name.to_string())
                } else {
                    Location::idx(idx)
                };
                val.encode_as_type_to(field.ty().id(), types, out)
                    .map_err(|e| e.at(loc))?;
            }
            Ok(())
        }
    }
}
