use std::{sync::Mutex, time::Duration};

use actix_files::NamedFile;
use actix_web::{
	App, Either, HttpResponse, HttpServer, Responder, get,
	http::{Method, StatusCode},
	middleware, post,
	web::{self, Data, Form, Html},
};
use db::{Database, ModQueryOptions};
use env::Env;
use mods::{Rating, refresh_mods};
use serde::Deserialize;
use tera::{Context, Tera};

mod db;
mod env;
mod mods;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let env = Env::load();
	env_logger::builder().filter_level(env.log_level).init();

	let db = Database::open_connection().unwrap();
	refresh_mods(&db, mods::ModRefreshOptions::default(), &env).unwrap();
	//refresh_mods(&db, mods::ModRefreshOptions::CacheOnly).unwrap();

	let data = Data::new(Mutex::new(db));

	let tera = Data::new(Mutex::new(Tera::new("templates/*.html").unwrap()));

	#[cfg(debug_assertions)]
	let _debouncer = {
		log::info!("Tera hot reloading enabled");

		let tera_clone = tera.clone();

		tera_hot_reload::watch(
			move || {
				log::info!("Reloading Tera templates");
				tera_clone.lock().unwrap().full_reload().unwrap();
			},
			Duration::from_millis(500),
			vec!["./templates"],
		)
	};

	let port = env.port;
	log::info!("Starting server on port {port}");

	HttpServer::new(move || {
		App::new()
			.wrap(middleware::Logger::default())
			.app_data(data.clone())
			.app_data(tera.clone())
			.service(favicon)
			.service(welcome_page)
			.service(get_rating_page)
			.service(post_rating)
			.service(rated_mods)
			.default_service(web::to(default_handler))
	})
	.bind(("0.0.0.0", port))?
	.run()
	.await
}

#[get("/")]
async fn welcome_page(template: Data<Mutex<Tera>>) -> Result<Html, actix_web::Error> {
	let ctx = Context::new();
	let html = template
		.lock()
		.unwrap()
		.render("index.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Html::new(html))
}

#[get("/rate")]
async fn get_rating_page(
	template: Data<Mutex<Tera>>,
	db: Data<Mutex<Database>>,
) -> Result<Html, actix_web::Error> {
	rating_page(template, db)
}

#[post("/rate")]
async fn post_rating(
	params: Form<RatingForm>,
	template: Data<Mutex<Tera>>,
	db: Data<Mutex<Database>>,
) -> Result<Html, actix_web::Error> {
	db.lock()
		.unwrap()
		.insert_mod_rating(&params.mod_id, &params.rating)?;

	rating_page(template, db)
}

#[get("/likes")]
async fn rated_mods(
	template: Data<Mutex<Tera>>,
	db: Data<Mutex<Database>>,
) -> Result<Html, actix_web::Error> {
	let mods = db
		.lock()
		.unwrap()
		.get_rated_mods(&Rating::Like, 100)
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

#[get("/favicon.ico")]
async fn favicon() -> actix_web::Result<impl Responder> {
	Ok(NamedFile::open("static/favicon.ico")?)
}

async fn default_handler(req_method: Method) -> actix_web::Result<impl Responder> {
	match req_method {
		Method::GET => {
			let file = NamedFile::open("static/404.html")?
				.customize()
				.with_status(StatusCode::NOT_FOUND);
			Ok(Either::Left(file))
		}
		_ => Ok(Either::Right(HttpResponse::MethodNotAllowed().finish())),
	}
}

fn rating_page(
	template: Data<Mutex<Tera>>,
	db: Data<Mutex<Database>>,
) -> Result<Html, actix_web::Error> {
	let mut ctx = Context::new();

	let options = ModQueryOptions {
		ignored_categories: Default::default(),
		limit: 1,
		include_deprecated: false,
		include_nsfw: false,
	};

	let mods = db
		.lock()
		.unwrap()
		.get_mods(&options)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

	let modd = mods
		.first()
		.ok_or_else(|| actix_web::error::ErrorInternalServerError("No mods found"))?;

	ctx.insert("name", &modd.name);
	ctx.insert("owner", &modd.owner);
	ctx.insert("icon_url", &modd.icon);
	ctx.insert("description", &modd.description);
	ctx.insert("package_url", &modd.package_url);
	ctx.insert("mod_id", &modd.id);

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
