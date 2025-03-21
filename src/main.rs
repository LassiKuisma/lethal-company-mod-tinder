use std::collections::HashSet;

use db::{Database, ModQueryOptions};

mod db;
mod mods;

fn main() {
	let db = Database::open_connection().unwrap();
	//refresh_mods(&db, mods::ModRefreshOptions::CacheOnly).unwrap();
	let ignored_categories = vec!["Suits"].into_iter().map(|s| s.to_string()).collect();
	let mods = db
		.get_mods(&ModQueryOptions {
			ignored_categories,
			limit: 20,
		})
		.unwrap();

	for modd in mods {
		println!("{} - {}", modd.name, modd.description);
	}
}
