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

#[cfg(feature = "bits")]
mod bits;
mod composite;
#[cfg(feature = "primitive-types")]
mod primitive_types;
mod variant;

use crate::{
    error::{Error, ErrorKind, Kind},
    EncodeAsFields, EncodeAsType,
};
use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque},
    format,
    rc::Rc,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use codec::{Compact, Encode};
use core::{
    marker::PhantomData,
    num::{
        NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroU128, NonZeroU16,
        NonZeroU32, NonZeroU64, NonZeroU8,
    },
    ops::{Range, RangeInclusive},
    time::Duration,
};
use scale_type_resolver::{visitor, FieldIter, Primitive, ResolvedTypeVisitor, TypeResolver};

// Useful to help encode key-value types or custom variant types manually.
// Primarily used in the derive macro.
pub use composite::{Composite, CompositeField};
pub use variant::Variant;

fn resolve_type_and_encode<
    'resolver,
    R: TypeResolver,
    V: ResolvedTypeVisitor<'resolver, TypeId = R::TypeId, Value = Result<(), Error>>,
>(
    types: &'resolver R,
    type_id: R::TypeId,
    visitor: V,
) -> Result<(), Error> {
    match types.resolve_type(type_id, visitor) {
        Ok(res) => res,
        Err(e) => Err(Error::new(ErrorKind::TypeResolvingError(e.to_string()))),
    }
}

impl EncodeAsType for bool {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let type_id = find_single_entry_with_same_repr(type_id, types);

        let wrong_shape_err = |type_id| {
            Error::new(ErrorKind::WrongShape {
                actual: Kind::Bool,
                expected_id: format!("{type_id:?}"),
            })
        };

        let v = visitor::new(type_id.clone(), |type_id, _| Err(wrong_shape_err(type_id)))
            .visit_primitive(|type_id, primitive| {
                if primitive == Primitive::Bool {
                    self.encode_to(out);
                    Ok(())
                } else {
                    Err(wrong_shape_err(type_id))
                }
            })
            .visit_not_found(|type_id| {
                Err(Error::new(ErrorKind::TypeNotFound(format!("{type_id:?}"))))
            });

        resolve_type_and_encode(types, type_id, v)
    }
}

impl EncodeAsType for str {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let type_id = find_single_entry_with_same_repr(type_id, types);

        let wrong_shape_err = |type_id| {
            Error::new(ErrorKind::WrongShape {
                actual: Kind::Str,
                expected_id: format!("{type_id:?}"),
            })
        };

        let v = visitor::new(type_id.clone(), |type_id, _| Err(wrong_shape_err(type_id)))
            .visit_primitive(|type_id, primitive| {
                if primitive == Primitive::Str {
                    self.encode_to(out);
                    Ok(())
                } else {
                    Err(wrong_shape_err(type_id))
                }
            })
            .visit_not_found(|type_id| {
                Err(Error::new(ErrorKind::TypeNotFound(format!("{type_id:?}"))))
            });

        resolve_type_and_encode(types, type_id, v)
    }
}

impl<'a, T> EncodeAsType for &'a T
where
    T: EncodeAsType + ?Sized,
{
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        (*self).encode_as_type_to(type_id, types, out)
    }
}

impl<'a, T> EncodeAsType for alloc::borrow::Cow<'a, T>
where
    T: 'a + EncodeAsType + ToOwned + ?Sized,
{
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        (**self).encode_as_type_to(type_id, types, out)
    }
}

impl<T> EncodeAsType for [T]
where
    T: EncodeAsType,
{
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        encode_iterable_sequence_to(self.len(), self.iter(), type_id, types, out)
    }
}

impl<const N: usize, T: EncodeAsType> EncodeAsType for [T; N] {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self[..].encode_as_type_to(type_id, types, out)
    }
}

impl<T> EncodeAsType for PhantomData<T> {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        ().encode_as_type_to(type_id, types, out)
    }
}

impl<T: EncodeAsType, E: EncodeAsType> EncodeAsType for Result<T, E> {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self {
            Ok(v) => Variant {
                name: "Ok",
                fields: Composite::new([(None, CompositeField::new(v))].iter().copied()),
            }
            .encode_variant_as_type_to(type_id, types, out),
            Err(e) => Variant {
                name: "Err",
                fields: Composite::new([(None, CompositeField::new(e))].iter().copied()),
            }
            .encode_variant_as_type_to(type_id, types, out),
        }
    }
}

impl<T: EncodeAsType> EncodeAsType for Option<T> {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self {
            Some(v) => Variant {
                name: "Some",
                fields: Composite::new([(None, CompositeField::new(v))].iter().copied()),
            }
            .encode_variant_as_type_to(type_id, types, out),
            None => Variant {
                name: "None",
                fields: Composite::new([].iter().copied()),
            }
            .encode_variant_as_type_to(type_id, types, out),
        }
    }
}

// Encode any numeric type implementing ToNumber, above, into the type ID given.
macro_rules! impl_encode_number {
    ($ty:ty) => {
        impl EncodeAsType for $ty {
            fn encode_as_type_to<R: TypeResolver>(
                &self,
                type_id: R::TypeId,
                types: &R,
                out: &mut Vec<u8>,
            ) -> Result<(), Error> {
                let type_id = find_single_entry_with_same_repr(type_id, types);

                let wrong_shape_err = |type_id| {
                    Error::new(ErrorKind::WrongShape {
                        actual: Kind::Number,
                        expected_id: format!("{type_id:?}"),
                    })
                };

                let v = visitor::new((type_id.clone(), out), |(type_id, _out), _kind| Err(wrong_shape_err(type_id)))
                    .visit_primitive(|(type_id, out), primitive| {
                        fn try_num<T: TryFrom<$ty> + Encode>(
                            num: $ty,
                            target_id: impl core::fmt::Debug,
                            out: &mut Vec<u8>,
                        ) -> Result<(), Error> {
                            let n: T = num.try_into().map_err(|_| {
                                Error::new(ErrorKind::NumberOutOfRange {
                                    value: num.to_string(),
                                    expected_id: format!("{target_id:?}"),
                                })
                            })?;
                            n.encode_to(out);
                            Ok(())
                        }

                        match primitive {
                            Primitive::U8 => try_num::<u8>(*self, type_id, out),
                            Primitive::U16 => try_num::<u16>(*self, type_id, out),
                            Primitive::U32 => try_num::<u32>(*self, type_id, out),
                            Primitive::U64 => try_num::<u64>(*self, type_id, out),
                            Primitive::U128 => try_num::<u128>(*self, type_id, out),
                            Primitive::I8 => try_num::<i8>(*self, type_id, out),
                            Primitive::I16 => try_num::<i16>(*self, type_id, out),
                            Primitive::I32 => try_num::<i32>(*self, type_id, out),
                            Primitive::I64 => try_num::<i64>(*self, type_id, out),
                            Primitive::I128 => try_num::<i128>(*self, type_id, out),
                            _ => Err(wrong_shape_err(type_id)),
                        }
                    })
                    .visit_compact(|(_,out), inner_type_id| {
                        let inner_type_id = find_single_entry_with_same_repr(inner_type_id, types);

                        macro_rules! try_compact_num {
                            ($num:expr, $inner_type_id:ident, $target_kind:expr, $out:expr, $type:ty) => {{
                                let n: $type = $num.try_into().map_err(|_| {
                                    Error::new(ErrorKind::NumberOutOfRange {
                                        value: $num.to_string(),
                                        expected_id: format!("{:?}", $inner_type_id),
                                    })
                                })?;
                                Compact(n).encode_to($out);
                                Ok(())
                            }};
                        }

                        let v = visitor::new((inner_type_id.clone(),out), |(inner_type_id,_out), _| Err(wrong_shape_err(inner_type_id))).visit_primitive(
                            |(inner_type_id,out), primitive| match primitive {
                                Primitive::U8 => {
                                    try_compact_num!(*self, inner_type_id, NumericKind::U8, out, u8)
                                }
                                Primitive::U16 => {
                                    try_compact_num!(*self, inner_type_id, NumericKind::U16, out, u16)
                                }
                                Primitive::U32 => {
                                    try_compact_num!(*self, inner_type_id, NumericKind::U32, out, u32)
                                }
                                Primitive::U64 => {
                                    try_compact_num!(*self, inner_type_id, NumericKind::U64, out, u64)
                                }
                                Primitive::U128 => {
                                    try_compact_num!(*self, inner_type_id, NumericKind::U128, out, u128)
                                }
                                _ => Err(wrong_shape_err(inner_type_id)),
                            },
                        );

                        resolve_type_and_encode(types, inner_type_id, v)
                    })
                    .visit_not_found(|(type_id,_out)| {
                        Err(Error::new(ErrorKind::TypeNotFound(format!("{type_id:?}"))))
                    });

                resolve_type_and_encode(types, type_id, v)
            }
        }
    };
}
impl_encode_number!(u8);
impl_encode_number!(u16);
impl_encode_number!(u32);
impl_encode_number!(u64);
impl_encode_number!(u128);
impl_encode_number!(usize);
impl_encode_number!(i8);
impl_encode_number!(i16);
impl_encode_number!(i32);
impl_encode_number!(i64);
impl_encode_number!(i128);
impl_encode_number!(isize);

// Encode tuple types to any matching type.
macro_rules! impl_encode_tuple {
    ($($name:ident: $t:ident),*) => {
        impl < $($t),* > EncodeAsType for ($($t,)*) where $($t: EncodeAsType),* {
            fn encode_as_type_to<Resolver: TypeResolver>(&self, type_id: Resolver::TypeId, types: &Resolver, out: &mut Vec<u8>) -> Result<(), Error> {
                let ($($name,)*) = self;
                Composite::new([
                    $(
                        (None as Option<&'static str>, CompositeField::new($name))
                    ,)*
                ].iter().copied()).encode_composite_as_type_to(type_id, types, out)
            }
        }
    }
}
#[rustfmt::skip]
const _: () = {
    impl_encode_tuple!();
    impl_encode_tuple!(a: A);
    impl_encode_tuple!(a: A, b: B);
    impl_encode_tuple!(a: A, b: B, c: C);
    impl_encode_tuple!(a: A, b: B, c: C, d: D);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M, n: N);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M, n: N, o: O);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M, n: N, o: O, p: P);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M, n: N, o: O, p: P, q: Q);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M, n: N, o: O, p: P, q: Q, r: R);
    impl_encode_tuple!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L, m: M, n: N, o: O, p: P, q: Q, r: R, s: S);
    // ^ Note: We make sure to support as many as parity-scale-codec's Encode impls do.
};

// Implement encoding via iterators for ordered collections
macro_rules! impl_encode_seq_via_iterator {
    ($ty:ident $( [$($param:ident),+] )?) => {
        impl $(< $($param),+ >)? EncodeAsType for $ty $(< $($param),+ >)?
        where $( $($param: EncodeAsType),+ )?
        {
            fn encode_as_type_to<R: TypeResolver>(
                &self,
                type_id: R::TypeId,
                types: &R,
                out: &mut Vec<u8>,
            ) -> Result<(), Error> {
                encode_iterable_sequence_to(self.len(), self.iter(), type_id, types, out)
            }
        }
    }
}
impl_encode_seq_via_iterator!(BTreeSet[K]);
impl_encode_seq_via_iterator!(LinkedList[V]);
impl_encode_seq_via_iterator!(BinaryHeap[V]);
impl_encode_seq_via_iterator!(VecDeque[V]);
impl_encode_seq_via_iterator!(Vec[V]);

impl<K: AsRef<str>, V: EncodeAsType> EncodeAsType for BTreeMap<K, V> {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let v = visitor::new((type_id.clone(), out), |(type_id, out), _| {
            Composite::new(
                self.iter()
                    .map(|(k, v)| (Some(k.as_ref()), CompositeField::new(v))),
            )
            .encode_composite_as_type_to(type_id, types, out)
        })
        .visit_array(|(type_id, out), _, _| {
            encode_iterable_sequence_to(self.len(), self.values(), type_id, types, out)
        })
        .visit_sequence(|(type_id, out), _, _| {
            encode_iterable_sequence_to(self.len(), self.values(), type_id, types, out)
        });

        resolve_type_and_encode(types, type_id, v)
    }
}
impl<K: AsRef<str>, V: EncodeAsType> EncodeAsFields for BTreeMap<K, V> {
    fn encode_as_fields_to<R: TypeResolver>(
        &self,
        fields: &mut dyn FieldIter<'_, R::TypeId>,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        Composite::new(
            self.iter()
                .map(|(k, v)| (Some(k.as_ref()), CompositeField::new(v))),
        )
        .encode_composite_fields_to(fields, types, out)
    }
}

// Generate EncodeAsType impls for simple types that can be easily transformed
// into types we have impls for already.
macro_rules! impl_encode_like {
    ($ty:ident $(<$( $param:ident ),+>)? as $delegate_ty:ty where |$val:ident| $expr:expr) => {
        impl $(< $($param: EncodeAsType),+ >)? EncodeAsType for $ty $(<$( $param ),+>)? {
            fn encode_as_type_to<R: TypeResolver>(
                &self,
                type_id: R::TypeId,
                types: &R,
                out: &mut Vec<u8>,
            ) -> Result<(), Error> {
                let delegate: $delegate_ty = {
                    let $val = self;
                    $expr
                };
                delegate.encode_as_type_to(type_id, types, out)
            }
        }
    }
}
impl_encode_like!(String as &str where |val| val);
impl_encode_like!(Box<T> as &T where |val| val);
impl_encode_like!(Arc<T> as &T where |val| val);
impl_encode_like!(Rc<T> as &T where |val| val);
impl_encode_like!(char as u32 where |val| *val as u32);
impl_encode_like!(NonZeroU8 as u8 where |val| val.get());
impl_encode_like!(NonZeroU16 as u16 where |val| val.get());
impl_encode_like!(NonZeroU32 as u32 where |val| val.get());
impl_encode_like!(NonZeroU64 as u64 where |val| val.get());
impl_encode_like!(NonZeroU128 as u128 where |val| val.get());
impl_encode_like!(NonZeroI8 as i8 where |val| val.get());
impl_encode_like!(NonZeroI16 as i16 where |val| val.get());
impl_encode_like!(NonZeroI32 as i32 where |val| val.get());
impl_encode_like!(NonZeroI64 as i64 where |val| val.get());
impl_encode_like!(NonZeroI128 as i128 where |val| val.get());
impl_encode_like!(Duration as (u64, u32) where |val| (val.as_secs(), val.subsec_nanos()));
impl_encode_like!(Range<T> as (&T, &T) where |val| (&val.start, &val.end));
impl_encode_like!(RangeInclusive<T> as (&T, &T) where |val| ((val.start()), (val.end())));
impl_encode_like!(Compact<T> as &T where |val| &val.0);

// Generate EncodeAsField impls for common smart pointers containing
// types we have impls for already.
macro_rules! impl_encode_like_to_fields {
    ($ty:ident $(<$( $param:ident ),+>)? as $delegate_ty:ty where |$val:ident| $expr:expr) => {
        impl $(< $($param: EncodeAsFields),+ >)? EncodeAsFields for $ty $(<$( $param ),+>)? {
            fn encode_as_fields_to<R: TypeResolver>(
                &self,
                fields: &mut dyn FieldIter<'_, R::TypeId>,
                types: &R,
                out: &mut Vec<u8>,
            ) -> Result<(), Error> {
                self.as_ref().encode_as_fields_to(fields, types, out)
            }
        }
    }
}
impl_encode_like_to_fields!(Box<T> as &T where |val| val);
impl_encode_like_to_fields!(Rc<T> as &T where |val| val);
impl_encode_like_to_fields!(Arc<T> as &T where |val| val);

// Attempt to recurse into some type, returning the innermost type found that has an identical
// SCALE encoded representation to the given type. For instance, `(T,)` encodes identically to
// `T`, as does `Mytype { inner: T }` or `[T; 1]`.
fn find_single_entry_with_same_repr<R: TypeResolver>(type_id: R::TypeId, types: &R) -> R::TypeId {
    let v = visitor::new(type_id.clone(), |type_id, _| type_id)
        .visit_tuple(|type_id, fields| {
            let Some(new_type_id) = fields.next() else {
                return type_id;
            };
            if fields.next().is_some() {
                return type_id;
            }
            find_single_entry_with_same_repr(new_type_id, types)
        })
        .visit_composite(|type_id, _, fields| {
            let Some(field) = fields.next() else {
                return type_id;
            };
            if fields.next().is_some() {
                return type_id;
            }
            find_single_entry_with_same_repr(field.id, types)
        });

    types.resolve_type(type_id.clone(), v).unwrap_or(type_id)
}

// Encode some iterator of items to the type provided.
fn encode_iterable_sequence_to<I, R>(
    len: usize,
    it: I,
    type_id: R::TypeId,
    types: &R,
    out: &mut Vec<u8>,
) -> Result<(), Error>
where
    I: Iterator,
    I::Item: EncodeAsType,
    R: TypeResolver,
{
    let wrong_shape_err = |type_id| {
        Error::new(ErrorKind::WrongShape {
            actual: Kind::Array,
            expected_id: format!("{type_id:?}"),
        })
    };

    let v = visitor::new((type_id.clone(), it, out), |(type_id, _, _), _| {
        Err(wrong_shape_err(type_id))
    })
    .visit_array(|(_, it, out), inner_ty_id: R::TypeId, array_len| {
        if array_len == len {
            for (idx, item) in it.enumerate() {
                item.encode_as_type_to(inner_ty_id.clone(), types, out)
                    .map_err(|e| e.at_idx(idx))?;
            }
            Ok(())
        } else {
            Err(Error::new(ErrorKind::WrongLength {
                actual_len: len,
                expected_len: array_len,
            }))
        }
    })
    .visit_sequence(|(_, it, out), _, inner_ty_id| {
        // Sequences are prefixed with their compact encoded length:
        Compact(len as u32).encode_to(out);
        for (idx, item) in it.enumerate() {
            item.encode_as_type_to(inner_ty_id.clone(), types, out)
                .map_err(|e| e.at_idx(idx))?;
        }
        Ok(())
    })
    .visit_tuple(|(type_id, it, out), inner_type_ids| {
        if inner_type_ids.len() == 1 {
            encode_iterable_sequence_to(len, it, inner_type_ids.next().unwrap(), types, out)
        } else {
            Err(wrong_shape_err(type_id))
        }
    })
    .visit_composite(|(type_id, it, out), _, fields| {
        if fields.len() == 1 {
            encode_iterable_sequence_to(len, it, fields.next().unwrap().id, types, out)
        } else {
            Err(wrong_shape_err(type_id))
        }
    });

    resolve_type_and_encode(types, type_id, v)
}

#[cfg(all(feature = "derive", feature = "bits", feature = "primitive-types"))]
#[cfg(test)]
mod test {
    use super::*;
    use crate::{EncodeAsFields, Field};
    use alloc::vec;
    use codec::Decode;
    use core::fmt::Debug;
    use scale_info::{PortableRegistry, TypeInfo};

    /// Given a type definition, return type ID and registry representing it.
    fn make_type<T: TypeInfo + 'static>() -> (u32, PortableRegistry) {
        let m = scale_info::MetaType::new::<T>();
        let mut types = scale_info::Registry::new();
        let id = types.register_type(&m);
        let portable_registry: PortableRegistry = types.into();

        (id.id, portable_registry)
    }

    fn encode_type<V: EncodeAsType, T: TypeInfo + 'static>(value: V) -> Result<Vec<u8>, Error> {
        let (type_id, types) = make_type::<T>();
        let bytes = value.encode_as_type(type_id, &types)?;
        Ok(bytes)
    }

    fn assert_value_roundtrips_to<
        V: EncodeAsType,
        T: PartialEq + Debug + Decode + TypeInfo + 'static,
    >(
        value: V,
        target: T,
    ) {
        let bytes = encode_type::<_, T>(&value).expect("can encode");
        let bytes_cursor = &mut &*bytes;
        let new_target = T::decode(bytes_cursor).expect("can decode");

        assert_eq!(bytes_cursor.len(), 0, "no bytes should be remaining");
        assert_eq!(
            target, new_target,
            "value does not roundtrip and decode to target"
        );
    }

    fn assert_encodes_like_codec<
        V: Encode + EncodeAsType + PartialEq + Debug + TypeInfo + 'static,
    >(
        value: V,
    ) {
        let encode_bytes = value.encode();
        let bytes = encode_type::<V, V>(value).expect("can encode");
        assert_eq!(
            bytes, encode_bytes,
            "scale-encode encoded differently from parity-scale-codec"
        );
    }

    fn assert_encodes_fields_like_type<V: EncodeAsFields, T: TypeInfo + Encode + 'static>(
        value: V,
        other: T,
    ) {
        let encoded_other = other.encode();

        let (type_id, types) = make_type::<T>();
        let type_def = &types.resolve(type_id).unwrap().type_def;

        let encoded_as_fields = match type_def {
            scale_info::TypeDef::Composite(c) => {
                let mut fields = c
                    .fields
                    .iter()
                    .map(|f| Field::new(f.ty.id, f.name.as_deref()));
                value.encode_as_fields(&mut fields, &types).unwrap()
            }
            scale_info::TypeDef::Tuple(t) => {
                let mut fields = t.fields.iter().map(|f| Field::unnamed(f.id));
                value.encode_as_fields(&mut fields, &types).unwrap()
            }
            _ => {
                panic!("Expected composite or tuple type def");
            }
        };

        assert_eq!(
            encoded_other, encoded_as_fields,
            "compare encode_with_fields with other encode"
        );
    }

    #[test]
    fn numeric_roundtrips_encode_ok() {
        macro_rules! int_value_roundtrip {
            ($($val:expr; $ty:ty),+) => {$(
                assert_value_roundtrips_to($val, $val as i8);
                assert_value_roundtrips_to($val, $val as i16);
                assert_value_roundtrips_to($val, $val as i32);
                assert_value_roundtrips_to($val, $val as i64);
                assert_value_roundtrips_to($val, $val as i128);
            )+}
        }
        macro_rules! uint_value_roundtrip {
            ($($val:expr; $ty:ty),+) => {$(
                assert_value_roundtrips_to($val, $val as u8);
                assert_value_roundtrips_to($val, $val as u16);
                assert_value_roundtrips_to($val, $val as u32);
                assert_value_roundtrips_to($val, $val as u64);
                assert_value_roundtrips_to($val, $val as u128);
            )+}
        }
        macro_rules! int_value_roundtrip_types {
            ($($val:expr),+) => {$(
                int_value_roundtrip!($val; i8);
                int_value_roundtrip!($val; i16);
                int_value_roundtrip!($val; i32);
                int_value_roundtrip!($val; i64);
                int_value_roundtrip!($val; i128);
            )+}
        }
        macro_rules! uint_value_roundtrip_types {
            ($($val:expr),+) => {$(
                uint_value_roundtrip!($val; u8);
                uint_value_roundtrip!($val; u16);
                uint_value_roundtrip!($val; u32);
                uint_value_roundtrip!($val; u64);
                uint_value_roundtrip!($val; u128);
            )+}
        }
        macro_rules! all_value_roundtrip_types {
            ($($val:expr),+) => {$(
                int_value_roundtrip_types!($val);
                uint_value_roundtrip_types!($val);
            )+}
        }
        uint_value_roundtrip_types!(200);
        int_value_roundtrip_types!(-127, -100, 0, 1, 100, 127);
        all_value_roundtrip_types!(0, 1, 100, 127);
    }

    #[test]
    fn out_of_range_numeric_roundtrips_fail_to_encode() {
        encode_type::<_, u8>(&1234u16).unwrap_err();
        encode_type::<_, i8>(&129u8).unwrap_err();
        encode_type::<_, u8>(&-10i8).unwrap_err();
    }

    #[test]
    fn sequence_encodes_like_scale_codec() {
        let (type_id, types) = make_type::<Vec<u8>>();
        let e = vec![1u8, 2, 3].encode();
        let e2 = vec![1u8, 2, 3]
            .encode_as_type(type_id, &types)
            .expect("can encode 2");
        assert_eq!(e, e2);
    }

    #[test]
    fn basic_types_encode_like_scale_codec() {
        assert_encodes_like_codec(true);
        assert_encodes_like_codec(false);
        assert_encodes_like_codec("hi");
        assert_encodes_like_codec("hi".to_string());
        assert_encodes_like_codec(Box::new("hi"));
        assert_encodes_like_codec(-1234);
        assert_encodes_like_codec(100_000_000_000_000u128);
        assert_encodes_like_codec(());
        assert_encodes_like_codec(core::marker::PhantomData::<()>);
        assert_encodes_like_codec([1, 2, 3, 4, 5]);
        assert_encodes_like_codec([1u8, 2, 3, 4, 5]);
        assert_encodes_like_codec(vec![1, 2, 3, 4, 5]);
        assert_encodes_like_codec([1, 2, 3, 4, 5]);
        assert_encodes_like_codec(Some(1234u32));
        assert_encodes_like_codec(None as Option<bool>);
        assert_encodes_like_codec(Ok::<_, &str>("hello"));
        assert_encodes_like_codec(Err::<u32, _>("aah"));
        assert_encodes_like_codec(0..100);
        assert_encodes_like_codec(0..=100);

        // These don't impl TypeInfo so we have to provide the target type to encode to & compare with:
        assert_value_roundtrips_to(Arc::new("hi"), "hi".to_string());
        assert_value_roundtrips_to(Rc::new("hi"), "hi".to_string());
        // encodes_like_codec(core::time::Duration::from_millis(123456));
    }

    #[test]
    fn other_container_types_roundtrip_ok() {
        // These things don't have TypeInfo impls, and so we just assume that they should
        // encode like any sequence, prefixed with length.

        let v = LinkedList::from([1u8, 2, 3]);
        assert_value_roundtrips_to(v, vec![1u8, 2, 3]);

        // (it's a max heap, so values ordered max first.)
        let v = BinaryHeap::from([2, 3, 1]);
        assert_value_roundtrips_to(v, vec![3u8, 2, 1]);

        let v = BTreeSet::from([1u8, 2, 3]);
        assert_value_roundtrips_to(v, vec![1u8, 2, 3]);

        let v = VecDeque::from([1u8, 2, 3]);
        assert_value_roundtrips_to(v, vec![1u8, 2, 3]);
    }

    #[test]
    fn btreemap_can_encode_to_struct() {
        #[derive(Debug, scale_info::TypeInfo, codec::Decode, PartialEq)]
        struct Foo {
            a: u8,
            b: u16,
            c: u32,
        }

        let v = BTreeMap::from([("a", 1), ("c", 2), ("b", 3)]);

        // BTreeMap can go to a key-val composite, or unnamed:
        assert_value_roundtrips_to(v.clone(), Foo { a: 1, b: 3, c: 2 });
        // BTreeMaps are iterated in order of key:
        assert_value_roundtrips_to(v, (1, 3, 2));
    }

    #[test]
    fn mixed_tuples_roundtrip_ok() {
        assert_encodes_like_codec(());
        assert_encodes_like_codec((12345,));
        assert_encodes_like_codec((123u8, true));
        assert_encodes_like_codec((123u8, true, "hello"));
        // Encode isn't implemented for `char` (but we treat it as a u32):
        assert_encodes_like_codec((123u8, true, "hello".to_string(), 'a' as u32));
        assert_encodes_like_codec((
            123u8,
            true,
            "hello".to_string(),
            'a' as u32,
            123_000_000_000u128,
        ));
    }

    #[test]
    fn sequences_roundtrip_into_eachother() {
        // Nesting can be resolved (but tuples and sequences are distinct)
        assert_value_roundtrips_to(([1u8, 2u8, 3u8],), vec![1u8, 2u8, 3u8]);
        assert_value_roundtrips_to(([(1u8,), (2u8,), (3u8,)],), (([1u8, 2u8, 3u8],),));
        assert_value_roundtrips_to(((([1u8],),),), (([1u8],),));
        assert_value_roundtrips_to((([(1u8,)],),), (([1u8],),));
    }

    #[test]
    fn tuples_to_structs() {
        #[derive(Debug, scale_info::TypeInfo, codec::Decode, PartialEq)]
        struct Foo {
            a: (u32,),
            b: u64,
            c: u128,
        }
        assert_value_roundtrips_to(
            (1u8, 2u8, 3u8),
            Foo {
                a: (1,),
                b: 2,
                c: 3,
            },
        );
    }

    #[test]
    fn values_roundtrip_into_wrappers() {
        #[derive(Debug, scale_info::TypeInfo, codec::Decode, PartialEq)]
        struct Wrapper<T> {
            val: T,
        }

        assert_value_roundtrips_to(true, (true,));
        assert_value_roundtrips_to(1234u16, (1234u16,));
        assert_value_roundtrips_to(1234u16, Wrapper { val: 1234u16 });
        assert_value_roundtrips_to("hi", (("hi".to_string(),),));
        assert_value_roundtrips_to(
            "hi",
            (Wrapper {
                val: "hi".to_string(),
            },),
        );
    }

    #[test]
    fn compacts_roundtrip() {
        assert_encodes_like_codec(Compact(123u16));
        assert_encodes_like_codec(Compact(123u8));
        assert_encodes_like_codec(Compact(123u64));
    }

    #[test]
    fn tuple_composite_can_encode_to_named_structs() {
        #[derive(Debug, scale_info::TypeInfo, codec::Decode, PartialEq)]
        struct Foo {
            bar: u32,
            wibble: bool,
            hello: String,
        }

        // note: fields do not need to be in order when named:
        let source_vals = [
            (Some("hello"), CompositeField::new(&"world")),
            (Some("bar"), CompositeField::new(&12345u128)),
            (Some("wibble"), CompositeField::new(&true)),
        ];
        let source = Composite::new(source_vals.iter().copied());

        // Composite can't implement `EncodeAsType` and so need "manually" encoding:
        let (type_id, types) = make_type::<Foo>();
        let bytes = source.encode_composite_as_type(type_id, &types).unwrap();
        let cursor = &mut &*bytes;

        let target = Foo {
            bar: 12345,
            wibble: true,
            hello: "world".to_string(),
        };

        let new_target = Foo::decode(cursor).unwrap();

        assert_eq!(target, new_target);
        assert_eq!(cursor.len(), 0);
    }

    #[test]
    fn tuple_composite_can_encode_to_unnamed_structs() {
        #[derive(Debug, scale_info::TypeInfo, codec::Decode, PartialEq, Clone)]
        struct Foo(u32, bool, String);
        let (type_id, types) = make_type::<Foo>();

        // note: unnamed target so fields need to be in order (can be named or not)
        let source_vals = [
            (Some("bar"), CompositeField::new(&12345u128)),
            (Some("wibble"), CompositeField::new(&true)),
            (Some("hello"), CompositeField::new(&"world")),
        ];
        let source = Composite::new(source_vals.iter().copied());
        let source_bytes = source.encode_composite_as_type(type_id, &types).unwrap();
        let source_cursor = &mut &*source_bytes;

        let source2_vals = [
            (None, CompositeField::new(&12345u128)),
            (None, CompositeField::new(&true)),
            (None, CompositeField::new(&"world")),
        ];
        let source2 = Composite::new(source2_vals.iter().copied());
        let source2_bytes = source2.encode_composite_as_type(type_id, &types).unwrap();
        let source2_cursor = &mut &*source2_bytes;

        let target = Foo(12345, true, "world".to_string());
        let new_target = Foo::decode(source_cursor).unwrap();
        let new_target2 = Foo::decode(source2_cursor).unwrap();

        assert_eq!(target, new_target);
        assert_eq!(target, new_target2);
        assert_eq!(source_cursor.len(), 0);
        assert_eq!(source2_cursor.len(), 0);
    }

    #[test]
    fn tuple_composite_names_must_line_up() {
        #[derive(Debug, scale_info::TypeInfo, codec::Decode, PartialEq)]
        struct Foo {
            bar: u32,
            wibble: bool,
            hello: String,
        }

        // note: fields do not need to be in order when named:
        let source_vals = [
            (Some("hello"), CompositeField::new(&"world")),
            (Some("bar"), CompositeField::new(&12345u128)),
            // wrong name:
            (Some("wibbles"), CompositeField::new(&true)),
        ];
        let source = Composite::new(source_vals.iter().copied());

        let (type_id, types) = make_type::<Foo>();
        let _bytes = source
            .encode_composite_as_type(type_id, &types)
            .unwrap_err();
    }

    #[test]
    fn bits_roundtrip_ok() {
        use bitvec::{
            order::{Lsb0, Msb0},
            vec::BitVec,
        };
        use scale_bits::Bits;

        fn test_bits(bits: impl IntoIterator<Item = bool> + Clone) {
            let source = Bits::from_iter(bits.clone());

            let target = BitVec::<u8, Lsb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u16, Lsb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u32, Lsb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u64, Lsb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u8, Msb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u16, Msb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u32, Msb0>::from_iter(bits.clone());
            assert_value_roundtrips_to(source.clone(), target);
            let target = BitVec::<u64, Msb0>::from_iter(bits);
            assert_value_roundtrips_to(source, target);
        }

        test_bits([]);
        test_bits([true]);
        test_bits([false]);
        test_bits([true, false, true, true, false]);
        test_bits([
            true, false, true, true, false, true, false, true, true, false, false,
        ]);

        // Wrapping the input or output bitvecs is fine; it'll figure it out:
        assert_value_roundtrips_to(
            Bits::from_iter([true, false, true]),
            ((BitVec::<u8, Lsb0>::from_iter([true, false, true]),),),
        );
        assert_value_roundtrips_to(
            (Bits::from_iter([true, false, true]),),
            ((BitVec::<u8, Lsb0>::from_iter([true, false, true]),),),
        );
    }

    #[test]
    fn hxxx_types_roundtrip_ok() {
        use ::primitive_types::{H128, H160, H256, H384, H512, H768};

        // Check that Hxxx types roundtirp to themselves or to byte sequences
        fn test_hxxx(bytes: impl IntoIterator<Item = u8> + Clone) {
            let mut bytes: Vec<u8> = bytes.into_iter().collect();

            while bytes.len() < 128 / 8 {
                bytes.push(0)
            }
            assert_value_roundtrips_to(H128::from_slice(&bytes), bytes.clone());
            assert_value_roundtrips_to(H128::from_slice(&bytes), H128::from_slice(&bytes));

            while bytes.len() < 160 / 8 {
                bytes.push(0)
            }
            assert_value_roundtrips_to(H160::from_slice(&bytes), bytes.clone());
            assert_value_roundtrips_to(H160::from_slice(&bytes), H160::from_slice(&bytes));

            while bytes.len() < 256 / 8 {
                bytes.push(0)
            }
            assert_value_roundtrips_to(H256::from_slice(&bytes), bytes.clone());
            assert_value_roundtrips_to(H256::from_slice(&bytes), H256::from_slice(&bytes));

            while bytes.len() < 384 / 8 {
                bytes.push(0)
            }
            assert_value_roundtrips_to(H384::from_slice(&bytes), bytes.clone());
            assert_value_roundtrips_to(H384::from_slice(&bytes), H384::from_slice(&bytes));

            while bytes.len() < 512 / 8 {
                bytes.push(0)
            }
            assert_value_roundtrips_to(H512::from_slice(&bytes), bytes.clone());
            assert_value_roundtrips_to(H512::from_slice(&bytes), H512::from_slice(&bytes));

            while bytes.len() < 768 / 8 {
                bytes.push(0)
            }
            assert_value_roundtrips_to(H768::from_slice(&bytes), bytes.clone());
            assert_value_roundtrips_to(H768::from_slice(&bytes), H768::from_slice(&bytes));
        }

        test_hxxx([0u8]);
        test_hxxx([1, 2, 3, 4]);
    }

    #[test]
    fn encode_as_fields_works() {
        #[derive(TypeInfo, Encode)]
        struct Foo {
            some_field: u64,
            another: u8,
        }

        assert_encodes_fields_like_type(
            BTreeMap::from([
                ("other1", 1),
                ("another", 2),
                ("some_field", 3),
                ("other2", 4),
            ]),
            Foo {
                some_field: 3,
                another: 2,
            },
        )
    }

    #[test]
    fn encode_as_fields_via_macro_works() {
        #[derive(TypeInfo, Encode)]
        struct Foo {
            some_field: u64,
            another: bool,
        }

        #[derive(TypeInfo, Encode)]
        struct FooUnnamed(
            String,
            (u8,), // different types still map ok.
            bool,
            u8,
        );

        #[derive(EncodeAsType)]
        #[encode_as_type(crate_path = "crate")]
        struct FooBigger {
            random: String,
            some_field: u64,
            another: bool,
            more_random: u8,
        }

        assert_encodes_fields_like_type(
            FooBigger {
                random: "hello".to_string(),
                some_field: 123,
                another: true,
                more_random: 1,
            },
            Foo {
                some_field: 123,
                another: true,
            },
        );
        assert_encodes_fields_like_type(
            FooBigger {
                random: "hello".to_string(),
                some_field: 123,
                another: true,
                more_random: 1,
            },
            FooUnnamed("hello".to_string(), (123,), true, 1),
        );
        assert_encodes_fields_like_type(
            FooBigger {
                random: "hello".to_string(),
                some_field: 123,
                another: true,
                more_random: 1,
            },
            ("hello".to_string(), (123u8,), true, (1u64,)),
        );
    }

    #[test]
    fn encode_to_number_skipping_attrs_via_macro_works() {
        struct NotEncodeAsType;

        #[allow(dead_code)]
        #[derive(EncodeAsType)]
        #[encode_as_type(crate_path = "crate")]
        struct FooNotSkipping {
            value: u64,
            other: bool,
            third: String,
        }

        #[derive(EncodeAsType)]
        #[encode_as_type(crate_path = "crate")]
        struct FooSkipping {
            value: u64,
            #[encode_as_type(skip)]
            other: bool,
            // Even though this type doesn't impl EncodeAsType,
            // it's ignored so should be fine:
            #[codec(skip)]
            third: NotEncodeAsType,
        }

        assert_value_roundtrips_to(
            FooSkipping {
                value: 123,
                other: true,
                third: NotEncodeAsType,
            },
            123u64,
        );
    }

    #[test]
    fn encode_unnamed_to_number_skipping_attrs_via_macro_works() {
        struct NotEncodeAsType;

        #[derive(EncodeAsType)]
        #[encode_as_type(crate_path = "crate")]
        struct FooSkipping(
            u64,
            #[encode_as_type(skip)] bool,
            // Even though this type doesn't impl EncodeAsType,
            // it's ignored so should be fine:
            #[codec(skip)] NotEncodeAsType,
        );

        assert_value_roundtrips_to(FooSkipping(123, true, NotEncodeAsType), 123u64);
    }

    // If you don't skip values, you can't turn a multi-value
    // struct into a number.
    #[test]
    #[should_panic]
    fn encode_to_number_not_skipping_via_macro_fails() {
        #[derive(EncodeAsType)]
        #[encode_as_type(crate_path = "crate")]
        struct FooNotSkipping {
            value: u64,
            other: bool,
            third: String,
        }

        assert_value_roundtrips_to(
            FooNotSkipping {
                value: 123,
                other: true,
                third: "hello".to_string(),
            },
            123u64,
        );
    }

    #[test]
    fn encode_smart_pointers_as_fields() {
        #[derive(TypeInfo, Encode, PartialEq, Debug, Decode)]
        struct Foo {
            some_field: u64,
            another: u8,
        }

        let map = BTreeMap::from([
            ("other1", 1),
            ("another", 2),
            ("some_field", 3),
            ("other2", 4),
        ]);
        let a = Box::new(map.clone());
        assert_encodes_fields_like_type(
            a.clone(),
            Foo {
                some_field: 3,
                another: 2,
            },
        );
        assert_value_roundtrips_to(
            &a,
            Box::new(Foo {
                some_field: 3,
                another: 2,
            }),
        );
        let b = Rc::new(map.clone());
        assert_encodes_fields_like_type(
            b.clone(),
            Foo {
                some_field: 3,
                another: 2,
            },
        );
        assert_value_roundtrips_to(
            &b,
            Rc::new(Foo {
                some_field: 3,
                another: 2,
            }),
        );
        let c = Arc::new(map.clone());
        assert_encodes_fields_like_type(
            c.clone(),
            Foo {
                some_field: 3,
                another: 2,
            },
        );
        assert_value_roundtrips_to(
            &c,
            Arc::new(Foo {
                some_field: 3,
                another: 2,
            }),
        );
    }
}
