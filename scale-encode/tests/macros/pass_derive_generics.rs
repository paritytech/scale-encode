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

use scale_encode::EncodeAsType;

struct NotEncodeAsType;

// Enums with generic params can impl EncodeAsType.
#[derive(EncodeAsType)]
enum Bar<T, U, V> {
    Wibble(bool, T, U, V),
    Wobble,
}

// This impls EncodeAsType ok; we set no default trait bounds.
#[derive(EncodeAsType)]
#[encode_as_type(trait_bounds = "")]
enum NoTraitBounds<T> {
    Wibble(core::marker::PhantomData<T>),
}

// Structs (and const bounds) impl EncodeAsType OK.
#[derive(EncodeAsType)]
struct MyStruct<const V: usize, Bar: Clone + PartialEq> {
    array: [Bar; V],
}

fn can_encode_as_type<T: EncodeAsType>() {}

fn main() {
    // assert that the trait is implemented as expected:
    can_encode_as_type::<Bar<u8, String, bool>>();
    can_encode_as_type::<NoTraitBounds<NotEncodeAsType>>();
    can_encode_as_type::<MyStruct<16, u64>>();
}
