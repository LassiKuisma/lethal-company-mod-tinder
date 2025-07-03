use std::{sync::Mutex, time::Duration};

use actix_web::{
	App, HttpServer, middleware,
	web::{self, Data},
};
use db::Database;
use env::Env;
use mods::{are_mods_expired, do_import_mods, import_mods_if_expired};
use serde_qs::actix::QsQueryConfig;
use services::{
	css, default_handler, favicon, home_page,
	import_mods::{ImportStatus, import_mods, import_mods_page},
	login_error_page,
	ratings::{post_rating, rated_mods, rating_page},
	settings::{save_settings, settings_page},
	users::{basic_auth, create_user, create_user_page, login_page, logout, logout_page},
};
use tera::Tera;

mod db;
mod env;
mod middlewares;
mod mods;
mod services;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let env = Env::load();
	env_logger::builder().filter_level(env.log_level).init();

	let db = Database::open_connection(&env.db_url, 5).await.unwrap();
	import_mods_if_expired(&db, &env).await.unwrap();

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

	let import_status = Data::new(Mutex::new(ImportStatus::default()));

	let status_clone = import_status.clone();
	let db_clone = db.clone();
	let env_clone = env.clone();
	actix_rt::spawn(async move {
		import_request_checker(status_clone, db_clone, env_clone).await;
	});

	let status_clone = import_status.clone();
	let db_clone = db.clone();
	let env_clone = env.clone();
	actix_rt::spawn(async move {
		expiration_checker(status_clone, db_clone, env_clone).await;
	});

	let port = env.port;
	log::info!("Starting server on port {port}");

	HttpServer::new(move || {
		let qs_config = QsQueryConfig::default().qs_config(serde_qs::Config::new(5, false));

		App::new()
			.wrap(middleware::Logger::default())
			.app_data(Data::new(db.clone()))
			.app_data(tera.clone())
			.app_data(qs_config)
			.app_data(import_status.clone())
			.service(favicon)
			.service(create_user)
			.service(create_user_page)
			.service(basic_auth)
			.service(login_page)
			.service(login_error_page)
			.service(css)
			.service(import_mods_page)
			.service(import_mods)
			.service(logout)
			.service(logout_page)
			.service(home_page)
			.service(rating_page)
			.service(post_rating)
			.service(rated_mods)
			.service(settings_page)
			.service(save_settings)
			.default_service(web::to(default_handler))
	})
	.bind(("0.0.0.0", port))?
	.run()
	.await
}

async fn import_request_checker(import_status: Data<Mutex<ImportStatus>>, db: Database, env: Env) {
	let mut interval = actix_rt::time::interval(Duration::from_secs(10));
	loop {
		interval.tick().await;

		let do_import = {
			let status = import_status.lock().unwrap();
			status.import_requested && !status.import_in_progress
		};

		if !do_import {
			continue;
		}

		{
			let mut status = import_status.lock().unwrap();
			status.import_in_progress = true;
		}

		do_import_mods(&db, &env).await.unwrap();

		{
			let mut status = import_status.lock().unwrap();
			status.import_in_progress = false;
			status.import_requested = false;
		}
	}
}

async fn expiration_checker(import_status: Data<Mutex<ImportStatus>>, db: Database, env: Env) {
	let mut interval = actix_rt::time::interval(Duration::from_secs(60 * 60));
	loop {
		interval.tick().await;

		let already_importing = {
			let status = import_status.lock().unwrap();
			status.import_in_progress || status.import_requested
		};

		if already_importing {
			continue;
		}

		let need_to_import = are_mods_expired(&db, &env)
			.await
			.inspect_err(|error| log::error!("Failed to check mod expiration status: {error}"))
			.unwrap_or(false);

		if need_to_import {
			log::info!("Mods are expired, requesting reimport");
			import_status.lock().unwrap().import_requested = true;
		}
	}
}
