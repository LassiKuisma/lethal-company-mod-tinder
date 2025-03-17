use db::Database;
use mods::refresh_mods;

mod db;
mod mods;

fn main() {
	let db = Database::open_connection().unwrap();
	refresh_mods(&db, mods::ModRefreshOptions::CacheOnly).unwrap();
}
