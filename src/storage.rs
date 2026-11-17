use std::{error::Error, fs, path::Path};

use directories::ProjectDirs;
use rusqlite::{Connection, params};

pub type StorageResult<T> = Result<T, Box<dyn Error>>;

pub struct Campaign {
    pub id: i64,
    pub name: String,
}

pub struct Character {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub class: String,
    pub level: Option<i64>,
    pub abilities: [Option<i64>; 6],
}

pub struct Storage {
    connection: Connection,
}

impl Storage {
    pub fn open_default() -> StorageResult<Self> {
        let dirs = ProjectDirs::from("com", "DNDHandler", "DNDHandler")
            .ok_or("Could not locate an application data directory")?;
        fs::create_dir_all(dirs.data_dir())?;
        Self::open(dirs.data_dir().join("characters.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 3 {
            return Err(format!("Database schema version {version} is newer than this app").into());
        }
        if version == 0 {
            connection.execute_batch(
                "BEGIN;
                 CREATE TABLE campaigns (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        if version < 2 {
            connection.execute_batch(
                "BEGIN;
                 CREATE TABLE characters (
                     id INTEGER PRIMARY KEY,
                     campaign_id INTEGER REFERENCES campaigns(id) ON DELETE SET NULL,
                     name TEXT NOT NULL,
                     position INTEGER NOT NULL,
                     created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 CREATE INDEX characters_by_campaign ON characters(campaign_id, position);
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        }
        if version < 3 {
            connection.execute_batch(
                "BEGIN;
                 ALTER TABLE characters ADD COLUMN class TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN level INTEGER;
                 ALTER TABLE characters ADD COLUMN strength INTEGER;
                 ALTER TABLE characters ADD COLUMN dexterity INTEGER;
                 ALTER TABLE characters ADD COLUMN constitution INTEGER;
                 ALTER TABLE characters ADD COLUMN intelligence INTEGER;
                 ALTER TABLE characters ADD COLUMN wisdom INTEGER;
                 ALTER TABLE characters ADD COLUMN charisma INTEGER;
                 PRAGMA user_version = 3;
                 COMMIT;",
            )?;
        }
        Ok(Self { connection })
    }

    pub fn campaigns(&self) -> StorageResult<Vec<Campaign>> {
        let mut query = self
            .connection
            .prepare("SELECT id, name FROM campaigns ORDER BY id")?;
        let rows = query.query_map([], |row| {
            Ok(Campaign {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_campaign(&self, name: &str) -> StorageResult<Campaign> {
        self.connection
            .execute("INSERT INTO campaigns (name) VALUES (?1)", params![name])?;
        Ok(Campaign {
            id: self.connection.last_insert_rowid(),
            name: name.to_owned(),
        })
    }

    pub fn characters(&self, campaign_id: i64) -> StorageResult<Vec<Character>> {
        let mut query = self.connection.prepare(
            "SELECT id, name, position, class, level, strength, dexterity,
                    constitution, intelligence, wisdom, charisma FROM characters
             WHERE campaign_id = ?1 ORDER BY position, id",
        )?;
        let rows = query.query_map([campaign_id], |row| {
            Ok(Character {
                id: row.get(0)?,
                name: row.get(1)?,
                position: row.get(2)?,
                class: row.get(3)?,
                level: row.get(4)?,
                abilities: [
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ],
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_character(&self, campaign_id: i64, name: &str) -> StorageResult<Character> {
        self.connection.execute(
            "INSERT INTO characters (campaign_id, name, position)
             SELECT ?1, ?2, COALESCE(MAX(position), 0) + 1
             FROM characters WHERE campaign_id = ?1",
            params![campaign_id, name],
        )?;
        let id = self.connection.last_insert_rowid();
        let position = self.connection.query_row(
            "SELECT position FROM characters WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(Character {
            id,
            name: name.to_owned(),
            position,
            class: String::new(),
            level: None,
            abilities: [None; 6],
        })
    }

    pub fn save_field(&self, character_id: i64, field: &str, value: &str) -> StorageResult<()> {
        let column = match field {
            "name" | "class" | "level" | "strength" | "dexterity" | "constitution"
            | "intelligence" | "wisdom" | "charisma" => field,
            _ => return Err("Unknown character field".into()),
        };
        let sql = format!("UPDATE characters SET {column} = ?1 WHERE id = ?2");
        if column == "name" || column == "class" {
            self.connection
                .execute(&sql, params![value, character_id])?;
        } else {
            let number = if value.trim().is_empty() {
                None
            } else {
                Some(value.trim().parse::<i64>()?)
            };
            if number.is_some_and(|n| !(1..=30).contains(&n)) {
                return Err("Enter a number from 1 to 30".into());
            }
            self.connection
                .execute(&sql, params![number, character_id])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Storage;
    use rusqlite::Connection;

    #[test]
    fn campaign_survives_reopening_database() {
        let path = std::env::temp_dir().join(format!(
            "dnd-handler-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let storage = Storage::open(&path).unwrap();
            let campaign = storage.create_campaign("").unwrap();
            assert!(campaign.id > 0);
            assert_eq!(
                storage
                    .create_character(campaign.id, "John")
                    .unwrap()
                    .position,
                1
            );
            assert_eq!(
                storage.create_character(campaign.id, "").unwrap().position,
                2
            );
        }
        let reopened = Storage::open(&path).unwrap();
        assert_eq!(reopened.campaigns().unwrap()[0].name, "");
        let id = reopened
            .characters(reopened.campaigns().unwrap()[0].id)
            .unwrap()[0]
            .id;
        reopened.save_field(id, "strength", "16").unwrap();
        reopened.save_field(id, "name", "Renamed").unwrap();
        assert_eq!(
            reopened
                .characters(reopened.campaigns().unwrap()[0].id)
                .unwrap()
                .len(),
            2
        );
        let saved = &reopened
            .characters(reopened.campaigns().unwrap()[0].id)
            .unwrap()[0];
        assert_eq!(saved.name, "Renamed");
        assert_eq!(saved.abilities[0], Some(16));
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn existing_campaign_database_gains_characters_table() {
        let path = std::env::temp_dir().join(format!(
            "dnd-handler-migration-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE campaigns (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO campaigns (name) VALUES ('Existing');
             PRAGMA user_version = 1;",
            )
            .unwrap();
        drop(connection);
        let storage = Storage::open(&path).unwrap();
        let campaign = &storage.campaigns().unwrap()[0];
        assert_eq!(campaign.name, "Existing");
        assert_eq!(
            storage
                .create_character(campaign.id, "New")
                .unwrap()
                .position,
            1
        );
        drop(storage);
        std::fs::remove_file(path).unwrap();
    }
}
