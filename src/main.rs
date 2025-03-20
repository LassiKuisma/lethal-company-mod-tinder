use std::collections::HashSet;

use db::Database;

mod db;
mod mods;

fn main() {
	let db = Database::open_connection().unwrap();
	//refresh_mods(&db, mods::ModRefreshOptions::CacheOnly).unwrap();
	let ignored = vec!["Suits"].into_iter().map(|s| s.to_string()).collect();
	let mods = db.get_mods(ignored, 20).unwrap();

	for modd in mods {
		println!("{} - {}", modd.name, modd.description);
	}
}
