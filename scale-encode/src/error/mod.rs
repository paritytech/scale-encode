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

//! An error that is emitted whenever some encoding fails.
mod context;

use std::borrow::Cow;
use std::fmt::Display;

pub use context::{Context, Location};

/// An error produced while attempting to encode some type.
#[derive(Debug, thiserror::Error)]
pub struct Error {
    context: Context,
    kind: ErrorKind,
}

impl Error {
    /// Construct a new error given an error kind.
    pub fn new(kind: ErrorKind) -> Error {
        Error {
            context: Context::new(),
            kind,
        }
    }
    /// Construct a new, custom error.
    pub fn custom(error: impl Into<CustomError>) -> Error {
        Error::new(ErrorKind::Custom(error.into()))
    }
    /// Retrieve more information about what went wrong.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
    /// Retrieve details about where the error occurred.
    pub fn context(&self) -> &Context {
        &self.context
    }
    /// Give some context to the error.
    pub fn at(mut self, loc: Location) -> Self {
        self.context.push(loc);
        Error {
            context: self.context,
            kind: self.kind,
        }
    }
    /// Note which sequence index the error occurred in.
    pub fn at_idx(mut self, idx: usize) -> Self {
        self.context.push(Location::idx(idx));
        Error {
            context: self.context,
            kind: self.kind,
        }
    }
    /// Note which field the error occurred in.
    pub fn at_field(mut self, field: impl Into<Cow<'static, str>>) -> Self {
        self.context.push(Location::field(field));
        Error {
            context: self.context,
            kind: self.kind,
        }
    }
    /// Note which variant the error occurred in.
    pub fn at_variant(mut self, variant: impl Into<Cow<'static, str>>) -> Self {
        self.context.push(Location::variant(variant));
        Error {
            context: self.context,
            kind: self.kind,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.context.path();
        let kind = &self.kind;
        write!(f, "Error at {path}: {kind}")
    }
}

/// The underlying nature of the error.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    /// Cannot find a given type.
    #[error("Cannot find type with ID {0}")]
    TypeNotFound(u32),
    /// Cannot encode the actual type given into the target type ID.
    #[error("Cannot encode {actual:?} into type with ID {expected}")]
    WrongShape {
        /// The actual kind we have to encode
        actual: Kind,
        /// ID of the expected type.
        expected: u32,
    },
    /// The types line up, but the expected length of the target type is different from the length of the input value.
    #[error("Cannot encode to type; expected length {expected_len} but got length {actual_len}")]
    WrongLength {
        /// Length we have
        actual_len: usize,
        /// Length expected for type.
        expected_len: usize,
    },
    /// We cannot encode the number given into the target type; it's out of range.
    #[error("Number {value} is out of range for target type {expected}")]
    NumberOutOfRange {
        /// A string represenatation of the numeric value that was out of range.
        value: String,
        /// Id of the expected numeric type that we tried to encode it to.
        expected: u32,
    },
    /// Cannot find a variant with a matching name on the target type.
    #[error("Variant {name} does not exist on type with ID {expected}")]
    CannotFindVariant {
        /// Variant name we can't find in the expected type.
        name: String,
        /// ID of the expected type.
        expected: u32,
    },
    /// Cannot find a field on our source type that's needed for the target type.
    #[error("Field {name} does not exist in our source struct")]
    CannotFindField {
        /// Name of the field which was not provided.
        name: String,
    },
    /// A custom error.
    #[error("Custom error: {0}")]
    Custom(CustomError),
}

type CustomError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The kind of type that we're trying to encode.
#[allow(missing_docs)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Kind {
    Struct,
    Tuple,
    Variant,
    Array,
    BitSequence,
    Bool,
    Char,
    Str,
    Number,
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(thiserror::Error, Debug)]
    enum MyError {
        #[error("Foo!")]
        Foo,
    }

    #[test]
    fn custom_error() {
        // Just a compile-time check that we can ergonomically provide an arbitrary custom error:
        Error::custom(MyError::Foo);
    }
}
