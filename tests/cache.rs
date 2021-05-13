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
    use std::collections::HashMap;

    use super::{Cache, Result};

    use proptest::{collection::vec, prelude::*, strategy::Union};

    const MAX_CACHE_SIZE: usize = 10;
    const MAX_COMMAND_SEQUENCE_SIZE: usize = 10;

    #[test]
    fn basic() -> Result<()> {
        let mut cache = Cache::new(2)?;

        cache.set(1, 123)?;
        assert_eq!(cache.get(1)?, Some(123));

        cache.flush()?;
        assert_eq!(cache.get(1)?, None);

        cache.set(1, 123)?;
        cache.set(2, 124)?;
        cache.set(3, 125)?;
        assert_eq!(cache.get(1)?, None);

        Ok(())
    }

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
                        None => Ok(CommandResult::None)
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

    struct Entry {
        index: usize,
        val: isize,
    }

    struct Model {
        entries: HashMap<isize, Entry>,
        min_index: usize,
        max_index: usize,
    }

    impl Model {
        fn new() -> Model {
            Model {
                entries: HashMap::new(),
                min_index: 0,
                max_index: 0,
            }
        }

        fn key() -> impl Strategy<Value = isize> {
            prop_oneof![(1isize..(MAX_CACHE_SIZE as isize)), any::<isize>(),]
        }

        fn val() -> impl Strategy<Value = isize> {
            any::<isize>()
        }

        fn command(&self) -> impl Strategy<Value = Command> {
            Union::new_weighted(
                vec![
                    (1, any::<isize>().prop_map(|k| Command::Get { key: k }).boxed()),
                    (3, (any::<isize>(), any::<isize>()).prop_map(|(k, v)| Command::Set { key: k, value: v }).boxed()),
                    (1, Just(Command::Flush).boxed())
                ]
            )
        }

        fn precondition(&self, cmd: Command) -> bool {
            if let Command::Flush = cmd {
                if self.entries.is_empty() {
                    return false;
                }
            }

            true
        }

        fn postcondition(&self, cmd: Command, res: CommandResult) -> bool {
            if let Command::Get { key } = cmd {
                return match self.entries.get(&key) {
                    Some(Entry { val, .. }) => {
                        res == CommandResult::Some(*val)
                    },
                    None => {
                        res == CommandResult::None
                    }
                }
            }
            true
        }

        fn next_state(&mut self, _res: CommandResult, cmd: Command) {
            match cmd {
                Command::Get { key: _ } => {}
                Command::Set { key, value } => {
                    if let Some(entry) = self.entries.get_mut(&key) {
                        entry.val = value;
                    } else {
                        if self.entries.len() == MAX_CACHE_SIZE {
                            let key_to_delete = self.entries.iter()
                            .filter(|&(_, v)| v.index == self.min_index)
                            .map(|(k,_)| *k)
                            .collect::<Vec<isize>>()[0];
                            self.entries.remove(&key_to_delete);
                            self.min_index += 1;
                        }
                        self.max_index += 1;
                        self.entries.insert(key, Entry {index: self.max_index, val: value });
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

    fn command_strategy() -> impl Strategy<Value = Command> {
        Union::new_weighted(
            vec![
                (1, any::<isize>().prop_map(|k| Command::Get { key: k }).boxed()),
                (3, (any::<isize>(), any::<isize>()).prop_map(|(k, v)| Command::Set { key: k, value: v }).boxed()),
                (1, Just(Command::Flush).boxed())
            ]
        )
    }

    fn command_sequence_strategy() -> impl Strategy<Value = Vec<Command>> {
        vec(command_strategy(), MAX_COMMAND_SEQUENCE_SIZE)
    }

    proptest! {
        #[test]
        fn simple_command_execution(commands in command_sequence_strategy()) {
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
