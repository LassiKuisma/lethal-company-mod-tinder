use std::{collections::HashSet, sync::Mutex};

use actix_web::{
	HttpResponse, Responder,
	cookie::Cookie,
	get, post,
	web::{Data, Html},
};
use serde::{Deserialize, Serialize};
use serde_qs::actix::QsForm;
use tera::{Context, Tera};

use crate::{db::Database, services::header_redirect_to};

pub const SETTINGS_COOKIE: &'static str = "lcmt-settings";

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Settings {
	#[serde(default)]
	pub excluded_category: HashSet<String>,
	#[serde(default)]
	pub include_nsfw: bool,
	#[serde(default)]
	pub include_deprecated: bool,
}

#[get("/settings")]
pub async fn settings_page(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
) -> Result<impl Responder, actix_web::Error> {
	let mut ctx = Context::new();

	let categories = db.get_categories().await?;

	ctx.insert("categories", &categories);

	let html = template
		.lock()
		.unwrap()
		.render("settings.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Html::new(html))
}

#[post("/save-settings")]
pub async fn save_settings(settings: QsForm<Settings>) -> Result<impl Responder, actix_web::Error> {
	let settings_json = serde_json::to_string(&settings.into_inner())
		.map_err(|_| actix_web::error::ErrorInternalServerError("Unknown error"))?;

	let cookie = Cookie::build(SETTINGS_COOKIE, settings_json).finish();

	let response = HttpResponse::Ok()
		.insert_header(header_redirect_to("/"))
		.cookie(cookie)
		.finish();

	Ok(response)
}
