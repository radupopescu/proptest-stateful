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

    use super::{Cache, Result};

    use proptest::prelude::*;
    use proptest_stateful::{CommandSequenceStrategy, StateMachine};

    const MAX_CACHE_SIZE: usize = 10;
    const MAX_COMMAND_SEQUENCE_SIZE: usize = 10;

    #[derive(Debug, Clone)]
    enum Command {
        Get { key: isize },
        Set { key: isize, value: isize },
        Flush,
    }

    impl Command {
        pub fn run(self, instance: &mut Cache) -> Result<CommandResult> {
            match self {
                Command::Get { key } => {
                    let v = instance.get(key)?;
                    match v {
                        Some(v) => Ok(CommandResult::Some(v)),
                        None => Ok(CommandResult::None),
                    }
                }
                Command::Set { key, value } => {
                    instance.set(key, value)?;
                    Ok(CommandResult::None)
                }
                Command::Flush => {
                    instance.flush()?;
                    Ok(CommandResult::None)
                }
            }
        }
    }

    #[derive(PartialEq)]
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

    impl StateMachine<Command> for CacheModel {
        fn reset(&mut self) {
            self.entries.clear();
            self.min_index = 0;
            self.max_index = 0;
        }

        fn commands(&self) -> Vec<(usize, BoxedStrategy<Command>)> {
            let mut options = vec![
                (
                    1,
                    self.key().prop_map(|k| Command::Get { key: k }).boxed(),
                ),
                (
                    3,
                    (self.key(), CacheModel::val())
                        .prop_map(|(k, v)| Command::Set { key: k, value: v })
                        .boxed(),
                ),
            ];
            if !self.entries.is_empty() {
                options.push((1, Just(Command::Flush).boxed()));
            }
            options
        }

        fn postcondition(&self, cmd: Command, res: &(dyn Any)) -> bool {
            if let Command::Get { key } = cmd {
                return match self.entries.get(&key) {
                    Some(Entry { val, .. }) => {
                        if let Some(cmd_res) = res.downcast_ref::<CommandResult>() {
                            cmd_res == &CommandResult::Some(*val)
                        } else {
                            false
                        }
                    }
                    None => {
                        if let Some(cmd_res) = res.downcast_ref::<CommandResult>() {
                            cmd_res == &CommandResult::None
                        } else {
                            false
                        }
                    }
                };
            }
            true
        }

        fn next_state(&mut self, cmd: Command) {
            match cmd {
                Command::Get { key: _ } => {}
                Command::Set { key, value } => {
                    if let Some(entry) = self.entries.get_mut(&key) {
                        entry.val = value;
                    } else {
                        if self.entries.len() == MAX_CACHE_SIZE {
                            let key_to_delete = self
                                .entries
                                .iter()
                                .filter(|&(_, v)| v.index == self.min_index)
                                .map(|(k, _)| *k)
                                .collect::<Vec<isize>>()[0];
                            self.entries.remove(&key_to_delete);
                            self.min_index += 1;
                        }
                        self.max_index += 1;
                        self.entries.insert(
                            key,
                            Entry {
                                index: self.max_index,
                                val: value,
                            },
                        );
                    }
                }
                Command::Flush => {
                    self.min_index = 0;
                    self.max_index = 0;
                    self.entries.clear();
                }
            }
        }
    }

    fn command_sequence(max_size: usize) -> CommandSequenceStrategy<BoxedStrategy<Command>, CacheModel>
    {
        let state_machine = CacheModel::new(MAX_CACHE_SIZE);
        CommandSequenceStrategy::new(max_size, state_machine)
    }

    proptest! {
        #[test]
        fn simple_command_execution(commands in command_sequence(MAX_COMMAND_SEQUENCE_SIZE)) {
            if let Ok(mut cache) = Cache::new(MAX_CACHE_SIZE) {
                println!("BEGIN");
                for cmd in commands {
                    println!("{:?}", cmd);
                    let _v = cmd.run(&mut cache);
                }
                println!("END");
            }
        }
    }
}
