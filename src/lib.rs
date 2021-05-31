//-
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod errors;

use std::{fmt::Debug, marker::PhantomData};

use proptest::{
    prelude::ProptestConfig,
    strategy::{BoxedStrategy, NewTree, Strategy, ValueTree},
    test_runner::TestRunner,
};
use rand::distributions::{uniform::Uniform, Distribution, WeightedIndex};

use errors::Result;

const MIN_COMMAND_SEQUENCE_SIZE: usize = 1;

pub trait SystemUnderTest<C, R> {
    fn run(&mut self, cmd: &C) -> Result<R>;
}

#[derive(Debug)]
pub struct CommandSequence<SM>
where SM: StateMachine
{
    commands: Vec<SM::Command>,
    state_machine: SM,
}

impl<SM> CommandSequence<SM>
where
    SM: StateMachine,
{
    pub fn run(&mut self, system_under_test: &mut Box<dyn SystemUnderTest<SM::Command, SM::CommandResult>>) -> Result<()> {
        self.state_machine.reset();
        for cmd in &self.commands {
            let result = system_under_test.run(cmd)?;
            self.state_machine.next_state(&cmd);
            self.state_machine.postcondition(&cmd, &result)?;
        }
        Ok(())
    }
}

impl<SM> IntoIterator for CommandSequence<SM>
where SM: StateMachine
{
    type Item = SM::Command;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.into_iter()
    }
}

pub trait StateMachine {
    type Command: std::fmt::Debug;
    type CommandResult: std::fmt::Debug;

    fn reset(&mut self);
    fn commands(&self) -> Vec<(usize, BoxedStrategy<Self::Command>)>;
    fn postcondition(&self, cmd: &Self::Command, res: &Self::CommandResult) -> Result<()>;
    fn next_state(&mut self, cmd: &Self::Command);
}

#[derive(Clone, Copy, Debug)]
enum Shrink {
    DeleteCommand,
    ShrinkCommand(usize),
}
pub struct CommandSequenceValueTree<SM>
where SM: StateMachine
{
    elements: Vec<Box<dyn ValueTree<Value = SM::Command>>>,
    state_machine: SM,
    num_included: usize,
    shrink: Shrink,
    prev_shrink: Option<Shrink>,
}

impl<SM> ValueTree for CommandSequenceValueTree<SM>
where
    SM: StateMachine + Clone + std::fmt::Debug
{
    type Value = CommandSequence<SM>;

    fn current(&self) -> Self::Value {
        let commands = self
            .elements
            .iter()
            .enumerate()
            .take(self.num_included)
            .map(|(_, element)| element.current())
            .collect();
        CommandSequence {
            commands,
            state_machine: self.state_machine.clone(),
        }
    }

    fn simplify(&mut self) -> bool {
        if let Shrink::DeleteCommand = self.shrink {
            if self.num_included == MIN_COMMAND_SEQUENCE_SIZE {
                self.shrink = Shrink::ShrinkCommand(self.num_included - 1);
            } else {
                self.num_included -= 1;
                self.prev_shrink = Some(self.shrink);
                self.shrink = Shrink::DeleteCommand;
                return true;
            }
        }

        while let Shrink::ShrinkCommand(ix) = self.shrink {
            if ix >= self.elements.len() {
                return false;
            }

            if ix >= self.num_included {
                self.shrink = Shrink::ShrinkCommand(ix - 1);
                continue;
            }

            if !self.elements[ix].simplify() {
                self.shrink = Shrink::ShrinkCommand(ix - 1);
            } else {
                self.prev_shrink = Some(self.shrink);
                return true;
            }
        }

        panic!("Unexpected shrink state");
    }

    fn complicate(&mut self) -> bool {
        match self.prev_shrink {
            None => false,
            Some(Shrink::DeleteCommand) => {
                self.num_included += 1;
                self.prev_shrink = None;
                true
            }
            Some(Shrink::ShrinkCommand(ix)) => {
                if self.elements[ix].complicate() {
                    true
                } else {
                    self.prev_shrink = None;
                    false
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct CommandSequenceStrategy<S, SM>
where
    S: Strategy,
    SM: StateMachine + Clone,
{
    state_machine: SM,
    max_size: usize,
    _strategy: PhantomData<S>,
}

impl<S, SM> CommandSequenceStrategy<S, SM>
where
    S: Strategy,
    SM: StateMachine + Clone,
{
    fn new(max_size: usize, state_machine: SM) -> Self {
        assert!(max_size >= MIN_COMMAND_SEQUENCE_SIZE);
        CommandSequenceStrategy {
            state_machine,
            max_size,
            _strategy: PhantomData,
        }
    }
}

impl<S, SM> Strategy for CommandSequenceStrategy<S, SM>
where
    S: Strategy,
    SM: StateMachine + Clone + Debug,
{
    type Tree = CommandSequenceValueTree<SM>;
    type Value = CommandSequence<SM>;

    fn new_tree(&self, runner: &mut proptest::test_runner::TestRunner) -> NewTree<Self> {
        let size = Uniform::new_inclusive(1, self.max_size).sample(runner.rng());

        let mut state_machine = self.state_machine.clone();
        state_machine.reset();
        let mut elements = Vec::with_capacity(size);
        while elements.len() < size {
            let possible_commands = state_machine.commands();
            let weights = possible_commands
                .iter()
                .map(|(w, _)| *w)
                .collect::<Vec<usize>>();
            let choice = WeightedIndex::new(&weights)
                .map_err(|e| e.to_string())?
                .sample(runner.rng());
            let (_, ref command_strategy) = possible_commands[choice];
            let command = command_strategy.new_tree(runner)?;
            state_machine.next_state(&command.current());
            elements.push(command);
        }
        state_machine.reset();
        let num_included = elements.len();
        Ok(CommandSequenceValueTree {
            elements,
            state_machine,
            num_included,
            shrink: Shrink::DeleteCommand,
            prev_shrink: None,
        })
    }
}

pub fn command_sequence<SM>(
    max_size: usize,
    state_machine: SM,
) -> CommandSequenceStrategy<BoxedStrategy<SM::Command>, SM>
where
    SM: StateMachine + Clone,
{
    CommandSequenceStrategy::new(max_size, state_machine)
}

pub fn execute_plan<SM, SUTF>(
    config: ProptestConfig,
    max_sequence_size: usize,
    state_machine: SM,
    system_under_test_factory: SUTF
) where
    SM: StateMachine + Clone + std::fmt::Debug,
    SUTF: Fn() -> Box<dyn SystemUnderTest<SM::Command, SM::CommandResult>>
{
    let mut runner = TestRunner::new(config);

    let result = runner.run(
        &command_sequence(max_sequence_size, state_machine),
        |mut commands| {
            let mut sys = system_under_test_factory();
            commands.run(&mut sys)?;
            Ok(())
        }
    );
    if let Err(e) = result {
        println!("Found minimal failing case: {}", e);
    }
}
