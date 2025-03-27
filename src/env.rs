use std::{collections::HashMap, env, str::FromStr};

use log::LevelFilter;

pub struct Env {
	pub port: u16,
	pub log_level: LevelFilter,
	pub sql_chunk_size: usize,
}

impl Env {
	pub fn load() -> Self {
		dotenvy::dotenv().expect("Can't find .env file");
		let vars = env::vars().collect::<HashMap<_, _>>();

		let port_str = vars.get("PORT").expect("Missing .env variable: PORT");
		let port = port_str
			.parse()
			.expect(&format!("Can't convert PORT to number: '{port_str}'"));

		let log_level = vars
			.get("LOG_LEVEL")
			.expect("Missing .env variable: LOG_LEVEL");

		let sql_chunk_size_str = vars
			.get("SQL_CHUNK_SIZE")
			.expect("Missing .env variable: SQL_CHUNK_SIZE");

		let sql_chunk_size = sql_chunk_size_str.parse().expect(&format!(
			"Can't convert SQL_CHUNK_SIZE to number: '{sql_chunk_size_str}'"
		));

		if sql_chunk_size == 0 {
			panic!("SQL_CHUNK_SIZE can't be zero");
		}

		Self {
			port,
			log_level: get_log_level(log_level),
			sql_chunk_size,
		}
	}
}

fn get_log_level(str: &str) -> LevelFilter {
	if let Some(log_level) = LevelFilter::from_str(str).ok() {
		return log_level;
	}

	let allowed_values = LevelFilter::iter()
		.map(|lf| lf.to_string())
		.collect::<Vec<_>>()
		.join(", ");

	panic!(
		"Not a valid log level: '{}'. Allowed values are: {}",
		str, allowed_values
	);
}
