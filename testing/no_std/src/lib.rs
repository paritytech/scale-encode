#![no_std]
extern crate alloc;

use scale_encode::EncodeAsType;

pub struct NotEncodeAsType;

// Enums with generic params can impl EncodeAsType.
#[derive(EncodeAsType)]
pub enum Bar<T, U, V> {
    Wibble(bool, T, U, V),
    Wobble,
}

// This impls EncodeAsType ok; we set no default trait bounds.
#[derive(EncodeAsType)]
#[encode_as_type(trait_bounds = "")]
pub enum NoTraitBounds<T> {
    Wibble(core::marker::PhantomData<T>),
}

// Structs (and const bounds) impl EncodeAsType OK.
#[derive(EncodeAsType)]
pub struct MyStruct<const V: usize, Bar: Clone + PartialEq> {
    array: [Bar; V],
}

pub fn can_encode_as_type<T: EncodeAsType>() {}
