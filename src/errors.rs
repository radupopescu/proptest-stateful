//
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub type Result<T> = std::result::Result<T, Error>;

/// The errors which can be produced by the library
#[derive(Debug)]
pub enum Error {
    /// Error in the execution of the system-under-test
    SystemUnderTest {
        source: Box<dyn std::error::Error + 'static>,
    },
    /// Model state machine postcondition does not hold
    Postcondition {
        command: String,
        expected: String,
        actual: String,
    },
}

impl Error {
    pub fn system_under_test<T>(source: T) -> Error
    where
        T: std::error::Error + 'static,
    {
        Self::SystemUnderTest {
            source: Box::new(source),
        }
    }

    pub fn postcondition<T: AsRef<str>>(command: T, expected: T, actual: T) -> Error {
        Self::Postcondition {
            command: command.as_ref().to_string(),
            expected: expected.as_ref().to_string(),
            actual: actual.as_ref().to_string(),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::SystemUnderTest { ref source } => Some(&**source),
            Error::Postcondition { .. } => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::SystemUnderTest { ref source } => source.fmt(f),
            Error::Postcondition {
                ref command,
                ref expected,
                ref actual,
            } => {
                write!(
                    f,
                    "Postcondition does not hold. Command: {}. Expected result: {}. Actual result: {}",
                    command, expected, actual
                )
            }
        }
    }
}
