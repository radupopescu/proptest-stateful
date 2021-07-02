//
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::errors::Result;
use proptest::strategy::BoxedStrategy;

/// This trait represents the interface to the system-under-test. The two type parameters,
/// `C` and `R`, are the types encoding the commands the system can receive, respectively
/// the responses given by the system to various commands.
pub trait SystemUnderTest<C, R> {
    /// The method takes a reference to a system command, applies the command to the system
    /// and updates its internal state, returning the corresponding response.
    fn run(&mut self, cmd: &C) -> Result<R>;
}

/// The trait defines the interface of the simplified model of the system-under-test.
pub trait StateMachine {
    /// Type which encodes the commands accepted by the model
    type Command: std::fmt::Debug;

    /// Type which encodes the responses of the model to the various commands
    type CommandResult: std::fmt::Debug;

    /// Reset the model to its initial state
    fn reset(&mut self);

    /// Returns a vector of tuples describing the possible commands for the system based
    /// on its current state. Each tuple is made up of an integer weight and a proptest
    /// strategy for sampling the command. The weight can used to bias the sampling
    /// towards specific commands (for example, when modelling a database, one might want
    /// to bias writes over reads).
    fn commands(&self) -> Vec<(usize, BoxedStrategy<Self::Command>)>;

    /// Check that all postconditions would hold after applying the provided command to
    /// the current state of the system model
    fn postcondition(&self, cmd: &Self::Command, res: &Self::CommandResult) -> Result<()>;

    /// Advance the system model to the next state by applying the provided command
    fn next_state(&mut self, cmd: &Self::Command);
}
