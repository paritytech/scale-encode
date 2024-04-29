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
    EncodeAsType, Field, FieldIter, TypeResolver,
};
use alloc::collections::BTreeMap;
use alloc::{format, string::ToString, vec::Vec};
use scale_type_resolver::visitor;

/// This trait exists to get around object safety issues using [`EncodeAsType`].
/// It's object safe and automatically implemented for any type which implements
/// [`EncodeAsType`]. We need this to construct generic [`Composite`] types.
trait EncodeAsTypeWithResolver<R: TypeResolver> {
    fn encode_as_type_with_resolver_to(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error>;
}
impl<T: EncodeAsType, R: TypeResolver> EncodeAsTypeWithResolver<R> for T {
    fn encode_as_type_with_resolver_to(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self.encode_as_type_to(type_id, types, out)
    }
}

/// A struct representing a single composite field. To be used in conjunction
/// with the [`Composite`] struct to construct generic composite shaped types.
/// this basically takes a type which implements [`EncodeAsType`] and turns it
/// into something object safe.
pub struct CompositeField<'a, R> {
    val: &'a dyn EncodeAsTypeWithResolver<R>,
}

impl<'a, R> Copy for CompositeField<'a, R> {}
impl<'a, R> Clone for CompositeField<'a, R> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, R> core::fmt::Debug for CompositeField<'a, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CompositeField")
    }
}

impl<'a, R: TypeResolver> CompositeField<'a, R> {
    /// Construct a new composite field given some type which implements
    /// [`EncodeAsType`].
    pub fn new<T: EncodeAsType>(val: &'a T) -> Self {
        CompositeField { val }
    }

    /// SCALE encode this composite field to bytes based on the underlying type.
    pub fn encode_composite_field_to(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self.val
            .encode_as_type_with_resolver_to(type_id, types, out)
    }
}

/// This type represents named or unnamed composite values, and can be used to help generate
/// `EncodeAsType` impls. It's primarily used by the exported macros to do just that.
///
/// ```rust
/// use scale_encode::{
///     Error, EncodeAsType, Composite, CompositeField, TypeResolver
/// };
///
/// struct MyType {
///    foo: bool,
///    bar: u64,
///    wibble: String
/// }
///
/// impl EncodeAsType for MyType {
///     fn encode_as_type_to<R: TypeResolver>(
///         &self,
///         type_id: R::TypeId,
///         types: &R,
///         out: &mut Vec<u8>
///     ) -> Result<(), Error> {
///         Composite::new([
///             (Some("foo"), CompositeField::new(&self.foo)),
///             (Some("bar"), CompositeField::new(&self.bar)),
///             (Some("wibble"), CompositeField::new(&self.wibble))
///         ].into_iter()).encode_composite_as_type_to(type_id, types, out)
///     }
/// }
/// ```
///
/// [`Composite`] cannot implement [`EncodeAsType`] itself, because it is tied to being
/// encoded with a specific `R: TypeResolver`, whereas things implementing [`EncodeAsType`]
/// need to be encodable using _any_ [`TypeResolver`]. This is ultimately because
/// [`EncodeAsType`] is not object safe, which prevents it from being used to describe
/// [`CompositeFields`][CompositeField].
pub struct Composite<R, Vals> {
    vals: Vals,
    marker: core::marker::PhantomData<R>,
}

impl<'a, R, Vals> Composite<R, Vals>
where
    R: TypeResolver + 'a,
    Vals: ExactSizeIterator<Item = (Option<&'a str>, CompositeField<'a, R>)> + Clone,
{
    /// Construct a new [`Composite`] type by providing an iterator over
    /// the fields that it contains.
    ///
    /// ```rust
    /// use scale_encode::{ Composite, CompositeField };
    /// use scale_info::PortableRegistry;
    ///
    /// Composite::<PortableRegistry, _>::new([
    ///     (Some("foo"), CompositeField::new(&123)),
    ///     (Some("bar"), CompositeField::new(&"hello"))
    /// ].into_iter());
    /// ```
    pub fn new(vals: Vals) -> Self {
        Composite {
            vals,
            marker: core::marker::PhantomData,
        }
    }

    /// A shortcut for [`Self::encode_composite_as_type_to()`] which internally
    /// allocates a [`Vec`] and returns it.
    pub fn encode_composite_as_type(
        &self,
        type_id: R::TypeId,
        types: &R,
    ) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        self.encode_composite_as_type_to(type_id, types, &mut out)?;
        Ok(out)
    }

    /// Encode this composite value as the provided type to the output bytes.
    pub fn encode_composite_as_type_to(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let vals_iter = self.vals.clone();
        let vals_iter_len = vals_iter.len();

        // Skip through any single field composites/tuples without names. If there
        // are names, we may want to line up input field(s) on them.
        let type_id = skip_through_single_unnamed_fields(type_id, types);

        let v = visitor::new(
            (type_id.clone(), out, vals_iter),
            |(type_id, out, mut vals_iter), _| {
                // Rather than immediately giving up, we should at least see whether
                // we can skip one level in to our value and encode that.
                if vals_iter_len == 1 {
                    return vals_iter
                        .next()
                        .expect("1 value expected")
                        .1
                        .encode_composite_field_to(type_id, types, out);
                }

                // If we get here, then it means the value we were given had more than
                // one field, and the type we were given was ultimately some one-field thing
                // that contained a non composite/tuple type, so it would never work out.
                Err(Error::new(ErrorKind::WrongShape {
                    actual: Kind::Struct,
                    expected_id: format!("{type_id:?}"),
                }))
            },
        )
        .visit_not_found(|(type_id, _, _)| {
            Err(Error::new(ErrorKind::TypeNotFound(format!("{type_id:?}"))))
        })
        .visit_composite(|(type_id, out, mut vals_iter), _, mut fields| {
            // If vals are named, we may need to line them up with some named composite.
            // If they aren't named, we only care about lining up based on matching lengths.
            let is_named_vals = vals_iter.clone().any(|(name, _)| name.is_some());

            // If there is exactly one val that isn't named, then we know it won't line
            // up with this composite then, so try encoding one level in.
            if !is_named_vals && vals_iter_len == 1 {
                return vals_iter
                    .next()
                    .expect("1 value expected")
                    .1
                    .encode_composite_field_to(type_id, types, out);
            }

            self.encode_composite_fields_to(&mut fields, types, out)
        })
        .visit_tuple(|(type_id, out, mut vals_iter), type_ids| {
            // If there is exactly one val, it won't line up with the tuple then, so
            // try encoding one level in instead.
            if vals_iter_len == 1 {
                return vals_iter
                    .next()
                    .unwrap()
                    .1
                    .encode_composite_field_to(type_id, types, out);
            }

            let mut fields = type_ids.map(Field::unnamed);
            self.encode_composite_fields_to(
                &mut fields as &mut dyn FieldIter<'_, R::TypeId>,
                types,
                out,
            )
        });

        super::resolve_type_and_encode(types, type_id, v)
    }

    /// A shortcut for [`Self::encode_composite_fields_to()`] which internally
    /// allocates a [`Vec`] and returns it.
    pub fn encode_composite_fields(
        &self,
        fields: &mut dyn FieldIter<'_, R::TypeId>,
        types: &R,
    ) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        self.encode_composite_fields_to(fields, types, &mut out)?;
        Ok(out)
    }

    /// Encode the composite fields as the provided field description to the output bytes
    pub fn encode_composite_fields_to(
        &self,
        fields: &mut dyn FieldIter<'_, R::TypeId>,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let vals_iter = self.vals.clone();

        // Most of the time there aren't too many fields, so avoid allocation in most cases:
        let fields = smallvec::SmallVec::<[_; 16]>::from_iter(fields);

        // Both the target and source type have to have named fields for us to use
        // names to line them up.
        let is_named = {
            let is_target_named = fields.iter().any(|f| f.name.is_some());
            let is_source_named = vals_iter.clone().any(|(name, _)| name.is_some());
            is_target_named && is_source_named
        };

        if is_named {
            // target + source fields are named, so hash source values by name and
            // then encode to the target type by matching the names. If fields are
            // named, we don't even mind if the number of fields doesn't line up;
            // we just ignore any fields we provided that aren't needed.
            let source_fields_by_name: BTreeMap<&str, CompositeField<'a, R>> = vals_iter
                .map(|(name, val)| (name.unwrap_or(""), val))
                .collect();

            for field in fields {
                // Find the field in our source type:
                let name = field.name.unwrap_or("");
                let Some(value) = source_fields_by_name.get(name) else {
                    return Err(Error::new(ErrorKind::CannotFindField {
                        name: name.to_string(),
                    }));
                };

                // Encode the value to the output:
                value
                    .encode_composite_field_to(field.id, types, out)
                    .map_err(|e| e.at_field(name.to_string()))?;
            }

            Ok(())
        } else {
            let fields_len = fields.len();

            // target fields aren't named, so encode by order only. We need the field length
            // to line up for this to work.
            if fields_len != vals_iter.len() {
                return Err(Error::new(ErrorKind::WrongLength {
                    actual_len: vals_iter.len(),
                    expected_len: fields_len,
                }));
            }

            for (idx, (field, (name, val))) in fields.iter().zip(vals_iter).enumerate() {
                val.encode_composite_field_to(field.id.clone(), types, out)
                    .map_err(|e| {
                        let loc = if let Some(name) = name {
                            Location::field(name.to_string())
                        } else {
                            Location::idx(idx)
                        };
                        e.at(loc)
                    })?;
            }
            Ok(())
        }
    }
}

// Single unnamed fields carry no useful information and can be skipped through.
// Single named fields may still be useful to line up with named composites.
fn skip_through_single_unnamed_fields<R: TypeResolver>(type_id: R::TypeId, types: &R) -> R::TypeId {
    let v = visitor::new(type_id.clone(), |type_id, _| type_id)
        .visit_composite(|type_id, _, fields| {
            // If exactly 1 unnamed field, recurse into it, else return current type ID.
            let Some(f) = fields.next() else {
                return type_id;
            };
            if fields.next().is_some() || f.name.is_some() {
                return type_id;
            };
            skip_through_single_unnamed_fields(f.id, types)
        })
        .visit_tuple(|type_id, type_ids| {
            // Else if exactly 1 tuple entry, recurse into it, else return current type ID.
            let Some(new_type_id) = type_ids.next() else {
                return type_id;
            };
            if type_ids.next().is_some() {
                return type_id;
            };
            skip_through_single_unnamed_fields(new_type_id, types)
        });

    types.resolve_type(type_id.clone(), v).unwrap_or(type_id)
}
