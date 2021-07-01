//
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod config;
mod errors;
mod traits;

use std::{fmt::Debug, marker::PhantomData};

use proptest::{
    strategy::{BoxedStrategy, NewTree, Strategy, ValueTree},
    test_runner::{TestError, TestRunner},
};
use rand::distributions::{uniform::Uniform, Distribution, WeightedIndex};

pub use config::Config;
pub use errors::{Error, Result};
pub use traits::{StateMachine, SystemUnderTest};

#[derive(Debug)]
pub struct CommandSequence<SM>
where
    SM: StateMachine,
{
    commands: Vec<SM::Command>,
    state_machine: SM,
}

impl<SM> CommandSequence<SM>
where
    SM: StateMachine,
{
    pub fn run(
        &mut self,
        system_under_test: &mut Box<dyn SystemUnderTest<SM::Command, SM::CommandResult>>,
    ) -> Result<()> {
        self.state_machine.reset();
        for cmd in &self.commands {
            let result = system_under_test.run(cmd)?;
            self.state_machine.postcondition(&cmd, &result)?;
            self.state_machine.next_state(&cmd);
        }
        Ok(())
    }
}

impl<SM> IntoIterator for CommandSequence<SM>
where
    SM: StateMachine,
{
    type Item = SM::Command;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.into_iter()
    }
}

#[derive(Clone, Copy, Debug)]
enum Shrink {
    DeleteCommand(usize),
    ShrinkCommand(usize),
}
pub struct CommandSequenceValueTree<SM>
where
    SM: StateMachine,
{
    elements: Vec<Box<dyn ValueTree<Value = SM::Command>>>,
    included: Vec<bool>,
    state_machine: SM,
    shrink: Shrink,
    prev_shrink: Option<Shrink>,
}

impl<SM> CommandSequenceValueTree<SM>
where
    SM: StateMachine,
{
    fn num_included(&self) -> usize {
        self.included.iter().filter(|&x| *x).count()
    }
}

impl<SM> ValueTree for CommandSequenceValueTree<SM>
where
    SM: StateMachine + Clone + std::fmt::Debug,
{
    type Value = CommandSequence<SM>;

    fn current(&self) -> Self::Value {
        let commands = self
            .elements
            .iter()
            .enumerate()
            .filter(|&(x, _)| self.included[x])
            .map(|(_, element)| element.current())
            .collect();
        CommandSequence {
            commands,
            state_machine: self.state_machine.clone(),
        }
    }

    fn simplify(&mut self) -> bool {
        if let Shrink::DeleteCommand(index) = self.shrink {
            if index >= self.elements.len() || self.num_included() == 1 {
                self.shrink = Shrink::ShrinkCommand(0);
            } else {
                self.included[index] = false;
                self.prev_shrink = Some(self.shrink);
                self.shrink = Shrink::DeleteCommand(index + 1);
                return true;
            }
        }

        while let Shrink::ShrinkCommand(index) = self.shrink {
            if index >= self.elements.len() {
                return false;
            }

            if !self.included[index] {
                self.shrink = Shrink::ShrinkCommand(index + 1);
                continue;
            }

            if !self.elements[index].simplify() {
                self.shrink = Shrink::ShrinkCommand(index + 1);
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
            Some(Shrink::DeleteCommand(index)) => {
                self.included[index] = true;
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
    min_size: usize,
    max_size: usize,
    _strategy: PhantomData<S>,
}

impl<S, SM> CommandSequenceStrategy<S, SM>
where
    S: Strategy,
    SM: StateMachine + Clone,
{
    fn new(min_size: usize, max_size: usize, state_machine: SM) -> Self {
        assert!(max_size >= min_size);
        CommandSequenceStrategy {
            state_machine,
            min_size,
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
        let size = Uniform::new_inclusive(self.min_size, self.max_size).sample(runner.rng());

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
        let num_elements = elements.len();
        Ok(CommandSequenceValueTree {
            elements,
            included: vec![true; num_elements],
            state_machine,
            shrink: Shrink::DeleteCommand(0),
            prev_shrink: None,
        })
    }
}

pub fn command_sequence<SM>(
    min_size: usize,
    max_size: usize,
    state_machine: SM,
) -> CommandSequenceStrategy<BoxedStrategy<SM::Command>, SM>
where
    SM: StateMachine + Clone,
{
    CommandSequenceStrategy::new(min_size, max_size, state_machine)
}

pub fn execute_plan<SM, SUTF>(
    config: Config,
    state_machine: SM,
    system_under_test_factory: SUTF,
) -> std::result::Result<(), TestError<CommandSequence<SM>>>
where
    SM: StateMachine + Clone + std::fmt::Debug,
    SUTF: Fn() -> Box<dyn SystemUnderTest<SM::Command, SM::CommandResult>>,
{
    let mut runner = TestRunner::new(config.proptest);

    let result = runner.run(
        &command_sequence(
            config.min_sequence_size,
            config.max_sequence_size,
            state_machine,
        ),
        |mut commands| {
            let mut sys = system_under_test_factory();
            commands.run(&mut sys)?;
            Ok(())
        },
    );
    if let Err(e) = &result {
        println!("Found minimal failing case: {}", e);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use proptest::strategy::{Just, Strategy};
    use proptest::test_runner::TestError;

    use crate::{config::Config, errors::Result, execute_plan, Error, StateMachine};
    use crate::{CommandSequence, SystemUnderTest};

    #[derive(Clone, Debug)]
    struct TestModel {
        plan: Vec<TestCommand>,
        target: usize,
        idx: Cell<usize>,
        state: usize,
    }

    impl TestModel {
        fn new(plan: Vec<TestCommand>) -> TestModel {
            let target = plan
                .iter()
                .filter(|&x| match *x {
                    TestCommand::Up { .. } => true,
                    _ => false,
                })
                .count();
            TestModel {
                plan,
                target,
                idx: Cell::new(0),
                state: 0,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum TestCommand {
        Up { tag: usize },
        Down,
    }

    impl StateMachine for TestModel {
        type Command = TestCommand;

        type CommandResult = usize;

        fn reset(&mut self) {
            self.idx.set(0);
            self.state = 0;
        }

        fn commands(&self) -> Vec<(usize, proptest::strategy::BoxedStrategy<Self::Command>)> {
            let idx = self.idx.get();
            let s = vec![(1usize, Just(self.plan[idx]).boxed())];
            self.idx.set(usize::min(idx + 1, self.plan.len() - 1));
            s
        }

        fn postcondition(&self, cmd: &Self::Command, _res: &Self::CommandResult) -> Result<()> {
            let state_update = if let &TestCommand::Up { .. } = cmd {
                1
            } else {
                0
            };
            if self.state + state_update == self.target {
                return Result::Err(Error::postcondition(
                    format!("{:?}", cmd),
                    format!("{:?}", 0),
                    format!("{:?}", 0),
                ));
            }
            Ok(())
        }

        fn next_state(&mut self, cmd: &Self::Command) {
            if let &TestCommand::Up { .. } = cmd {
                self.state += 1;
            }
        }
    }

    struct TestSystem;

    impl SystemUnderTest<TestCommand, usize> for TestSystem {
        fn run(&mut self, cmd: &TestCommand) -> Result<usize> {
            match *cmd {
                TestCommand::Up { tag } => Ok(tag),
                TestCommand::Down => Ok(0),
            }
        }
    }

    fn check_result<SM: StateMachine>(
        result: std::result::Result<(), TestError<CommandSequence<SM>>>,
        model: &TestModel,
    ) {
        match result {
            Err(test_error) => match test_error {
                TestError::Fail(_, seq) => {
                    assert_eq!(
                        seq.commands.len(),
                        model.target,
                        "Invalid minimal sequence length"
                    )
                }
                _ => assert!(false, "Test aborted"),
            },
            _ => assert!(false, "Test should have failed"),
        }
    }

    #[test]
    fn shrink_removes_sequence_head() {
        let plan = vec![
            TestCommand::Down,
            TestCommand::Down,
            TestCommand::Up { tag: 1 },
            TestCommand::Up { tag: 2 },
            TestCommand::Up { tag: 3 },
        ];
        let plan_length = plan.len();
        let model = TestModel::new(plan);
        let mut config = Config::default();
        config.min_sequence_size = plan_length;
        config.max_sequence_size = plan_length;
        config.proptest.max_shrink_iters = 100;
        let result = execute_plan(config, model.clone(), || Box::new(TestSystem));
        check_result(result, &model);
    }

    #[test]
    fn shrink_removes_sequence_tail() {
        let plan = vec![
            TestCommand::Up { tag: 1 },
            TestCommand::Up { tag: 2 },
            TestCommand::Up { tag: 3 },
            TestCommand::Down,
        ];
        let plan_length = plan.len();
        let model = TestModel::new(plan);
        let mut config = Config::default();
        config.min_sequence_size = plan_length;
        config.max_sequence_size = plan_length;
        config.proptest.max_shrink_iters = 100;
        let result = execute_plan(config, model.clone(), || Box::new(TestSystem));
        check_result(result, &model);
    }

    #[test]
    fn shrink_removes_arbitrary() {
        let plan = vec![
            TestCommand::Down,
            TestCommand::Up { tag: 1 },
            TestCommand::Down,
            TestCommand::Down,
            TestCommand::Up { tag: 2 },
            TestCommand::Down,
            TestCommand::Down,
            TestCommand::Down,
            TestCommand::Up { tag: 3 },
            TestCommand::Down,
        ];
        let plan_length = plan.len();
        let model = TestModel::new(plan);
        let mut config = Config::default();
        config.min_sequence_size = plan_length;
        config.max_sequence_size = plan_length;
        config.proptest.max_shrink_iters = 100;
        let result = execute_plan(config, model.clone(), || Box::new(TestSystem));
        check_result(result, &model);
    }
}
