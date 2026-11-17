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
    pub background: String,
    pub player_name: String,
    pub race: String,
    pub alignment: String,
    pub experience_points: Option<i64>,
    pub armor_class: Option<i64>,
    pub initiative_override: Option<i64>,
    pub speed: String,
    pub hit_point_maximum: Option<i64>,
    pub current_hit_points: Option<i64>,
    pub temporary_hit_points: Option<i64>,
    pub hit_dice: String,
    pub death_successes: i64,
    pub death_failures: i64,
    pub abilities: [Option<i64>; 6],
    pub inspiration: bool,
    pub proficiency_bonus_override: Option<i64>,
    pub saving_throw_proficiencies: [bool; 6],
    pub saving_throw_overrides: [Option<i64>; 6],
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
        if version > 6 {
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
        if version < 4 {
            connection.execute_batch(
                "BEGIN;
                 ALTER TABLE characters ADD COLUMN background TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN player_name TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN race TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN alignment TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN experience_points INTEGER;
                 PRAGMA user_version = 4;
                 COMMIT;",
            )?;
        }
        if version < 5 {
            connection.execute_batch(
                "BEGIN;
                 ALTER TABLE characters ADD COLUMN armor_class INTEGER;
                 ALTER TABLE characters ADD COLUMN initiative_override INTEGER;
                 ALTER TABLE characters ADD COLUMN speed TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN hit_point_maximum INTEGER;
                 ALTER TABLE characters ADD COLUMN current_hit_points INTEGER;
                 ALTER TABLE characters ADD COLUMN temporary_hit_points INTEGER;
                 ALTER TABLE characters ADD COLUMN hit_dice TEXT NOT NULL DEFAULT '';
                 ALTER TABLE characters ADD COLUMN death_successes INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE characters ADD COLUMN death_failures INTEGER NOT NULL DEFAULT 0;
                 PRAGMA user_version = 5;
                 COMMIT;",
            )?;
        }
        if version < 6 {
            connection.execute_batch(
                "BEGIN;
                 ALTER TABLE characters ADD COLUMN inspiration INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE characters ADD COLUMN proficiency_bonus_override INTEGER;
                 CREATE TABLE saving_throws (
                     character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                     ability_index INTEGER NOT NULL CHECK (ability_index BETWEEN 0 AND 5),
                     proficient INTEGER NOT NULL DEFAULT 0,
                     bonus_override INTEGER,
                     PRIMARY KEY (character_id, ability_index)
                 );
                 PRAGMA user_version = 6;
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
                    constitution, intelligence, wisdom, charisma, background,
                    player_name, race, alignment, experience_points, armor_class,
                    initiative_override, speed, hit_point_maximum, current_hit_points,
                    temporary_hit_points, hit_dice, death_successes, death_failures,
                    inspiration, proficiency_bonus_override
             FROM characters
             WHERE campaign_id = ?1 ORDER BY position, id",
        )?;
        let rows = query.query_map([campaign_id], |row| {
            Ok(Character {
                id: row.get(0)?,
                name: row.get(1)?,
                position: row.get(2)?,
                class: row.get(3)?,
                level: row.get(4)?,
                background: row.get(11)?,
                player_name: row.get(12)?,
                race: row.get(13)?,
                alignment: row.get(14)?,
                experience_points: row.get(15)?,
                armor_class: row.get(16)?,
                initiative_override: row.get(17)?,
                speed: row.get(18)?,
                hit_point_maximum: row.get(19)?,
                current_hit_points: row.get(20)?,
                temporary_hit_points: row.get(21)?,
                hit_dice: row.get(22)?,
                death_successes: row.get(23)?,
                death_failures: row.get(24)?,
                inspiration: row.get(25)?,
                proficiency_bonus_override: row.get(26)?,
                saving_throw_proficiencies: [false; 6],
                saving_throw_overrides: [None; 6],
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
        let mut characters = rows.collect::<Result<Vec<_>, _>>()?;
        let mut saves = self.connection.prepare(
            "SELECT ability_index, proficient, bonus_override FROM saving_throws
             WHERE character_id = ?1",
        )?;
        for character in &mut characters {
            let rows = saves.query_map([character.id], |row| {
                Ok((
                    row.get::<_, i64>(0)? as usize,
                    row.get::<_, bool>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?;
            for row in rows {
                let (index, proficient, override_bonus) = row?;
                character.saving_throw_proficiencies[index] = proficient;
                character.saving_throw_overrides[index] = override_bonus;
            }
        }
        Ok(characters)
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
            background: String::new(),
            player_name: String::new(),
            race: String::new(),
            alignment: String::new(),
            experience_points: None,
            armor_class: None,
            initiative_override: None,
            speed: String::new(),
            hit_point_maximum: None,
            current_hit_points: None,
            temporary_hit_points: None,
            hit_dice: String::new(),
            death_successes: 0,
            death_failures: 0,
            abilities: [None; 6],
            inspiration: false,
            proficiency_bonus_override: None,
            saving_throw_proficiencies: [false; 6],
            saving_throw_overrides: [None; 6],
        })
    }

    pub fn save_field(&self, character_id: i64, field: &str, value: &str) -> StorageResult<()> {
        let column = match field {
            "name"
            | "class"
            | "level"
            | "background"
            | "player_name"
            | "race"
            | "alignment"
            | "experience_points"
            | "strength"
            | "dexterity"
            | "constitution"
            | "intelligence"
            | "wisdom"
            | "charisma"
            | "armor_class"
            | "initiative_override"
            | "speed"
            | "hit_point_maximum"
            | "current_hit_points"
            | "temporary_hit_points"
            | "hit_dice"
            | "death_successes"
            | "death_failures"
            | "proficiency_bonus_override" => field,
            _ => return Err("Unknown character field".into()),
        };
        let sql = format!("UPDATE characters SET {column} = ?1 WHERE id = ?2");
        if matches!(
            column,
            "name"
                | "class"
                | "background"
                | "player_name"
                | "race"
                | "alignment"
                | "speed"
                | "hit_dice"
        ) {
            self.connection
                .execute(&sql, params![value, character_id])?;
        } else {
            let number = if value.trim().is_empty() {
                None
            } else {
                Some(value.trim().parse::<i64>()?)
            };
            if matches!(column, "death_successes" | "death_failures") {
                if number.is_none_or(|n| !(0..=3).contains(&n)) {
                    return Err("Death saves must be from 0 to 3".into());
                }
            } else if matches!(
                column,
                "experience_points"
                    | "armor_class"
                    | "hit_point_maximum"
                    | "current_hit_points"
                    | "temporary_hit_points"
            ) {
                if number.is_some_and(|n| n < 0) {
                    return Err("Enter a nonnegative number".into());
                }
            } else if !matches!(column, "initiative_override" | "proficiency_bonus_override")
                && number.is_some_and(|n| !(1..=30).contains(&n))
            {
                return Err("Enter a number from 1 to 30".into());
            }
            self.connection
                .execute(&sql, params![number, character_id])?;
        }
        Ok(())
    }

    pub fn save_inspiration(&self, character_id: i64, inspired: bool) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE characters SET inspiration = ?1 WHERE id = ?2",
            params![inspired, character_id],
        )?;
        Ok(())
    }

    pub fn save_saving_throw(
        &self,
        character_id: i64,
        ability_index: usize,
        proficient: bool,
        bonus_override: Option<i64>,
    ) -> StorageResult<()> {
        if ability_index >= 6 {
            return Err("Unknown saving throw".into());
        }
        self.connection.execute(
            "INSERT INTO saving_throws (character_id, ability_index, proficient, bonus_override)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(character_id, ability_index) DO UPDATE SET
                 proficient = excluded.proficient, bonus_override = excluded.bonus_override",
            params![
                character_id,
                ability_index as i64,
                proficient,
                bonus_override
            ],
        )?;
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
        reopened.save_field(id, "background", "Sage").unwrap();
        reopened.save_field(id, "experience_points", "0").unwrap();
        reopened
            .save_field(id, "proficiency_bonus_override", "+4")
            .unwrap();
        reopened.save_inspiration(id, true).unwrap();
        reopened.save_saving_throw(id, 1, true, Some(7)).unwrap();
        assert!(reopened.save_field(id, "experience_points", "-1").is_err());
        assert!(reopened.save_field(id, "experience_points", "1.5").is_err());
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
        assert_eq!(saved.background, "Sage");
        assert_eq!(saved.experience_points, Some(0));
        assert_eq!(saved.proficiency_bonus_override, Some(4));
        assert!(saved.inspiration);
        assert!(saved.saving_throw_proficiencies[1]);
        assert_eq!(saved.saving_throw_overrides[1], Some(7));
        drop(reopened);
        let reopened = Storage::open(&path).unwrap();
        assert_eq!(
            reopened
                .characters(reopened.campaigns().unwrap()[0].id)
                .unwrap()[0]
                .saving_throw_overrides[1],
            Some(7)
        );
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

    #[test]
    fn existing_character_database_gains_header_fields() {
        let path = std::env::temp_dir().join(format!(
            "dnd-handler-header-migration-{}-{}.sqlite3",
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
                 INSERT INTO campaigns (id, name) VALUES (1, 'Existing');
                 CREATE TABLE characters (
                     id INTEGER PRIMARY KEY, campaign_id INTEGER, name TEXT NOT NULL,
                     position INTEGER NOT NULL, class TEXT NOT NULL DEFAULT '', level INTEGER,
                     strength INTEGER, dexterity INTEGER, constitution INTEGER,
                     intelligence INTEGER, wisdom INTEGER, charisma INTEGER
                 );
                 INSERT INTO characters (campaign_id, name, position, class, level)
                     VALUES (1, 'Hero', 1, 'Fighter', 3);
                 PRAGMA user_version = 3;",
            )
            .unwrap();
        drop(connection);

        let storage = Storage::open(&path).unwrap();
        let character = &storage.characters(1).unwrap()[0];
        assert_eq!(character.name, "Hero");
        assert_eq!(character.class, "Fighter");
        assert_eq!(character.level, Some(3));
        assert_eq!(character.background, "");
        assert_eq!(character.experience_points, None);
        drop(storage);
        std::fs::remove_file(path).unwrap();
    }
}
