//-
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod errors;

use std::{any::Any, fmt::Debug, marker::PhantomData};

use proptest::strategy::{BoxedStrategy, NewTree, Strategy, ValueTree};
use rand::distributions::{uniform::Uniform, Distribution, WeightedIndex};

use errors::Result;

const MIN_COMMAND_SEQUENCE_SIZE: usize = 1;

pub trait Command {
    fn run(&self, system_under_test: &mut (dyn Any)) -> Result<Box<dyn Any>>;
}

#[derive(Debug)]
pub struct CommandSequence<C> {
    commands: Vec<C>,
}

impl<C> CommandSequence<C>
where
    C: Command,
{
    pub fn run<SM: StateMachine<C>>(
        &self,
        model: &mut SM,
        system_under_test: &mut (dyn Any),
    ) -> Result<()> {
        model.reset();
        for cmd in &self.commands {
            let result = cmd.run(system_under_test)?;
            model.next_state(&cmd);
            model.postcondition(&cmd, result)?;
        }
        Ok(())
    }
}

impl<C> IntoIterator for CommandSequence<C> {
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
pub struct CommandSequenceValueTree<C> {
    elements: Vec<Box<dyn ValueTree<Value = C>>>,
    num_included: usize,
    shrink: Shrink,
    prev_shrink: Option<Shrink>,
}

impl<C: Debug> ValueTree for CommandSequenceValueTree<C> {
    type Value = CommandSequence<C>;

    fn current(&self) -> Self::Value {
        let commands = self
            .elements
            .iter()
            .enumerate()
            .take(self.num_included)
            .map(|(_, element)| element.current())
            .collect();
        CommandSequence { commands }
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
    pub fn new(max_size: usize, state_machine: SM) -> Self {
        assert!(max_size >= MIN_COMMAND_SEQUENCE_SIZE);
        CommandSequenceStrategy {
            state_machine,
            max_size,
            _strategy: PhantomData,
        }
    }

    fn state_machine(&self) -> SM {
        let mut sm = self.state_machine.clone();
        sm.reset();
        sm
    }
}

impl<S, SM> Strategy for CommandSequenceStrategy<S, SM>
where
    S: Strategy,
    SM: StateMachine<S::Value> + Clone + Debug,
{
    type Tree = CommandSequenceValueTree<S::Value>;
    type Value = CommandSequence<S::Value>;

    fn new_tree(&self, runner: &mut proptest::test_runner::TestRunner) -> NewTree<Self> {
        let size = Uniform::new_inclusive(1, self.max_size).sample(runner.rng());

        let mut state_machine = self.state_machine();
        let mut elements = Vec::with_capacity(size);
        while elements.len() < size {
            let possible_commands = state_machine.commands();
            let weights = possible_commands
                .iter()
                .map(|(w, _)| *w)
                .collect::<Vec<usize>>();
            let choice = WeightedIndex::new(&weights)
                .map_err(|_| "Could not construct weighted index distribution")?
                .sample(runner.rng());
            let (_, ref command_strategy) = possible_commands[choice];
            let command = command_strategy.new_tree(runner)?;
            state_machine.next_state(&command.current());
            elements.push(command);
        }
        let num_included = elements.len();
        Ok(CommandSequenceValueTree {
            elements,
            num_included,
            shrink: Shrink::DeleteCommand,
            prev_shrink: None,
        })
    }
}
