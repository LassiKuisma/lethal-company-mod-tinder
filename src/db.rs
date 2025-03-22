use std::{collections::HashSet, error::Error};

use include_dir::{Dir, include_dir};
use rusqlite::{Connection, OptionalExtension, ToSql, params_from_iter};
use rusqlite_migration::Migrations;
use time::{UtcDateTime, format_description::well_known::Iso8601};

use crate::mods::{Category, Mod};

const DB_PATH: &str = "data/db.db";

pub struct Database {
	connection: Connection,
}

impl Database {
	pub fn open_connection() -> Result<Self, Box<dyn Error>> {
		assert!(!cfg!(test), "Trying to open db connection in tests");

		let mut connection = Connection::open(DB_PATH)?;
		apply_migrations(&mut connection)?;

		Ok(Database { connection })
	}

	pub fn get_mods(&self, options: &ModQueryOptions) -> Result<Vec<Mod>, Box<dyn Error>> {
		let ignored_categories = &options.ignored_categories;

		let mut filters = Vec::<String>::new();

		if ignored_categories.len() != 0 {
			let category_filter = format!(
				"Mods.id NOT IN
					(SELECT ModCategory.modId FROM ModCategory
					JOIN Categories ON Categories.id = ModCategory.categoryId
					WHERE Categories.name IN {})",
				repeat_vars(1, ignored_categories.len())
			);

			filters.push(category_filter);
		}

		if !options.include_deprecated {
			filters.push("Mods.deprecated = false".to_string());
		}

		if !options.include_nsfw {
			filters.push("Mods.nsfw = false".to_string());
		}

		let where_statement = if filters.len() != 0 {
			let joined = filters.join(" AND ");
			format!("WHERE {joined}")
		} else {
			"".to_string()
		};

		let sql = format!(
			r#"SELECT Mods.name, Mods.owner, Mods.description, Mods.iconUrl, Mods.packageUrl
			FROM Mods
			{where_statement}
			ORDER BY Mods.updatedDate DESC
			LIMIT ?;"#
		);

		let mut statement = self.connection.prepare(&sql)?;

		let mut vars = ignored_categories
			.iter()
			.map::<Box<dyn ToSql>, _>(|i| Box::new(i))
			.collect::<Vec<_>>();

		vars.push(Box::new(options.limit));

		let mods = statement
			.query_map(params_from_iter(vars), |row| {
				Ok(Mod {
					name: row.get(0)?,
					owner: row.get(1)?,
					description: row.get(2)?,
					icon: row.get(3)?,
					package_url: row.get(4)?,
				})
			})?
			.collect::<Result<_, _>>()?;

		Ok(mods)
	}

	pub fn insert_categories(
		&self,
		categories: &HashSet<impl ToString>,
	) -> Result<(), Box<dyn Error>> {
		if categories.len() == 0 {
			return Ok(());
		}

		let sql = format!(
			"INSERT OR IGNORE INTO Categories(name) VALUES {};",
			repeat_vars(categories.len(), 1)
		);

		let mut statement = self.connection.prepare(&sql)?;
		statement.execute(params_from_iter(categories.iter().map(|s| s.to_string())))?;

		Ok(())
	}

	pub fn get_categories(&self) -> Result<Vec<Category>, Box<dyn Error>> {
		let mut statement = self
			.connection
			.prepare("SELECT id, name FROM Categories;")?;

		let categories = statement
			.query_map([], |row| {
				Ok(Category {
					id: row.get(0)?,
					name: row.get(1)?,
				})
			})?
			.collect::<Result<Vec<_>, _>>()?;

		Ok(categories)
	}

	pub fn insert_mods(&self, mods: &Vec<InsertMod>) -> Result<(), Box<dyn Error>> {
		self.clear_categories_junction_table()?;

		let chunk_size = 10000;
		let mod_chunks = mods.chunks(chunk_size);
		let mod_chunks_count = mod_chunks.len();

		for (index, chunk) in mod_chunks.enumerate() {
			println!("Inserting mods chunk {}/{}", index + 1, mod_chunks_count);

			self.insert_mods_data(&chunk.iter().collect())?;
		}

		let mod_categories = mods
			.iter()
			.map(|m| {
				m.category_ids.iter().map(|c_id| InsertModCategory {
					mod_id: m.uuid4,
					category_id: &c_id,
				})
			})
			.flatten()
			.collect::<Vec<_>>();

		let category_chunks = mod_categories.chunks(chunk_size);
		let category_chunks_count = category_chunks.len();
		for (index, chunk) in category_chunks.enumerate() {
			println!(
				"Inserting mod category junction chunk {}/{}",
				index + 1,
				category_chunks_count
			);

			self.insert_mod_category_junction_data(&chunk.iter().collect())?;
		}

		Ok(())
	}

	fn insert_mods_data(&self, mods: &Vec<&InsertMod>) -> Result<(), Box<dyn Error>> {
		if mods.len() == 0 {
			return Ok(());
		}

		let sql = format!(
			r#"INSERT OR REPLACE INTO Mods
			(id, name, description, iconUrl, fullName, owner, packageUrl, updatedDate, rating, deprecated, nsfw)
			VALUES {};"#,
			repeat_vars(mods.len(), 11)
		);
		let mut statement = self.connection.prepare(&sql)?;

		let variables = mods.iter().map(|m| m.get_sql_vars()).flatten();
		statement.execute(params_from_iter(variables))?;

		Ok(())
	}

	fn insert_mod_category_junction_data(
		&self,
		mod_categories: &Vec<&InsertModCategory>,
	) -> Result<(), Box<dyn Error>> {
		if mod_categories.len() == 0 {
			return Ok(());
		}

		let sql = format!(
			"INSERT OR IGNORE INTO ModCategory (modId, categoryId) VALUES {};",
			repeat_vars(mod_categories.len(), 2)
		);

		let mut statement = self.connection.prepare(&sql)?;
		let variables = mod_categories.iter().map(|mc| mc.get_sql_vars()).flatten();

		statement.execute(params_from_iter(variables))?;

		Ok(())
	}

	fn clear_categories_junction_table(&self) -> Result<(), Box<dyn Error>> {
		let mut statement = self.connection.prepare("DELETE FROM ModCategory;")?;
		statement.execute([])?;
		Ok(())
	}

	pub fn latest_mod_update_date(&self) -> Result<Option<UtcDateTime>, Box<dyn Error>> {
		let mut statement = self
			.connection
			.prepare("SELECT date FROM ModsUpdatedDate WHERE ModsUpdatedDate.id = 0;")?;

		let result: Option<String> = statement.query_row([], |row| Ok(row.get(0)?)).optional()?;

		if let Some(date) = result {
			let date = UtcDateTime::parse(&date, &Iso8601::DEFAULT)?;
			Ok(Some(date))
		} else {
			// query was ok, but no data found -> no updates have been done to db
			Ok(None)
		}
	}

	pub fn set_mods_updated_date(&self, timestamp: UtcDateTime) -> Result<(), Box<dyn Error>> {
		let mut statement = self
			.connection
			.prepare("INSERT INTO ModsUpdatedDate (id, date) VALUES (0, ?);")?;

		let str = timestamp.format(&Iso8601::DEFAULT)?;
		statement.execute([str])?;
		return Ok(());
	}
}

fn apply_migrations(connection: &mut Connection) -> Result<(), Box<dyn Error>> {
	static MIGRATION_DIR: Dir = include_dir!("migrations");
	let migrations = Migrations::from_directory(&MIGRATION_DIR).unwrap();
	migrations.to_latest(connection)?;
	Ok(())
}

pub struct InsertMod<'a> {
	pub uuid4: &'a String,
	pub name: &'a String,
	pub description: &'a str,
	pub icon_url: &'a str,
	pub full_name: &'a String,
	pub owner: &'a String,
	pub package_url: &'a String,
	pub updated_date: &'a String,
	pub rating: i64,
	pub is_deprecated: bool,
	pub has_nsfw_content: bool,
	pub category_ids: HashSet<&'a i64>,
}

impl<'a> InsertMod<'a> {
	fn get_sql_vars(&self) -> Vec<Box<dyn ToSql + '_>> {
		vec![
			Box::new(&self.uuid4),
			Box::new(&self.name),
			Box::new(&self.description),
			Box::new(&self.icon_url),
			Box::new(&self.full_name),
			Box::new(&self.owner),
			Box::new(&self.package_url),
			Box::new(&self.updated_date),
			Box::new(&self.rating),
			Box::new(&self.is_deprecated),
			Box::new(&self.has_nsfw_content),
		]
	}
}

struct InsertModCategory<'a> {
	mod_id: &'a String,
	category_id: &'a i64,
}

impl<'a> InsertModCategory<'a> {
	fn get_sql_vars(&self) -> Vec<Box<dyn ToSql + '_>> {
		vec![Box::new(&self.mod_id), Box::new(&self.category_id)]
	}
}

fn repeat_vars(item_count: usize, variable_count: usize) -> String {
	let mut inner = "?,".repeat(variable_count);
	// Remove trailing comma
	inner.pop();
	let mut outer = format!("({inner}),").repeat(item_count);
	// Remove trailing comma
	outer.pop();
	outer
}

pub struct ModQueryOptions {
	pub ignored_categories: HashSet<String>,
	pub limit: usize,
	pub include_deprecated: bool,
	pub include_nsfw: bool,
}

impl Default for ModQueryOptions {
	fn default() -> Self {
		Self {
			ignored_categories: Default::default(),
			limit: 20,
			include_deprecated: false,
			include_nsfw: false,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	impl Database {
		fn open_in_memory() -> Database {
			let mut connection = Connection::open_in_memory().unwrap();
			apply_migrations(&mut connection).unwrap();

			Database { connection }
		}

		fn insert_test_data(&self) {
			let insert_categories = r#"
			INSERT INTO Categories(id, name) VALUES
			(0, 'Suits'),
			(1, 'Music'),
			(2, 'TV'),
			(3, 'Items'),
			(4, 'Misc');
			"#;

			let insert_mods = r#"
			INSERT INTO Mods
			(id,   name,          updatedDate,                   deprecated, nsfw,  description, iconUrl, fullName, owner, packageUrl, rating) VALUES
			('1',  '1st',         '2025-03-20T10:00:00.000000Z', false,      false, '',          '',      '',       '',    '',         0),
			('2',  'dep-mod',     '2025-03-20T09:00:00.000000Z', true,       false, '',          '',      '',       '',    '',         0),
			('3',  'nsfw-mod',    '2025-03-20T08:00:00.000000Z', false,      true,  '',          '',      '',       '',    '',         0),
			('4',  'dep-nsfw',    '2025-03-20T07:00:00.000000Z', true,       true,  '',          '',      '',       '',    '',         0),
			('5',  '5th',         '2025-03-09T00:00:00.000000Z', false,      false, '',          '',      '',       '',    '',         0),
			('6',  '6th',         '2025-03-08T00:00:00.000000Z', false,      false, '',          '',      '',       '',    '',         0),
			('7',  'nsfw-2',      '2025-03-07T00:00:00.000000Z', false,      true,  '',          '',      '',       '',    '',         0),
			('8',  'no-category', '2025-03-06T00:00:00.000000Z', false,      false, '',          '',      '',       '',    '',         0),
			('9',  'new-update',  '2025-03-21T00:00:00.000000Z', false,      false, '',          '',      '',       '',    '',         0),
			('10', 'old-mod',     '2020-01-01T00:00:00.000000Z', false,      false, '',          '',      '',       '',    '',         0);
			"#;

			let insert_mod_category = r#"
			INSERT INTO ModCategory(categoryId, modId) VALUES
			(1, 5),
			(1, 6),

			(2, 5),

			(3, 1),
			(3, 2),
			(3, 3),
			(3, 4),
			(3, 5),

			(4, 1),
			(4, 5),
			(4, 7);
			"#;

			self.connection.execute(insert_categories, []).unwrap();
			self.connection.execute(insert_mods, []).unwrap();
			self.connection.execute(insert_mod_category, []).unwrap();
		}
	}

	fn hashset_of(items: Vec<&str>) -> HashSet<String> {
		items.into_iter().map(|s| s.to_string()).collect()
	}

	fn mod_names(mods: Vec<Mod>) -> HashSet<String> {
		mods.into_iter().map(|m| m.name).collect()
	}

	#[test]
	fn repeating_vars_works_correctly() {
		assert_eq!(repeat_vars(1, 1), "(?)", "one variable repeated once");

		assert_eq!(
			repeat_vars(3, 1),
			"(?),(?),(?)",
			"one variable repeated many times"
		);

		assert_eq!(
			repeat_vars(1, 4),
			"(?,?,?,?)",
			"multiple variables repeated once"
		);

		assert_eq!(
			repeat_vars(3, 2),
			"(?,?),(?,?),(?,?)",
			"multiple variables repeated many times"
		);
	}

	#[test]
	fn querying_mods_without_ignored_categories() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: Default::default(),
				limit: 100,
				include_deprecated: true,
				include_nsfw: true,
			})
			.unwrap();

		let expected = hashset_of(vec![
			"1st",
			"dep-mod",
			"nsfw-mod",
			"dep-nsfw",
			"5th",
			"6th",
			"nsfw-2",
			"no-category",
			"new-update",
			"old-mod",
		]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[test]
	fn querying_mods_ignored_categories() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: hashset_of(vec!["Items", "Misc"]),
				limit: 100,
				include_deprecated: true,
				include_nsfw: true,
			})
			.unwrap();

		let expected = hashset_of(vec!["6th", "no-category", "new-update", "old-mod"]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[test]
	fn querying_mods_allowing_deprecated() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: Default::default(),
				limit: 100,
				include_deprecated: true,
				include_nsfw: false,
			})
			.unwrap();

		let expected = hashset_of(vec![
			"1st",
			"dep-mod",
			"5th",
			"6th",
			"no-category",
			"new-update",
			"old-mod",
		]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[test]
	fn querying_non_deprecated_mods() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: Default::default(),
				limit: 100,
				include_deprecated: false,
				include_nsfw: false,
			})
			.unwrap();

		let expected = hashset_of(vec![
			"1st",
			"5th",
			"6th",
			"no-category",
			"new-update",
			"old-mod",
		]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[test]
	fn querying_non_deprecated_mods_ignoring_categories() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: hashset_of(vec!["Music", "Suits"]),
				limit: 100,
				include_deprecated: false,
				include_nsfw: false,
			})
			.unwrap();

		let expected = hashset_of(vec!["1st", "no-category", "new-update", "old-mod"]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[test]
	fn querying_non_deprecated_nswf_mods_ignoring_categories() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: hashset_of(vec!["TV", "Suits", "Misc"]),
				limit: 100,
				include_deprecated: false,
				include_nsfw: true,
			})
			.unwrap();

		let expected = hashset_of(vec![
			"nsfw-mod",
			"6th",
			"no-category",
			"new-update",
			"old-mod",
		]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[test]
	fn querying_mods_most_recently_updated_is_first() {
		let db = Database::open_in_memory();
		db.insert_test_data();

		let result = db
			.get_mods(&ModQueryOptions {
				ignored_categories: Default::default(),
				limit: 4,
				include_deprecated: false,
				include_nsfw: false,
			})
			.unwrap();

		let expected = vec!["new-update", "1st", "5th", "6th"];

		let mods = result.iter().map(|m| m.name.as_str()).collect::<Vec<_>>();
		assert_eq!(expected, mods);
	}

	#[test]
	fn get_mod_update_date_from_empty_database() {
		let db = Database::open_in_memory();

		let last_update = db.latest_mod_update_date().unwrap();
		assert_eq!(None, last_update);
	}

	#[test]
	fn set_and_get_mod_update_date() {
		let db = Database::open_in_memory();

		let timestamp =
			UtcDateTime::parse("2025-03-22T12:45:56.001122Z", &Iso8601::DEFAULT).unwrap();
		db.set_mods_updated_date(timestamp).unwrap();

		let latest_update = db.latest_mod_update_date().unwrap().unwrap();
		assert_eq!(timestamp, latest_update);
	}

	#[test]
	fn set_mod_update_date_multiple_times() {
		let db = Database::open_in_memory();

		let old = UtcDateTime::parse("2000-01-01T00:00:00.000000Z", &Iso8601::DEFAULT).unwrap();
		let mid = UtcDateTime::parse("2002-02-22T00:00:00.000000Z", &Iso8601::DEFAULT).unwrap();
		let new = UtcDateTime::parse("2025-03-03T03:03:03.000000Z", &Iso8601::DEFAULT).unwrap();

		db.set_mods_updated_date(old).unwrap();
		db.set_mods_updated_date(mid).unwrap();
		db.set_mods_updated_date(new).unwrap();

		let latest_update = db.latest_mod_update_date().unwrap().unwrap();
		assert_eq!(new, latest_update);
	}

	#[test]
	fn insert_and_query_categories() {
		let db = Database::open_in_memory();
		let categories = hashset_of(vec!["Foo", "Bar", "Baz", "Cat", "Dog"]);
		db.insert_categories(&categories).unwrap();

		let result = db
			.get_categories()
			.unwrap()
			.into_iter()
			.map(|ctg| ctg.name)
			.collect::<HashSet<_>>();
		let expected = hashset_of(vec!["Foo", "Bar", "Baz", "Cat", "Dog"]);

		assert_eq!(expected, result);
	}
}
