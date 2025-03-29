use std::{collections::HashMap, env, str::FromStr, time::Duration};

use log::LevelFilter;

use crate::mods::ModRefreshOptions;

pub struct Env {
	pub port: u16,
	pub log_level: LevelFilter,
	pub sql_chunk_size: usize,
	pub mod_refresh_options: ModRefreshOptions,
	pub db_url: String,
}

impl Env {
	pub fn load() -> Self {
		dotenvy::dotenv().expect("Can't find .env file");
		let vars = env::vars().collect::<HashMap<_, _>>();

		Self {
			port: port(&vars),
			log_level: log_level(&vars),
			sql_chunk_size: chunk_size(&vars),
			mod_refresh_options: mod_refresh_options(&vars),
			db_url: db_url(&vars),
		}
	}
}

#[cfg(test)]
impl Env {
	pub fn get_test_db_url() -> String {
		// test db url might be given as an environment variable when running tests (for example in CI pipeline)
		// -> ignore file-not-found if .env doesn't exist
		let _ = dotenvy::dotenv();

		env::vars()
			.find_map(|(key, value)| key.eq("TEST_DB_URL").then_some(value))
			.expect("Missing .env variable: TEST_DB_URL")
	}
}

fn port(vars: &HashMap<String, String>) -> u16 {
	let port_str = vars.get("PORT").expect("Missing .env variable: PORT");

	port_str
		.parse()
		.expect(&format!("Can't convert PORT to number: '{port_str}'"))
}

fn log_level(vars: &HashMap<String, String>) -> LevelFilter {
	let log_level = vars
		.get("LOG_LEVEL")
		.expect("Missing .env variable: LOG_LEVEL");

	if let Some(log_level) = LevelFilter::from_str(&log_level).ok() {
		return log_level;
	}

	let allowed_values = LevelFilter::iter()
		.map(|lf| lf.to_string())
		.collect::<Vec<_>>()
		.join(", ");

	panic!(
		"Not a valid log level: '{}'. Allowed values are: {}",
		log_level, allowed_values
	);
}

fn chunk_size(vars: &HashMap<String, String>) -> usize {
	let str = vars
		.get("SQL_CHUNK_SIZE")
		.expect("Missing .env variable: SQL_CHUNK_SIZE");

	let sql_chunk_size = str
		.parse()
		.expect(&format!("Can't convert SQL_CHUNK_SIZE to number: '{str}'"));

	if sql_chunk_size == 0 {
		panic!("SQL_CHUNK_SIZE can't be zero");
	}

	sql_chunk_size
}

fn mod_refresh_options(vars: &HashMap<String, String>) -> ModRefreshOptions {
	let str = vars
		.get("MOD_REFRESH")
		.expect("Missing .env variable: MOD_REFRESH")
		.as_str();

	match str {
		"always-download" => ModRefreshOptions::ForceDownload,
		"cache-only" => ModRefreshOptions::CacheOnly,
		"none" => ModRefreshOptions::NoRefresh,
		"download-if-expired" => {
			let expiration_time = vars
				.get("MOD_EXPIRATION_TIME_HOURS")
				.expect("Missing .env variable: MOD_EXPIRATION_TIME_HOURS");
			let expiration_time = expiration_time.parse::<u64>().expect(&format!(
				"MOD_EXPIRATION_TIME_HOURS is not a valid number: '{expiration_time}'"
			));

			ModRefreshOptions::DownloadIfExpired(Duration::from_secs(expiration_time * 60 * 60))
		}
		_ => panic!(
			"Not a valid mod refresh option: '{str}'. Allowed values are: always-download, download-if-expired, cache-only, none"
		),
	}
}

fn db_url(vars: &HashMap<String, String>) -> String {
	vars.get("DB_URL")
		.expect("Missing .env variable: DB_URL")
		.clone()
}
