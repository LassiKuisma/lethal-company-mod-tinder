use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{
	Either, Responder, get, post,
	web::{Data, Html},
};
use tera::{Context, Tera};
use time::{OffsetDateTime, macros::format_description};

use crate::{
	db::Database,
	middlewares::{PrivilegeValidator, TokenValidator},
};

#[derive(Debug, Default, Clone)]
pub struct ImportStatus {
	pub import_requested: bool,
	pub import_in_progress: bool,
}

#[get("/import-mods", wrap = "PrivilegeValidator", wrap = "TokenValidator")]
pub async fn import_mods_page(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
	import_status: Data<Mutex<ImportStatus>>,
) -> Result<impl Responder, actix_web::Error> {
	let import_in_progress = {
		let import_status = import_status.lock().unwrap();
		import_status.import_requested || import_status.import_in_progress
	};

	if import_in_progress {
		return Ok(Either::Left(NamedFile::open(
			"static/import_in_progress.html",
		)));
	}

	let mut ctx = Context::new();

	let latest_import = db
		.latest_mod_import_date()
		.await
		.map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

	ctx.insert("latest_import", &latest_import_string(latest_import));

	let html = template
		.lock()
		.unwrap()
		.render("import_mods.html", &ctx)
		.map_err(|err| {
			log::error!("{err}");
			actix_web::error::ErrorInternalServerError("Template error")
		})?;

	Ok(Either::Right(Html::new(html)))
}

#[post("/import-mods", wrap = "PrivilegeValidator", wrap = "TokenValidator")]
pub async fn import_mods(import_status: Data<Mutex<ImportStatus>>) -> impl Responder {
	log::info!("Mod reimport requested");
	import_status.lock().unwrap().import_requested = true;
	NamedFile::open("static/import_in_progress.html")
}

fn latest_import_string(latest_import: Option<OffsetDateTime>) -> String {
	let date = match latest_import {
		None => return "Never".to_string(),
		Some(date) => date,
	};

	let date_str = date.format(format_description!(
		"[year]-[month]-[day] [hour]:[minute]UTC"
	));

	let date_str = match date_str {
		Ok(str) => str,
		Err(err) => {
			log::error!("Error formatting date: {err}");
			return "---".to_string();
		}
	};

	let now = OffsetDateTime::now_utc();
	let elapsed = now - date;

	let minutes = elapsed.whole_minutes();
	let hours = elapsed.whole_hours();
	let days = elapsed.whole_days();

	let time_since = if days > 0 {
		format!("{days} days ago")
	} else if hours > 0 {
		format!("{hours} hours ago")
	} else if minutes >= 5 {
		format!("{minutes} minutes ago")
	} else {
		"just now".to_string()
	};

	format!("{date_str} ({time_since})")
}
