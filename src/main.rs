use std::{sync::Mutex, time::Duration};

use actix_web::{
	App, HttpServer, middleware,
	web::{self, Data},
};
use db::Database;
use env::Env;
use mods::refresh_mods;
use services::{
	default_handler, favicon, get_home_page,
	ratings::{get_rating_page, post_rating, rated_mods},
	users::{basic_auth, create_user, create_user_page, login_page, logout, validator},
};
use tera::Tera;

mod db;
mod env;
mod mods;
mod services;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let env = Env::load();
	env_logger::builder().filter_level(env.log_level).init();

	let db = Database::open_connection(&env.db_url, 5).await.unwrap();
	refresh_mods(&db, &env).await.unwrap();

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
		let validator_middleware = middleware::from_fn(validator);

		App::new()
			.wrap(middleware::Logger::default())
			.app_data(Data::new(db.clone()))
			.app_data(tera.clone())
			.service(favicon)
			.service(get_home_page)
			.service(create_user)
			.service(create_user_page)
			.service(basic_auth)
			.service(login_page)
			.service(logout)
			.service(
				web::scope("")
					.wrap(validator_middleware)
					.service(get_rating_page)
					.service(post_rating)
					.service(rated_mods),
			)
			.default_service(web::to(default_handler))
	})
	.bind(("0.0.0.0", port))?
	.run()
	.await
}
