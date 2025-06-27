use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{Responder, get, post, web::Data};

use crate::middlewares::{PrivilegeValidator, TokenValidator};

#[derive(Debug, Default, Clone)]
pub struct ImportStatus {
	pub import_requested: bool,
	pub import_in_progress: bool,
}

#[get("/import-mods", wrap = "PrivilegeValidator", wrap = "TokenValidator")]
pub async fn import_mods_page(import_status: Data<Mutex<ImportStatus>>) -> impl Responder {
	let import_status = import_status.lock().unwrap();
	let already_requested = import_status.import_requested;
	let in_progress = import_status.import_in_progress;

	if already_requested || in_progress {
		return NamedFile::open("static/import_in_progress.html");
	}

	NamedFile::open("static/import_mods.html")
}

#[post("/import-mods", wrap = "PrivilegeValidator", wrap = "TokenValidator")]
pub async fn import_mods(import_status: Data<Mutex<ImportStatus>>) -> impl Responder {
	log::info!("Mod reimport requested");
	import_status.lock().unwrap().import_requested = true;
	NamedFile::open("static/import_in_progress.html")
}
