use std::{
	error::Error,
	path::Path,
	time::{Duration, Instant},
};

use curl::easy::Easy;
use serde::Deserialize;

pub type Mods = Vec<Mod>;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Mod {
	pub name: String,
	pub full_name: String,
	pub owner: String,
	pub package_url: String,
	pub donation_link: Option<String>,
	pub date_created: String,
	pub date_updated: String,
	pub uuid4: String,
	pub rating_score: i64,
	pub is_pinned: bool,
	pub is_deprecated: bool,
	pub has_nsfw_content: bool,
	pub categories: Vec<String>,
	pub versions: Vec<ModVersion>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ModVersion {
	pub name: String,
	pub full_name: String,
	pub description: String,
	pub icon: String,
	pub version_number: String,
	pub dependencies: Vec<String>,
	pub download_url: String,
	pub downloads: i64,
	pub date_created: String,
	pub website_url: String,
	pub is_active: bool,
	pub uuid4: String,
	pub file_size: i64,
}

const CACHE_FILE: &str = "data/mods_cache.json";
const THUNDERSTORE_API_URL: &str = "https://thunderstore.io/c/lethal-company/api/v1/package/";

#[allow(dead_code)]
pub fn refresh_mods(options: ModRefreshOptions) -> Result<(), Box<dyn Error>> {
	let should_download = match options {
		ModRefreshOptions::ForceDownload => true,
		ModRefreshOptions::CacheOnly => false,
		ModRefreshOptions::DownloadIfExpired(duration) => {
			let last_update = last_update_date()?;
			let now = Instant::now();

			if let Some(last_update) = last_update {
				let time_passed = now - last_update;
				time_passed > duration
			} else {
				// no previous value present -> this is first time
				true
			}
		}
	};

	if should_download {
		println!("Downloading mods from Thunderstore...");
		let mods_json = load_thunderstore_mods()?;
		save_mods_to_cache(&mods_json)?;
		// TODO: update last update date
	}

	let mods = load_mods_from_cache()?;

	Ok(())
}

fn last_update_date() -> Result<Option<Instant>, Box<dyn Error>> {
	// TODO:
	return Ok(None);
}

fn load_thunderstore_mods() -> Result<String, Box<dyn Error>> {
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

#[allow(dead_code)]
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
