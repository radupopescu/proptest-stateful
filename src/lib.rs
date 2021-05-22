//-
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod errors;

use std::{any::Any, fmt::Debug, marker::PhantomData};

use proptest::{
    prelude::ProptestConfig,
    strategy::{BoxedStrategy, NewTree, Strategy, ValueTree},
    test_runner::TestRunner,
};
use rand::distributions::{uniform::Uniform, Distribution, WeightedIndex};

use errors::Result;

const MIN_COMMAND_SEQUENCE_SIZE: usize = 1;

pub trait Command {
    fn run(&self, system_under_test: &mut Box<dyn Any>) -> Result<Box<dyn Any>>;
}

#[derive(Debug)]
pub struct CommandSequence<C, SM> {
    commands: Vec<C>,
    state_machine: SM,
}

impl<C, SM> CommandSequence<C, SM>
where
    C: Command,
    SM: StateMachine<C>,
{
    pub fn run(&mut self, system_under_test: &mut Box<dyn Any>) -> Result<()> {
        self.state_machine.reset();
        for cmd in &self.commands {
            let result = cmd.run(system_under_test)?;
            self.state_machine.next_state(&cmd);
            self.state_machine.postcondition(&cmd, result)?;
        }
        Ok(())
    }
}

impl<C, SM> IntoIterator for CommandSequence<C, SM> {
    type Item = C;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.into_iter()
    }
}

pub trait StateMachine<C> {
    fn reset(&mut self);
    fn commands(&self) -> Vec<(usize, BoxedStrategy<C>)>;
    fn postcondition(&self, cmd: &C, res: Box<(dyn Any)>) -> Result<()>;
    fn next_state(&mut self, cmd: &C);
}

#[derive(Clone, Copy, Debug)]
enum Shrink {
    DeleteCommand,
    ShrinkCommand(usize),
}
pub struct CommandSequenceValueTree<C, SM> {
    elements: Vec<Box<dyn ValueTree<Value = C>>>,
    state_machine: SM,
    num_included: usize,
    shrink: Shrink,
    prev_shrink: Option<Shrink>,
}

impl<C, SM> ValueTree for CommandSequenceValueTree<C, SM>
where
    C: std::fmt::Debug,
    SM: Clone + std::fmt::Debug,
{
    type Value = CommandSequence<C, SM>;

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
    SM: StateMachine<S::Value> + Clone,
{
    state_machine: SM,
    max_size: usize,
    _strategy: PhantomData<S>,
}

impl<S, SM> CommandSequenceStrategy<S, SM>
where
    S: Strategy,
    SM: StateMachine<S::Value> + Clone,
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
    SM: StateMachine<S::Value> + Clone + Debug,
{
    type Tree = CommandSequenceValueTree<S::Value, SM>;
    type Value = CommandSequence<S::Value, SM>;

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

pub fn command_sequence<C, SM, F>(
    max_size: usize,
    state_machine_builder: F,
) -> CommandSequenceStrategy<BoxedStrategy<C>, SM>
where
    C: std::fmt::Debug,
    SM: StateMachine<C> + Clone,
    F: Fn() -> SM,
{
    CommandSequenceStrategy::new(max_size, state_machine_builder())
}

pub fn execute_plan<SM, S, SMF, SUTF>(
    config: ProptestConfig,
    max_sequence_size: usize,
    state_machine_factory: SMF,
    system_under_test_factory: SUTF
) where
    S: Command + std::fmt::Debug,
    SM: StateMachine<S> + Clone + std::fmt::Debug,
    SMF: Fn() -> SM,
    SUTF: Fn() -> Box<dyn Any>
{
    let mut runner = TestRunner::new(config);

    let result = runner.run(
        &command_sequence(max_sequence_size, state_machine_factory),
        |mut commands| {
            let mut system_under_test = system_under_test_factory();
            commands.run(&mut system_under_test)?;
            Ok(())
        }
    );
    if let Err(e) = result {
        println!("Found minimal failing case: {}", e);
    }
}
