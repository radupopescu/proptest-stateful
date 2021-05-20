//-
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
pub enum Error {
    SystemExecution {
        source: Box<dyn std::error::Error + 'static>
    },
    Postcondition {
        expected: String,
        actual: String,
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::SystemExecution { ref source } => Some(&**source),
            Error::Postcondition { .. } => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::SystemExecution { ref source } => source.fmt(f),
            Error::Postcondition { ref expected, ref actual } => {
                write!(f, "Postcondition does not hold. Expected result: {}. Actual result: {}", expected, actual)
            }
        }
    }
}