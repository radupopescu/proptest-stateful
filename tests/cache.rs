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
    use std::collections::BTreeMap;

    use super::{Cache, Result};

    use proptest::{collection::vec, prelude::*, strategy::Union};

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
        pub fn exec(self, instance: &mut Cache) -> Result<()> {
            match self {
                Command::Get { key } => {
                    let _v = instance.get(key)?;
                    Ok(())
                }
                Command::Set { key, value } => instance.set(key, value),
                Command::Flush => instance.flush(),
            }
        }
    }

    fn command_strategy() -> impl Strategy<Value = Command> {
        prop_oneof![
            any::<isize>().prop_map(|k| Command::Get { key: k }),
            (any::<isize>(), any::<isize>()).prop_map(|(k, v)| Command::Set { key: k, value: v }),
            Just(Command::Flush),
        ]
    }

    const MAX_COMMAND_SEQUENCE_SIZE: usize = 10;

    fn command_sequence_strategy() -> impl Strategy<Value = Vec<Command>> {
        vec(command_strategy(), MAX_COMMAND_SEQUENCE_SIZE)
    }

    proptest! {
        #[test]
        fn simple_command_execution(commands in command_sequence_strategy()) {
            if let Ok(mut cache) = Cache::new(10) {
                println!("BEGIN");
                for cmd in commands {
                    println!("{:?}", cmd);
                    let _v = cmd.exec(&mut cache);
                }
                println!("END");
            }
        }
    }
}
