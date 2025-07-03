use std::{
	collections::{HashMap, HashSet},
	error::Error,
	fmt::Display,
	path::Path,
	string::FromUtf8Error,
	time::Duration,
};

use async_curl::{Actor, CurlActor};
use curl::easy::{Easy2, Handler, WriteError};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use time::{Date, OffsetDateTime, format_description::well_known::Iso8601};
use uuid::Uuid;

use crate::{
	db::{Database, InsertMod},
	env::Env,
};

type Mods = Vec<ModRaw>;

const CACHE_FILE: &str = "data/mods_cache.json";
const THUNDERSTORE_API_URL: &str = "https://thunderstore.io/c/lethal-company/api/v1/package/";

#[allow(dead_code)]
#[derive(Debug, PartialEq, Serialize, Eq, FromRow)]
pub struct Mod {
	pub name: String,
	pub owner: String,
	pub description: String,
	pub icon_url: String,
	pub package_url: String,
	pub id: Uuid,
	pub categories: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Hash, FromRow, Serialize)]
pub struct Category {
	pub name: String,
	pub id: i32,
}

#[derive(Debug, Deserialize, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "rating_type")]
pub enum Rating {
	Like,
	Dislike,
}

impl Display for Rating {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self)
	}
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ModRaw {
	name: String,
	full_name: String,
	owner: String,
	package_url: String,
	donation_link: Option<String>,
	date_created: String,
	date_updated: String,
	uuid4: String,
	rating_score: i64,
	is_pinned: bool,
	is_deprecated: bool,
	has_nsfw_content: bool,
	categories: Vec<String>,
	versions: Vec<ModVersion>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ModVersion {
	name: String,
	full_name: String,
	description: String,
	icon: String,
	version_number: String,
	dependencies: Vec<String>,
	download_url: String,
	downloads: i64,
	date_created: String,
	website_url: String,
	is_active: bool,
	uuid4: String,
	file_size: i64,
}

impl ModRaw {
	fn to_insertable<'a>(
		&'a self,
		categories: &'a HashMap<String, Category>,
	) -> Result<InsertMod<'a>, Box<dyn Error>> {
		// assume that the first version in list in the most recent
		let most_recent = self.versions.first();

		let (description, icon_url) = if let Some(most_recent) = most_recent {
			(most_recent.description.as_str(), most_recent.icon.as_str())
		} else {
			log::error!(
				"Faulty entry for mod '{}' (id='{}'): mod info found, but no versions of the mod found.",
				self.name,
				self.uuid4
			);

			("<No description available>", "")
		};

		let category_ids = self
			.categories
			.iter()
			.map(|ct_name| {
				let category = categories.get(ct_name);
				if category.is_none() {
					log::error!(
						"Faulty entry for mod '{}' (id='{}'): can't find category id of '{}'",
						self.name,
						self.uuid4,
						ct_name
					);
				}
				return category;
			})
			.filter_map(|option| option)
			.map(|ct| &ct.id)
			.collect::<HashSet<_>>();

		let uuid = Uuid::try_parse(&self.uuid4)?;
		let date = Date::parse(&self.date_updated, &Iso8601::DEFAULT)?;

		Ok(InsertMod {
			uuid4: uuid,
			name: &self.name,
			description,
			icon_url,
			full_name: &self.full_name,
			owner: &self.owner,
			package_url: &self.package_url,
			updated_date: date,
			rating: self.rating_score,
			is_deprecated: self.is_deprecated,
			has_nsfw_content: self.has_nsfw_content,
			category_ids,
		})
	}
}

pub async fn import_mods_if_expired(db: &Database, env: &Env) -> Result<(), Box<dyn Error>> {
	if are_mods_expired(db, env).await? {
		do_import_mods(db, env).await?;
	}

	Ok(())
}

pub async fn are_mods_expired(db: &Database, env: &Env) -> Result<bool, Box<dyn Error>> {
	let options = env.mod_refresh_options.clone();

	let duration = match options {
		ModRefreshOptions::NoRefresh => return Ok(false),
		ModRefreshOptions::CacheOnly(duration) => duration,
		ModRefreshOptions::DownloadIfExpired(duration) => duration,
	};

	let last_import = db.latest_mod_import_date().await?;
	let now = OffsetDateTime::now_utc();
	let result = is_expired(last_import, now, duration);

	return Ok(result);
}

pub async fn do_import_mods(db: &Database, env: &Env) -> Result<(), Box<dyn Error>> {
	let options = env.mod_refresh_options.clone();

	if options == ModRefreshOptions::NoRefresh {
		return Ok(());
	}

	let should_download_mods = match options {
		ModRefreshOptions::DownloadIfExpired(_) => true,
		_ => false,
	};

	if should_download_mods {
		let mods_json = download_mods_json().await?;
		save_mods_to_cache(&mods_json)?;
	}

	let mods = load_mods_from_cache()?;
	save_mods_to_db(db, &mods, env).await?;
	db.set_mods_imported_date(OffsetDateTime::now_utc()).await?;

	Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct ResponseHandler {
	data: Vec<u8>,
}

impl Handler for ResponseHandler {
	fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
		self.data.extend_from_slice(data);
		Ok(data.len())
	}
}

impl ResponseHandler {
	fn new() -> Self {
		Self::default()
	}

	fn to_string(self) -> Result<String, FromUtf8Error> {
		String::from_utf8(self.data)
	}
}

async fn download_mods_json() -> Result<String, Box<dyn Error>> {
	assert!(!cfg!(test), "Trying to load mod cache in tests");

	let mut easy = Easy2::new(ResponseHandler::new());
	easy.url(THUNDERSTORE_API_URL)?;
	easy.get(true)?;

	log::info!("Starting mods json download");
	let actor = CurlActor::new();
	let result = actor
		.send_request(easy)
		.await?
		.get_ref()
		.to_owned()
		.to_string()?;

	Ok(result)
}

fn save_mods_to_cache(mods_json: &String) -> Result<(), Box<dyn Error>> {
	assert!(!cfg!(test), "Trying to save mods to cache in tests");

	log::debug!("Saving mods json to cache");

	let path = Path::new(CACHE_FILE);
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}

	std::fs::write(path, mods_json)?;
	Ok(())
}

fn load_mods_from_cache() -> Result<Mods, Box<dyn Error>> {
	let str = std::fs::read_to_string(CACHE_FILE)?;
	let mods = serde_json::from_str(&str)?;
	Ok(mods)
}

fn is_expired(
	last_import: Option<OffsetDateTime>,
	now: OffsetDateTime,
	expiration_duration: Duration,
) -> bool {
	if let Some(last_import) = last_import {
		let time_passed = now - last_import;
		time_passed > expiration_duration
	} else {
		// no previous value present -> this is first time
		true
	}
}

#[allow(dead_code)]
#[derive(PartialEq, Eq, Clone)]
pub enum ModRefreshOptions {
	NoRefresh,
	CacheOnly(Duration),
	DownloadIfExpired(Duration),
}

async fn save_mods_to_db(
	db: &Database,
	mods: &Vec<ModRaw>,
	env: &Env,
) -> Result<(), Box<dyn Error>> {
	let category_names = mods
		.iter()
		.map(|modd| modd.categories.iter())
		.flatten()
		.collect::<HashSet<_>>();

	log::info!("Saving mod categories to db");
	db.insert_categories(&category_names).await?;

	let categories = db
		.get_categories()
		.await?
		.into_iter()
		.map(|ct| (ct.name.clone(), ct))
		.collect::<HashMap<String, Category>>();

	let mods = mods
		.iter()
		.filter_map(|m| {
			m.to_insertable(&categories)
				.inspect_err(|err| {
					log::warn!(
						"Failed to convert mod '{}' (id={}) to SQL-insertable: {}",
						m.name,
						m.uuid4,
						err
					)
				})
				.ok()
		})
		.collect();
	log::info!("Savings mods to db");
	db.insert_mods(&mods, env.sql_chunk_size).await?;
	Ok(())
}
