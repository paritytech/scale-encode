# Changelog

The format is based on [Keep a Changelog].

[Keep a Changelog]: http://keepachangelog.com/en/1.0.0/

## [v0.7.1] - 2024-05-17

- Implement EncodeAsFields for pointer types like Arc and Box([#22](https://github.com/paritytech/scale-encode/pull/22))


## [v0.7.0] - 2024-04-29

Update the `scale-type-resolver` dependency to 0.2.0 (and bump `scale-bits` for the same reason).

The main change here is that type IDs are now passed by value, rather than reference.

## [v0.6.0] - 2024-02-16

Up until now, `scale-info` has been the library that gives us the information needed to know how to SCALE encode values to the correct shape. In this release, we remove it from our dependency tree and replace it with `scale-type-resolver`, which provides a generic `TypeResolver` trait whose implementations are able to provide the information needed to encode/decode types. So now, rather than taking in a `scale_info::PortableRegistry`, the `EncodeAsType` and `EncodeAsFields` traits take a generic `R: scale_type_resolver::TypeResolver` value. `scale_info::PortableRegistry` implements `TypeResolver`, and so it can continue to be used similarly to before (though now, `type_id` is passed as a reference), but now we are generic over where the type information we need comes from.

To be more concrete, `EncodeAsType` used to look roughly like this:

```rust
pub trait EncodeAsType {
    fn encode_as_type_to(
        &self,
        type_id: u32,
        types: scale_info::PortableRegistry,
        out: &mut Vec<u8>,
    ) -> Result<(), Error>;
}
```

And now it looks like this:

```rust
pub trait EncodeAsType {
    fn encode_as_type_to<R: TypeResolver>(
        &self,
        type_id: &R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error>;
}
```

One effect that this has is that `EncodeAsType` and `EncodeAsFields` are no longer object safe (since the method they expose accepts a generic type now). Internally this led us to also change how `scale_encode::Composite` works slightly (see the docs for that for more information). if you need object safety, and know the type resolver that you want to use, then you can make a trait + blanket impl like this which _is_ object safe and is implemented for anything which implements `EncodeAsType`:

```rust
trait EncodeAsTypeWithResolver<R: TypeResolver> {
    fn encode_as_type_with_resolver_to(
        &self,
        type_id: &R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error>;
}
impl<T: EncodeAsType, R: TypeResolver> EncodeAsTypeWithResolver<R> for T {
    fn encode_as_type_with_resolver_to(
        &self,
        type_id: &R::TypeId,
        types: &R,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self.encode_as_type_to(type_id, types, out)
    }
}
```

We can now have `&dyn EncodeAsTypeWithResolver<SomeConcreteResolver>` instances.

The full PR is here:

- Enable generic type encoding via TypeResolver and remove dependency on scale-info ([#19](https://github.com/paritytech/scale-encode/pull/19)).

## [v0.5.0] - 2023-08-02

- Improve custom error handling: custom errors now require `Debug + Display` on `no_std` or `Error` on `std`.
  `Error::custom()` now accepts anything implementing these traits rather than depending on `Into<Error>`
  ([#13](https://github.com/paritytech/scale-encode/pull/13)).
- Enable using `#[codec(skip)]` or `#[encode_as_type(skip)]` to ignore fields when using the `EncodeAsType` macro.
  Skipping isn't generally necessary, but can be useful in edge cases (such as allowing a multi-field struct to be
  encoded to a number if all but one numeric field is skipped) ([#16](https://github.com/paritytech/scale-encode/pull/16)).

## [v0.4.0] - 2023-07-11

- Add support for `no_std` (+alloc) builds ([#11](https://github.com/paritytech/scale-encode/pull/11)). Thankyou @haerdib!

## [v0.3.0] - 2023-05-31

- Remove the generic iterator from `EncodeAsFields` and ensure that it's object safe. Use a `&mut dyn` iterator instead.

## [v0.2.0] - 2023-05-26

- Update `scale-info` to latest, removing deprecated method calls.
- Change `EncodeAsFields` to accept an iterator over fields, to allow more flexibility in how we provide fields to encode.

## [v0.1.2] - 2023-03-01

Fix a silly typo in the `scale-encode-derive` README.

## [v0.1.1] - 2023-03-01

Improve the documentation with more examples, and tweak BTreeMap encoding and Composite encoding to be more tolerant.

## [v0.1.0] - 2023-02-28

Initial release.
