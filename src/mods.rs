use std::{
	collections::{HashMap, HashSet},
	error::Error,
	path::Path,
	time::{Duration, Instant},
};

use curl::easy::Easy;
use serde::Deserialize;
use time::UtcDateTime;

use crate::db::{Database, InsertMod};

type Mods = Vec<ModRaw>;

const CACHE_FILE: &str = "data/mods_cache.json";
const THUNDERSTORE_API_URL: &str = "https://thunderstore.io/c/lethal-company/api/v1/package/";

#[allow(dead_code)]
pub struct Mod {
	pub name: String,
	pub owner: String,
	pub description: String,
	pub icon: String,
	pub package_url: String,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Category {
	pub name: String,
	pub id: i64,
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
	fn to_insertable<'a>(&'a self, categories: &'a HashMap<String, Category>) -> InsertMod<'a> {
		// assume that the first version in list in the most recent
		let most_recent = self.versions.first();

		let (description, icon_url) = if let Some(most_recent) = most_recent {
			(most_recent.description.as_str(), most_recent.icon.as_str())
		} else {
			println!(
				"Faulty entry for mod '{}' (id='{}'): mod info found, but no versions of the mod found.",
				self.name, self.uuid4
			);

			("<No description available>", "")
		};

		let category_ids = self
			.categories
			.iter()
			.map(|ct_name| {
				let category = categories.get(ct_name);
				if category.is_none() {
					println!(
						"Faulty entry for mod '{}' (id='{}'): can't find category id of '{}'",
						self.name, self.uuid4, ct_name
					);
				}
				return category;
			})
			.filter_map(|option| option)
			.map(|ct| &ct.id)
			.collect::<HashSet<_>>();

		InsertMod {
			uuid4: &self.uuid4,
			name: &self.name,
			description,
			icon_url,
			full_name: &self.full_name,
			owner: &self.owner,
			package_url: &self.package_url,
			updated_date: &self.date_updated,
			rating: self.rating_score,
			is_deprecated: self.is_deprecated,
			has_nsfw_content: self.has_nsfw_content,
			category_ids,
		}
	}
}

#[allow(dead_code)]
pub fn refresh_mods(db: &Database, options: ModRefreshOptions) -> Result<(), Box<dyn Error>> {
	let should_update_cache = match options {
		ModRefreshOptions::ForceDownload => true,
		ModRefreshOptions::CacheOnly => false,
		ModRefreshOptions::DownloadIfExpired(duration) => {
			let last_update = db.latest_mod_update_date()?;
			is_expired(last_update, UtcDateTime::now(), duration)
		}
	};

	if should_update_cache {
		let mods_json = load_mods_json()?;
		save_mods_to_cache(&mods_json)?;
		db.set_mods_updated_date(UtcDateTime::now())?;
	}

	// do "cache -> db" if we updated cache, or if user requested a cache-only treatment
	let should_update_db = should_update_cache || options == ModRefreshOptions::CacheOnly;

	if should_update_db {
		let mods = load_mods_from_cache()?;
		save_mods_to_db(db, &mods)?;
	}

	Ok(())
}

fn load_mods_json() -> Result<String, Box<dyn Error>> {
	assert!(!cfg!(test), "Trying to load mod cache in tests");

	let mut buffer = Vec::new();
	let mut easy = Easy::new();
	easy.url(THUNDERSTORE_API_URL)?;
	easy.progress(true)?;

	let log_frequency = Duration::from_millis(1000);

	{
		let mut last_log = Instant::now();

		let mut transfer = easy.transfer();
		transfer.write_function(|data| {
			buffer.extend_from_slice(data);
			Ok(data.len())
		})?;

		transfer.progress_function(|total_expected, downloaded, _total_upload, _uploaded| {
			if last_log.elapsed() < log_frequency {
				return true;
			}

			last_log = Instant::now();

			let percent = if total_expected != 0.0 {
				downloaded * 100.0 / total_expected
			} else {
				0.0
			};
			println!("{downloaded} / {total_expected} ({percent}%) downloaded");

			true
		})?;

		transfer.perform()?;
	}

	let result = String::from_utf8(buffer)?;
	Ok(result)
}

fn save_mods_to_cache(mods_json: &String) -> Result<(), Box<dyn Error>> {
	assert!(!cfg!(test), "Trying to save mods to cache in tests");

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
	last_update: Option<UtcDateTime>,
	now: UtcDateTime,
	expiration_duration: Duration,
) -> bool {
	if let Some(last_update) = last_update {
		let time_passed = now - last_update;
		time_passed > expiration_duration
	} else {
		// no previous value present -> this is first time
		true
	}
}

#[allow(dead_code)]
#[derive(PartialEq, Eq)]
pub enum ModRefreshOptions {
	ForceDownload,
	CacheOnly,
	DownloadIfExpired(Duration),
}

impl Default for ModRefreshOptions {
	fn default() -> Self {
		let days = 1;
		let secs_in_day = 24 * 60 * 60;
		let secs = days * secs_in_day;
		Self::DownloadIfExpired(Duration::from_secs(secs))
	}
}

fn save_mods_to_db(db: &Database, mods: &Vec<ModRaw>) -> Result<(), Box<dyn Error>> {
	let category_names = mods
		.iter()
		.map(|modd| modd.categories.iter())
		.flatten()
		.collect::<HashSet<_>>();

	db.insert_categories(&category_names)?;

	let categories = db
		.get_categories()?
		.into_iter()
		.map(|ct| (ct.name.clone(), ct))
		.collect::<HashMap<String, Category>>();

	let mods = mods.iter().map(|m| m.to_insertable(&categories)).collect();
	db.insert_mods(&mods)?;
	Ok(())
}
