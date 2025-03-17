use std::{collections::HashSet, error::Error};

use include_dir::{Dir, include_dir};
use rusqlite::{Connection, named_params};
use rusqlite_migration::Migrations;

use crate::mods::Category;

const DB_PATH: &str = "data/db.db";

pub struct Database {
	connection: Connection,
}

impl Database {
	pub fn open_connection() -> Result<Self, Box<dyn Error>> {
		let mut connection = Connection::open(DB_PATH)?;
		apply_migrations(&mut connection)?;

		Ok(Database { connection })
	}

	pub fn insert_categories(&self, categories: &HashSet<&String>) -> Result<(), Box<dyn Error>> {
		let mut statement = self
			.connection
			.prepare("INSERT OR IGNORE INTO Categories(name) VALUES (:name);")?;

		for name in categories.iter() {
			statement.execute(named_params! {
				":name": name
			})?;
		}

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
		self.insert_mods_data(mods)?;
		Ok(())
	}

	fn insert_mods_data(&self, mods: &Vec<InsertMod>) -> Result<(), Box<dyn Error>> {
		let mut insert_mod = self.connection.prepare(
			r#"INSERT OR REPLACE INTO Mods
			(id,   name,  description,  iconUrl,  fullName,  owner,  packageUrl,  updatedDate,  rating,  deprecated,  nsfw) VALUES
			(:id, :name, :description, :iconUrl, :fullName, :owner, :packageUrl, :updatedDate, :rating, :deprecated, :nsfw);"#
			)?;

		let mut insert_category = self.connection.prepare(
			"INSERT OR IGNORE INTO ModCategory (modId, categoryId) VALUES (:modId, :categoryId);",
		)?;

		for m in mods.iter() {
			insert_mod.execute(named_params! {
				":id": m.uuid4,
				":name": m.name,
				":description": m.description,
				":iconUrl": m.icon_url,
				":fullName": m.full_name,
				":owner": m.owner,
				":packageUrl": m.package_url,
				":updatedDate": m.updated_date,
				":rating": m.rating,
				":deprecated": m.is_deprecated,
				":nsfw": m.has_nsfw_content,
			})?;

			for category_id in m.category_ids.iter() {
				insert_category.execute(named_params! {
					":modId": m.uuid4,
					":categoryId": category_id,
				})?;
			}
		}

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
