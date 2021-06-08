//-
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use proptest::strategy::BoxedStrategy;
use crate::errors::Result;

pub trait SystemUnderTest<C, R> {
    fn run(&mut self, cmd: &C) -> Result<R>;
}

pub trait StateMachine {
    type Command: std::fmt::Debug;
    type CommandResult: std::fmt::Debug;

    fn reset(&mut self);
    fn commands(&self) -> Vec<(usize, BoxedStrategy<Self::Command>)>;
    fn postcondition(&self, cmd: &Self::Command, res: &Self::CommandResult) -> Result<()>;
    fn next_state(&mut self, cmd: &Self::Command);
}
