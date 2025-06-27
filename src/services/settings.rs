use std::{collections::HashSet, sync::Mutex};

use actix_web::{
	HttpRequest, HttpResponse, Responder,
	cookie::Cookie,
	get, post,
	web::{Data, Html},
};
use serde::{Deserialize, Serialize};
use serde_qs::actix::QsForm;
use tera::{Context, Tera};

use crate::{
	db::Database, middlewares::TokenValidator, mods::Category, services::header_redirect_to,
};

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

#[derive(Debug, Serialize)]
struct CategoryCheckbox {
	name: String,
	id: i32,
	checked: bool,
}

impl CategoryCheckbox {
	fn new(category: Category, checked: bool) -> Self {
		Self {
			name: category.name,
			id: category.id,
			checked,
		}
	}
}

#[get("/settings", wrap = "TokenValidator")]
pub async fn settings_page(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
	request: HttpRequest,
) -> Result<impl Responder, actix_web::Error> {
	let settings = request
		.cookie(SETTINGS_COOKIE)
		.map(|cookie| serde_json::from_str::<Settings>(cookie.value()).ok())
		.flatten()
		.unwrap_or_default();

	let mut ctx = Context::new();

	let categories = db
		.get_categories()
		.await?
		.into_iter()
		.map(|c| {
			let checked = settings.excluded_category.contains(&c.name);
			CategoryCheckbox::new(c, checked)
		})
		.collect::<Vec<_>>();

	ctx.insert("categories", &categories);
	ctx.insert("nsfw_checked", &settings.include_nsfw);
	ctx.insert("deprecated_checked", &settings.include_deprecated);

	let html = template
		.lock()
		.unwrap()
		.render("settings.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Html::new(html))
}

#[post("/save-settings", wrap = "TokenValidator")]
pub async fn save_settings(settings: QsForm<Settings>) -> Result<impl Responder, actix_web::Error> {
	let settings_json = serde_json::to_string(&settings.into_inner())
		.map_err(|_| actix_web::error::ErrorInternalServerError("Unknown error"))?;

	let cookie = Cookie::build(SETTINGS_COOKIE, settings_json)
		.permanent()
		.finish();

	let response = HttpResponse::Ok()
		.insert_header(header_redirect_to("/"))
		.cookie(cookie)
		.finish();

	Ok(response)
}
