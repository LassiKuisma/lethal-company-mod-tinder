use std::{collections::HashSet, error::Error};

use sqlx::{FromRow, Pool, Postgres, QueryBuilder, Row, postgres::PgPoolOptions};
use time::Date;
use uuid::Uuid;

use crate::{
	mods::{Category, Mod, Rating},
	services::users::{User, UserNoId},
};

#[derive(Clone)]
pub struct Database {
	pool: Pool<Postgres>,
}

impl Database {
	pub async fn open_connection(
		db_url: &str,
		max_connection: u32,
	) -> Result<Self, Box<dyn Error>> {
		let pool = PgPoolOptions::new()
			.max_connections(max_connection)
			.connect(db_url)
			.await?;

		let db = Database { pool };
		db.apply_migrations().await?;

		Ok(db)
	}

	async fn apply_migrations(&self) -> Result<(), Box<dyn Error>> {
		let migrator = sqlx::migrate!("./migrations");
		migrator.run(&self.pool).await?;

		Ok(())
	}

	pub async fn get_mods(&self, options: &ModQueryOptions) -> Result<Vec<Mod>, Box<dyn Error>> {
		let mut builder = QueryBuilder::new(
			"SELECT mods.name, mods.owner, mods.description, mods.icon_url, mods.package_url, mods.id FROM mods ",
		);
		builder.push("WHERE mods.id NOT IN (SELECT mod_id FROM ratings) ");

		if !options.include_deprecated {
			builder.push("AND mods.deprecated = false ");
		}

		if !options.include_nsfw {
			builder.push("AND mods.nsfw = false ");
		}

		let ignored_categories = &options.ignored_categories;
		if ignored_categories.len() != 0 {
			builder.push(
				"AND mods.id NOT IN
					(SELECT mod_category.mod_id FROM mod_category
					JOIN categories ON categories.id = mod_category.category_id
					WHERE categories.name IN ",
			);

			builder.push_tuples(ignored_categories, |mut b, category| {
				b.push_bind(category);
			});

			builder.push(") ");
		}

		let query = builder
			.push("ORDER BY mods.updated_date DESC ")
			.push("LIMIT ")
			.push_bind(options.limit)
			.build();

		let mods = query
			.fetch_all(&self.pool)
			.await?
			.into_iter()
			.map(|row| Mod::from_row(&row))
			.collect::<Result<_, _>>()?;
		Ok(mods)
	}

	pub async fn insert_categories(
		&self,
		categories: &HashSet<impl ToString>,
	) -> Result<(), Box<dyn Error>> {
		if categories.len() == 0 {
			return Ok(());
		}

		let categories = categories.iter().map(|s| s.to_string()).collect::<Vec<_>>();

		let mut builder = QueryBuilder::new("INSERT INTO categories(name)");
		builder
			.push_values(&categories, |mut b, category| {
				b.push_bind(category);
			})
			.push("ON CONFLICT DO NOTHING;");

		let query = builder.build();
		query.execute(&self.pool).await?;

		Ok(())
	}

	pub async fn get_categories(&self) -> Result<Vec<Category>, Box<dyn Error>> {
		let categories = sqlx::query_as("SELECT id, name FROM categories;")
			.fetch_all(&self.pool)
			.await?;
		Ok(categories)
	}

	pub async fn insert_mods(
		&self,
		mods: &Vec<InsertMod<'_>>,
		chunk_size: usize,
	) -> Result<(), Box<dyn Error>> {
		self.clear_categories_junction_table().await?;

		let mod_chunks = mods.chunks(chunk_size);
		let mod_chunks_count = mod_chunks.len();

		for (index, chunk) in mod_chunks.enumerate() {
			log::debug!("Inserting mods chunk {}/{}", index + 1, mod_chunks_count);

			self.insert_mods_data(&chunk.iter().collect()).await?;
		}

		let mod_categories = mods
			.iter()
			.map(|m| {
				m.category_ids.iter().map(|c_id| InsertModCategory {
					mod_id: &m.uuid4,
					category_id: &c_id,
				})
			})
			.flatten()
			.collect::<Vec<_>>();

		let category_chunks = mod_categories.chunks(chunk_size);
		let category_chunks_count = category_chunks.len();
		for (index, chunk) in category_chunks.enumerate() {
			log::debug!(
				"Inserting mod category junction chunk {}/{}",
				index + 1,
				category_chunks_count
			);

			self.insert_mod_category_junction_data(&chunk.iter().collect())
				.await?;
		}

		Ok(())
	}

	async fn insert_mods_data(&self, mods: &Vec<&InsertMod<'_>>) -> Result<(), Box<dyn Error>> {
		if mods.len() == 0 {
			return Ok(());
		}

		let mut builder = QueryBuilder::new(
			"INSERT INTO mods (id, name, description, icon_url, full_name, owner, package_url, updated_date, rating, deprecated, nsfw) ",
		);

		builder.push_values(mods, |mut b, m| {
			b.push_bind(m.uuid4);
			b.push_bind(m.name);
			b.push_bind(m.description);
			b.push_bind(m.icon_url);
			b.push_bind(m.full_name);
			b.push_bind(m.owner);
			b.push_bind(m.package_url);
			b.push_bind(m.updated_date);
			b.push_bind(m.rating);
			b.push_bind(m.is_deprecated);
			b.push_bind(m.has_nsfw_content);
		});

		builder.push(
			" ON CONFLICT(id) DO UPDATE SET
name        =EXCLUDED.name,
description =EXCLUDED.description,
icon_url    =EXCLUDED.icon_url,
full_name   =EXCLUDED.full_name,
owner       =EXCLUDED.owner,
package_url =EXCLUDED.package_url,
updated_date=EXCLUDED.updated_date,
rating      =EXCLUDED.rating,
deprecated  =EXCLUDED.deprecated,
nsfw        =EXCLUDED.nsfw",
		);

		let query = builder.build();
		query.execute(&self.pool).await?;
		Ok(())
	}

	async fn insert_mod_category_junction_data(
		&self,
		mod_categories: &Vec<&InsertModCategory<'_>>,
	) -> Result<(), Box<dyn Error>> {
		if mod_categories.len() == 0 {
			return Ok(());
		}

		let mut builder = QueryBuilder::new("INSERT INTO mod_category (mod_id, category_id) ");
		builder.push_values(mod_categories, |mut b, mod_category| {
			b.push_bind(mod_category.mod_id)
				.push_bind(mod_category.category_id);
		});
		builder.push("ON CONFLICT DO NOTHING;");

		let query = builder.build();
		query.execute(&self.pool).await?;
		Ok(())
	}

	async fn clear_categories_junction_table(&self) -> Result<(), Box<dyn Error>> {
		sqlx::query("DELETE FROM mod_category;")
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn latest_mod_import_date(&self) -> Result<Option<Date>, Box<dyn Error>> {
		let result = sqlx::query("SELECT date FROM mods_imported_date WHERE id = 0;")
			.fetch_optional(&self.pool)
			.await?;

		if let Some(row) = result {
			let date = row.try_get::<Date, _>("date")?;
			Ok(Some(date))
		} else {
			// query was ok, but no data found -> no updates have been done to db
			Ok(None)
		}
	}

	pub async fn set_mods_imported_date(&self, date: Date) -> Result<(), Box<dyn Error>> {
		sqlx::query("INSERT INTO mods_imported_date (id, date) VALUES (0, $1) ON CONFLICT(id) DO UPDATE SET date = EXCLUDED.date;")
			.bind(date)
			.execute(&self.pool)
			.await?;

		Ok(())
	}

	pub async fn insert_mod_rating(
		&self,
		mod_id: &Uuid,
		rating: &Rating,
	) -> Result<(), Box<dyn Error>> {
		sqlx::query("INSERT INTO ratings(mod_id, rating) VALUES ($1, $2);")
			.bind(mod_id)
			.bind(rating)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn get_rated_mods(
		&self,
		rating: &Rating,
		limit: i16,
	) -> Result<Vec<Mod>, Box<dyn Error>> {
		let sql = "SELECT mods.name, mods.owner, mods.description, mods.icon_url, mods.package_url, mods.id
			FROM mods
			JOIN ratings ON mods.id = ratings.mod_id
			WHERE ratings.rating = $1
			LIMIT $2;";

		let mods = sqlx::query_as(sql)
			.bind(rating)
			.bind(limit)
			.fetch_all(&self.pool)
			.await?;

		Ok(mods)
	}

	pub async fn insert_user(&self, user: &UserNoId) -> Result<(), Box<dyn Error>> {
		sqlx::query("INSERT INTO users(username, password_hash) VALUES ($1, $2);")
			.bind(&user.username)
			.bind(&user.password_hash)
			.execute(&self.pool)
			.await?;

		Ok(())
	}

	pub async fn find_user(&self, name: &str) -> Result<Option<User>, Box<dyn Error>> {
		let result =
			sqlx::query_as("SELECT id, username, password_hash FROM users WHERE username = $1;")
				.bind(name)
				.fetch_optional(&self.pool)
				.await?;

		Ok(result)
	}
}

pub struct InsertMod<'a> {
	pub uuid4: Uuid,
	pub name: &'a String,
	pub description: &'a str,
	pub icon_url: &'a str,
	pub full_name: &'a String,
	pub owner: &'a String,
	pub package_url: &'a String,
	pub updated_date: Date,
	pub rating: i64,
	pub is_deprecated: bool,
	pub has_nsfw_content: bool,
	pub category_ids: HashSet<&'a i32>,
}

struct InsertModCategory<'a> {
	mod_id: &'a Uuid,
	category_id: &'a i32,
}

pub struct ModQueryOptions {
	pub ignored_categories: HashSet<String>,
	pub limit: i32,
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
	use time::format_description::well_known::Iso8601;

	fn hashset_of(items: Vec<&str>) -> HashSet<String> {
		items.into_iter().map(|s| s.to_string()).collect()
	}

	fn mod_names(mods: Vec<Mod>) -> HashSet<String> {
		mods.into_iter().map(|m| m.name).collect()
	}

	#[sqlx::test(fixtures("mods"))]
	async fn querying_mods_without_ignored_categories(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: Default::default(),
			limit: 100,
			include_deprecated: true,
			include_nsfw: true,
		};

		let result = db.get_mods(&query_options).await.unwrap();

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

	#[sqlx::test(fixtures("mods"))]
	async fn querying_mods_ignored_categories(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: hashset_of(vec!["Items", "Misc"]),
			limit: 100,
			include_deprecated: true,
			include_nsfw: true,
		};

		let result = db.get_mods(&query_options).await.unwrap();

		let expected = hashset_of(vec!["6th", "no-category", "new-update", "old-mod"]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[sqlx::test(fixtures("mods"))]
	async fn querying_mods_allowing_deprecated(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: Default::default(),
			limit: 100,
			include_deprecated: true,
			include_nsfw: false,
		};

		let result = db.get_mods(&query_options).await.unwrap();

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

	#[sqlx::test(fixtures("mods"))]
	async fn querying_non_deprecated_mods(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: Default::default(),
			limit: 100,
			include_deprecated: false,
			include_nsfw: false,
		};

		let result = db.get_mods(&query_options).await.unwrap();

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

	#[sqlx::test(fixtures("mods"))]
	async fn querying_non_deprecated_mods_ignoring_categories(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: hashset_of(vec!["Music", "Suits"]),
			limit: 100,
			include_deprecated: false,
			include_nsfw: false,
		};

		let result = db.get_mods(&query_options).await.unwrap();

		let expected = hashset_of(vec!["1st", "no-category", "new-update", "old-mod"]);

		let mod_names = mod_names(result);
		assert_eq!(expected, mod_names);
	}

	#[sqlx::test(fixtures("mods"))]
	async fn querying_non_deprecated_nswf_mods_ignoring_categories(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: hashset_of(vec!["TV", "Suits", "Misc"]),
			limit: 100,
			include_deprecated: false,
			include_nsfw: true,
		};

		let result = db.get_mods(&query_options).await.unwrap();

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

	#[sqlx::test(fixtures("mods"))]
	async fn querying_mods_most_recently_updated_is_first(pool: Pool<Postgres>) {
		let db = Database { pool };

		let query_options = ModQueryOptions {
			ignored_categories: Default::default(),
			limit: 4,
			include_deprecated: false,
			include_nsfw: false,
		};

		let result = db.get_mods(&query_options).await.unwrap();

		let expected = vec!["new-update", "1st", "5th", "6th"];

		let mods = result.iter().map(|m| m.name.as_str()).collect::<Vec<_>>();
		assert_eq!(expected, mods);
	}

	#[sqlx::test]
	async fn get_mod_import_date_from_empty_database(pool: Pool<Postgres>) {
		let db = Database { pool };

		let date = db.latest_mod_import_date().await.unwrap();
		assert_eq!(None, date);
	}

	#[sqlx::test]
	async fn set_and_get_mod_import_date(pool: Pool<Postgres>) {
		let db = Database { pool };

		let timestamp = Date::parse("2025-03-22T12:45:56.001122Z", &Iso8601::DEFAULT).unwrap();
		db.set_mods_imported_date(timestamp).await.unwrap();

		let date = db.latest_mod_import_date().await.unwrap().unwrap();
		assert_eq!(timestamp, date);
	}

	#[sqlx::test]
	async fn set_mod_import_date_multiple_times(pool: Pool<Postgres>) {
		let db = Database { pool };

		let old = Date::parse("2000-01-01T00:00:00.000000Z", &Iso8601::DEFAULT).unwrap();
		let mid = Date::parse("2002-02-22T00:00:00.000000Z", &Iso8601::DEFAULT).unwrap();
		let new = Date::parse("2025-03-03T03:03:03.000000Z", &Iso8601::DEFAULT).unwrap();

		db.set_mods_imported_date(old).await.unwrap();
		db.set_mods_imported_date(mid).await.unwrap();
		db.set_mods_imported_date(new).await.unwrap();

		let date = db.latest_mod_import_date().await.unwrap().unwrap();
		assert_eq!(new, date);
	}

	#[sqlx::test]
	async fn insert_and_query_categories(pool: Pool<Postgres>) {
		let db = Database { pool };
		let categories = hashset_of(vec!["Foo", "Bar", "Baz", "Cat", "Dog"]);
		db.insert_categories(&categories).await.unwrap();

		let result = db
			.get_categories()
			.await
			.unwrap()
			.into_iter()
			.map(|ctg| ctg.name)
			.collect::<HashSet<_>>();
		let expected = hashset_of(vec!["Foo", "Bar", "Baz", "Cat", "Dog"]);

		assert_eq!(expected, result);
	}

	#[sqlx::test]
	async fn inserting_and_querying_mods(pool: Pool<Postgres>) {
		let null = "".to_string();

		let db = Database { pool };
		db.insert_categories(&hashset_of(vec!["first", "second", "third"]))
			.await
			.unwrap();
		let categories = db.get_categories().await.unwrap();

		let m1 = Mod {
			name: "mod-1".to_string(),
			owner: "cat".to_string(),
			description: "first mod".to_string(),
			icon_url: "icon-1 url".to_string(),
			package_url: "package-1 url".to_string(),
			id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
		};
		let date_1 = Date::parse("2025-03-22T19:59:59.012345Z", &Iso8601::DEFAULT).unwrap();

		let m2 = Mod {
			name: "mod-2".to_string(),
			owner: "dog".to_string(),
			description: "second mod".to_string(),
			icon_url: "icon-2 url".to_string(),
			package_url: "package-2 url".to_string(),
			id: Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap(),
		};
		let date_2 = Date::parse("2025-03-22T22:22:22.222222Z", &Iso8601::DEFAULT).unwrap();

		let mods = vec![
			InsertMod {
				uuid4: m1.id.clone(),
				name: &m1.name,
				description: &m1.description,
				icon_url: &m1.icon_url,
				owner: &m1.owner,
				package_url: &m1.package_url,
				full_name: &null,
				updated_date: date_1,
				rating: 12345,
				is_deprecated: false,
				has_nsfw_content: false,
				category_ids: HashSet::from_iter(vec![
					&categories.get(0).unwrap().id,
					&categories.get(1).unwrap().id,
					&categories.get(2).unwrap().id,
				]),
			},
			InsertMod {
				uuid4: m2.id.clone(),
				name: &m2.name,
				description: &m2.description,
				icon_url: &m2.icon_url,
				owner: &m2.owner,
				package_url: &m2.package_url,
				full_name: &null,
				updated_date: date_2,
				rating: 54321,
				is_deprecated: true,
				has_nsfw_content: true,
				category_ids: HashSet::from_iter(vec![]),
			},
		];

		db.insert_mods(&mods, 150).await.unwrap();

		let query_options = ModQueryOptions {
			ignored_categories: Default::default(),
			limit: 100,
			include_deprecated: true,
			include_nsfw: true,
		};

		let mut result = db.get_mods(&query_options).await.unwrap();
		result.sort_by(|a, b| a.name.cmp(&b.name));

		let mut expected = vec![m1, m2];
		expected.sort_by(|a, b| a.name.cmp(&b.name));

		assert_eq!(expected, result);
	}

	#[sqlx::test(fixtures("mods"))]
	async fn rated_mods_are_omitted_from_queries(pool: Pool<Postgres>) {
		let db = Database { pool };

		db.insert_mod_rating(
			&Uuid::parse_str("00000000-0000-0000-0000-000000000005").unwrap(),
			&Rating::Like,
		)
		.await
		.unwrap();
		db.insert_mod_rating(
			&Uuid::parse_str("00000000-0000-0000-0000-000000000006").unwrap(),
			&Rating::Dislike,
		)
		.await
		.unwrap();

		let query_options = ModQueryOptions {
			ignored_categories: Default::default(),
			limit: 100,
			include_deprecated: true,
			include_nsfw: true,
		};

		let result = db.get_mods(&query_options).await.unwrap();

		let mods = mod_names(result);
		let expected = hashset_of(vec![
			"1st",
			"dep-mod",
			"nsfw-mod",
			"dep-nsfw",
			"nsfw-2",
			"no-category",
			"new-update",
			"old-mod",
		]);

		assert_eq!(expected, mods);
	}

	#[sqlx::test(fixtures("mods"))]
	async fn querying_rated_mods(pool: Pool<Postgres>) {
		let db = Database { pool };

		db.insert_mod_rating(
			&Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
			&Rating::Like,
		)
		.await
		.unwrap();
		db.insert_mod_rating(
			&Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
			&Rating::Dislike,
		)
		.await
		.unwrap();
		db.insert_mod_rating(
			&Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap(),
			&Rating::Dislike,
		)
		.await
		.unwrap();
		db.insert_mod_rating(
			&Uuid::parse_str("00000000-0000-0000-0000-000000000005").unwrap(),
			&Rating::Like,
		)
		.await
		.unwrap();

		let result = db.get_rated_mods(&Rating::Like, 100).await.unwrap();

		let mods = mod_names(result);
		let expected = hashset_of(vec!["dep-mod", "5th"]);

		assert_eq!(expected, mods);
	}

	#[sqlx::test]
	async fn insert_and_find_users(pool: Pool<Postgres>) {
		let db = Database { pool };

		let first = UserNoId {
			username: "First".to_string(),
			password_hash: "aaaa".to_string(),
		};

		let second = UserNoId {
			username: "Second".to_string(),
			password_hash: "bbbb".to_string(),
		};

		let third = UserNoId {
			username: "Third".to_string(),
			password_hash: "cccc".to_string(),
		};

		db.insert_user(&first).await.unwrap();
		db.insert_user(&second).await.unwrap();
		db.insert_user(&third).await.unwrap();

		let result = db
			.find_user("Second")
			.await
			.unwrap()
			.expect("No user found");
		assert_eq!(result.username, "Second");
		assert_eq!(result.password_hash, "bbbb");
	}

	#[sqlx::test]
	async fn inserting_non_unique_user(pool: Pool<Postgres>) {
		let db = Database { pool };

		let first = UserNoId {
			username: "Taken".to_string(),
			password_hash: "aaaa".to_string(),
		};

		let second = UserNoId {
			username: "Taken".to_string(),
			password_hash: "bbbb".to_string(),
		};

		let _ = db.insert_user(&first).await.unwrap();

		let result = db.insert_user(&second).await;
		assert!(result.is_err());
	}
}
