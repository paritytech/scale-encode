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

use alloc::{borrow::Cow, boxed::Box, string::String};
use core::fmt::Display;

pub use context::{Context, Location};

/// An error produced while attempting to encode some type.
#[derive(Debug)]
pub struct Error {
    context: Context,
    kind: ErrorKind,
}

impl core::error::Error for Error {}

impl Error {
    /// Construct a new error given an error kind.
    pub fn new(kind: ErrorKind) -> Error {
        Error {
            context: Context::new(),
            kind,
        }
    }
    /// Construct a new, custom error.
    pub fn custom(error: impl core::error::Error + Send + Sync + 'static) -> Error {
        Error::new(ErrorKind::Custom(Box::new(error)))
    }
    /// Construct a custom error from a static string.
    pub fn custom_str(error: &'static str) -> Error {
        #[derive(Debug, thiserror::Error)]
        #[error("{0}")]
        pub struct StrError(pub &'static str);

        Error::new(ErrorKind::Custom(Box::new(StrError(error))))
    }
    /// Construct a custom error from an owned string.
    pub fn custom_string(error: String) -> Error {
        #[derive(Debug, thiserror::Error)]
        #[error("{0}")]
        pub struct StringError(String);

        Error::new(ErrorKind::Custom(Box::new(StringError(error))))
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let path = self.context.path();
        let kind = &self.kind;
        write!(f, "Error at {path}: {kind}")
    }
}

/// The underlying nature of the error.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    /// There was an error resolving the type via the given [`crate::TypeResolver`].
    #[error("Failed to resolve type: {0}")]
    TypeResolvingError(String),
    /// Cannot find a given type.
    #[error("Cannot find type with identifier {0}")]
    TypeNotFound(String),
    /// Cannot encode the actual type given into the target type ID.
    #[error("Cannot encode {actual:?} into type with ID {expected_id}")]
    WrongShape {
        /// The actual kind we have to encode
        actual: Kind,
        /// Identifier for the expected type
        expected_id: String,
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
    #[error("Number {value} is out of range for target type with identifier {expected_id}")]
    NumberOutOfRange {
        /// A string represenatation of the numeric value that was out of range.
        value: String,
        /// Identifier for the expected numeric type that we tried to encode it to.
        expected_id: String,
    },
    /// Cannot find a variant with a matching name on the target type.
    #[error("Variant {name} does not exist on type with identifier {expected_id}")]
    CannotFindVariant {
        /// Variant name we can't find in the expected type.
        name: String,
        /// Identifier for the expected type.
        expected_id: String,
    },
    /// Cannot find a field on our source type that's needed for the target type.
    #[error("Field {name} does not exist in our source struct")]
    CannotFindField {
        /// Name of the field which was not provided.
        name: String,
    },
    /// A custom error.
    #[error("Custom error: {0}")]
    Custom(Box<dyn core::error::Error + Send + Sync + 'static>),
}

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
