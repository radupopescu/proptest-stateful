//-
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Basic in-memory cache backed by SQLite.
///
/// This test is based on Chapter 9 of the book "Property-Based Testing with PropEr", by Fred Hebert
///
/// * Values can be read by searching for their key
/// * The cache can be emptied on demand
/// * The cache can be configured with a maximum number of iterms to hold in memory
/// * Once the maximal size is reached, the oldest written value is replaced
/// * If an item is overwritten, even with a changed value, the cache entry remains in the same position
use rusqlite::{Connection, OptionalExtension, Result};

struct Cache {
    conn: Connection,
    size: usize,
}

impl Cache {
    pub fn new(size: usize) -> Result<Cache> {
        let conn = Connection::open_in_memory()?;

        conn.execute(
            "create table cache (
                    id             integer primary key,
                    key            integer unique,
                    val            integer
                    )",
            [],
        )?;

        Ok(Cache { conn, size })
    }

    pub fn get(&self, key: isize) -> Result<Option<isize>> {
        self.conn
            .query_row("select val from cache where key = ?", [key], |row| {
                row.get(0)
            })
            .optional()
    }

    pub fn set(&mut self, key: isize, val: isize) -> Result<()> {
        let tx = self.conn.transaction()?;
        let params = &[(":key", &key), (":val", &val)];
        let _ = match tx
            .query_row("select val from cache where key = ?", [key], |row| {
                row.get::<_, isize>(0)
            })
            .optional()?
        {
            Some(_) => {
                tx.execute("update cache set val = :val where key = :key", params)?;
            }
            None => {
                if let Some(num_entries) = tx
                    .query_row("select count(*) from cache", [], |row| {
                        row.get::<_, usize>(0)
                    })
                    .optional()?
                {
                    if num_entries == self.size {
                        tx.execute(
                            "delete from cache where id = (select min(id) from cache)",
                            [],
                        )?;
                    }
                    tx.execute("insert into cache (key, val) values (:key, :val)", params)?;
                }
            }
        };

        tx.commit()
    }

    pub fn flush(&mut self) -> Result<()> {
        self.conn.execute("delete from cache", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, collections::HashMap};

    use super::Cache;

    use proptest::prelude::*;
    use proptest_stateful::{Command, CommandSequenceStrategy, StateMachine, errors::{Error, Result}};

    const MAX_CACHE_SIZE: usize = 10;
    const MAX_COMMAND_SEQUENCE_SIZE: usize = 20;

    #[derive(Debug, Clone)]
    enum CacheCommand {
        Get { key: isize },
        Set { key: isize, value: isize },
        Flush,
    }

    impl Command for CacheCommand {
        fn run(&self, instance: &mut (dyn Any)) -> Result<Box<dyn Any>> {
            let cache = instance
                .downcast_mut::<Cache>()
                .ok_or_else(|| "Invalid argument. Expecting instance of system-under-test")?;
            match self {
                &CacheCommand::Get { key } => {
                    let v = cache.get(key)?;
                    match v {
                        Some(v) => Ok(Box::new(CommandResult::Some(v))),
                        None => Ok(Box::new(CommandResult::None)),
                    }
                }
                &CacheCommand::Set { key, value } => {
                    cache.set(key, value)?;
                    Ok(Box::new(CommandResult::None))
                }
                &CacheCommand::Flush => {
                    cache.flush()?;
                    Ok(Box::new(CommandResult::None))
                }
            }
        }
    }

    #[derive(Debug, PartialEq)]
    enum CommandResult {
        Some(isize),
        None,
    }

    #[derive(Clone, Debug)]
    struct Entry {
        index: usize,
        val: isize,
    }

    #[derive(Clone, Debug)]
    struct CacheModel {
        entries: HashMap<isize, Entry>,
        max_num_entries: usize,
        min_index: usize,
        max_index: usize,
    }

    impl CacheModel {
        fn new(max_num_entries: usize) -> CacheModel {
            CacheModel {
                entries: HashMap::new(),
                max_num_entries,
                min_index: 0,
                max_index: 0,
            }
        }

        fn key(&self) -> impl Strategy<Value = isize> {
            prop_oneof![(1isize..(self.max_num_entries as isize)), any::<isize>(),]
        }

        fn val() -> impl Strategy<Value = isize> {
            any::<isize>()
        }
    }

    impl StateMachine<CacheCommand> for CacheModel {
        fn reset(&mut self) {
            self.entries.clear();
            self.min_index = 0;
            self.max_index = 0;
        }

        fn commands(&self) -> Vec<(usize, BoxedStrategy<CacheCommand>)> {
            let mut options = vec![
                (
                    1,
                    self.key()
                        .prop_map(|k| CacheCommand::Get { key: k })
                        .boxed(),
                ),
                (
                    3,
                    (self.key(), CacheModel::val())
                        .prop_map(|(k, v)| CacheCommand::Set { key: k, value: v })
                        .boxed(),
                ),
            ];
            if !self.entries.is_empty() {
                options.push((1, Just(CacheCommand::Flush).boxed()));
            }
            options
        }

        fn postcondition(&self, cmd: &CacheCommand, res: Box<dyn Any>) -> Result<()> {
            if let CacheCommand::Get { key } = cmd {
                match self.entries.get(&key) {
                    Some(Entry { val, .. }) => {
                        if let Some(cmd_res) = res.downcast_ref::<CommandResult>() {
                            if cmd_res != &CommandResult::Some(*val) {
                                return Result::Err(Error::Postcondition {
                                    expected: format!("{:?}", CommandResult::Some(*val)),
                                    actual: format!("{:?}", *cmd_res),
                                }.into())
                            }
                        } else {
                            panic!("Invalid command result data type")
                        }
                    }
                    None => {
                        if let Some(cmd_res) = res.downcast_ref::<CommandResult>() {
                            if cmd_res != &CommandResult::None {
                                return Result::Err(Error::Postcondition {
                                    expected: format!("{:?}", CommandResult::None),
                                    actual: format!("{:?}", *cmd_res),
                                }.into())
                            }
                        } else {
                            panic!("Invalid command result data type")
                        }
                    }
                }
            }
            Ok(())
        }

        fn next_state(&mut self, cmd: &CacheCommand) {
            match cmd {
                &CacheCommand::Get { key: _ } => {}
                &CacheCommand::Set { key, value } => {
                    if let Some(entry) = self.entries.get_mut(&key) {
                        entry.val = value;
                    } else {
                        if self.entries.len() == self.max_num_entries {
                            let key_to_delete = self
                                .entries
                                .iter()
                                .filter(|&(_, v)| v.index == self.min_index)
                                .map(|(k, _)| *k)
                                .collect::<Vec<isize>>()[0];
                            self.entries.remove(&key_to_delete);
                            self.min_index += 1;
                        }
                        self.entries.insert(
                            key,
                            Entry {
                                index: self.max_index,
                                val: value,
                            },
                        );
                        self.max_index += 1;
                    }
                }
                &CacheCommand::Flush => {
                    self.min_index = 0;
                    self.max_index = 0;
                    self.entries.clear();
                }
            }
        }
    }

    fn command_sequence(
        max_size: usize,
    ) -> CommandSequenceStrategy<BoxedStrategy<CacheCommand>, CacheModel> {
        let state_machine = CacheModel::new(MAX_CACHE_SIZE);
        CommandSequenceStrategy::new(max_size, state_machine)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10))]
        #[test]
        fn simple_command_execution(commands in command_sequence(MAX_COMMAND_SEQUENCE_SIZE)) {
            if let Ok(mut cache) = Cache::new(MAX_CACHE_SIZE) {
                let mut model = CacheModel::new(MAX_CACHE_SIZE);
                let _ = commands.run(&mut model, &mut cache);
            }
        }
    }
}
