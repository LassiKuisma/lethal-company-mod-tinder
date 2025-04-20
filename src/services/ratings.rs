use actix_web::{
	HttpResponse, Responder, get, post,
	web::{Data, Form, Html, ReqData},
};
use serde::Deserialize;
use std::sync::Mutex;
use tera::{Context, Tera};
use uuid::Uuid;

use crate::{
	db::{Database, ModQueryOptions},
	mods::Rating,
	services::header_redirect_to,
};

use super::users::TokenClaims;

#[get("/rate")]
async fn rating_page(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
	req_user: ReqData<TokenClaims>,
) -> Result<Html, actix_web::Error> {
	let mut ctx = Context::new();

	let options = ModQueryOptions {
		ignored_categories: Default::default(),
		limit: 1,
		include_deprecated: false,
		include_nsfw: false,
	};

	let mods = db
		.get_mods(&options, req_user.id)
		.await
		.map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

	let modd = mods
		.first()
		.ok_or_else(|| actix_web::error::ErrorInternalServerError("No mods found"))?;

	ctx.insert("name", &modd.name);
	ctx.insert("owner", &modd.owner);
	ctx.insert("icon_url", &modd.icon_url);
	ctx.insert("description", &modd.description);
	ctx.insert("package_url", &modd.package_url);
	ctx.insert("mod_id", &modd.id.to_string());

	let html = template
		.lock()
		.unwrap()
		.render("rating.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Html::new(html))
}

#[derive(Deserialize)]
struct RatingForm {
	mod_id: String,
	rating: Rating,
}

#[post("/rate")]
async fn post_rating(
	params: Form<RatingForm>,
	db: Data<Database>,
	req_user: ReqData<TokenClaims>,
) -> Result<impl Responder, actix_web::Error> {
	let user_id = req_user.id;

	let uuid = Uuid::parse_str(&params.mod_id)
		.map_err(|_| actix_web::error::ErrorBadRequest("Bad mod uuid"))?;
	db.insert_mod_rating(&uuid, &params.rating, user_id).await?;

	Ok(HttpResponse::Created()
		.insert_header(header_redirect_to("/rate"))
		.finish())
}

#[get("/likes")]
async fn rated_mods(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
	req_user: ReqData<TokenClaims>,
) -> Result<Html, actix_web::Error> {
	let user_id = req_user.id;

	let mods = db
		.get_rated_mods(&Rating::Like, 100, user_id)
		.await
		.map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

	let mut ctx = Context::new();
	ctx.insert("mods", &mods);

	let html = template
		.lock()
		.unwrap()
		.render("rated_mods.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Html::new(html))
}
