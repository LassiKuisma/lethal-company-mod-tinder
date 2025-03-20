use std::{collections::HashSet, error::Error};

use include_dir::{Dir, include_dir};
use rusqlite::{Connection, ToSql, params_from_iter};
use rusqlite_migration::Migrations;

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

	pub fn get_mods(
		&self,
		ignored_categories: HashSet<String>,
		limit: usize,
	) -> Result<Vec<Mod>, Box<dyn Error>> {
		let where_statement = if ignored_categories.len() != 0 {
			format!(
				"WHERE Categories.name NOT IN {}",
				repeat_vars(ignored_categories.len(), 1)
			)
		} else {
			"".to_string()
		};

		let sql = format!(
			r#"SELECT Mods.name, Mods.owner, Mods.description, Mods.iconUrl, Mods.packageUrl
			FROM Mods
			JOIN ModCategory ON Mods.id = ModCategory.modId
			JOIN Categories ON ModCategory.categoryId = Categories.id
			{where_statement}
			GROUP BY Mods.id
			ORDER BY Mods.updatedDate DESC
			LIMIT ?;"#
		);

		let mut statement = self.connection.prepare(&sql)?;

		let mut vars = ignored_categories
			.iter()
			.map::<Box<dyn ToSql>, _>(|i| Box::new(i))
			.collect::<Vec<_>>();

		vars.push(Box::new(limit));

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

	pub fn insert_categories(&self, categories: &HashSet<&String>) -> Result<(), Box<dyn Error>> {
		if categories.len() == 0 {
			return Ok(());
		}

		let sql = format!(
			"INSERT OR IGNORE INTO Categories(name) VALUES {};",
			repeat_vars(categories.len(), 1)
		);

		let mut statement = self.connection.prepare(&sql)?;
		statement.execute(params_from_iter(categories.iter()))?;

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

#[cfg(test)]
mod tests {
	use super::*;

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
}
