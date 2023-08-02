# Changelog

The format is based on [Keep a Changelog].

[Keep a Changelog]: http://keepachangelog.com/en/1.0.0/

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
