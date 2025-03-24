use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{
	App, Either, HttpResponse, HttpServer, Responder, get,
	http::{Method, StatusCode},
	web::{self, Data, Html},
};
use db::Database;
use mods::Mod;
use tera::{Context, Tera};

mod db;
mod mods;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let db = Database::open_connection().unwrap();
	let data = Data::new(Mutex::new(db));

	let port = 3000;
	println!("Starting server on port {port}");

	HttpServer::new(move || {
		let tera = Data::new(Tera::new("templates/*.html").unwrap());

		App::new()
			.app_data(data.clone())
			.app_data(tera)
			.service(favicon)
			.service(welcome_page)
			.service(rating_page)
			.default_service(web::to(default_handler))
	})
	.bind(("127.0.0.1", port))?
	.run()
	.await
}

#[get("/")]
async fn welcome_page(template: Data<Tera>) -> Result<Html, actix_web::Error> {
	let ctx = Context::new();
	let html = template
		.render("index.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Html::new(html))
}

#[get("/rate")]
async fn rating_page(template: Data<Tera>) -> Result<Html, actix_web::Error> {
	let mut ctx = Context::new();

	let modd = Mod {
		name: "Foobar mod".to_string(),
		owner: "BarBaz".to_string(),
		description: "This is a mod description".to_string(),
		icon: "https://gcdn.thunderstore.io/live/repository/icons/ebkr-r2modman-3.1.57.png"
			.to_string(),
		package_url: "https://thunderstore.io/c/lethal-company/p/ebkr/r2modman/".to_string(),
	};

	ctx.insert("name", &modd.name);
	ctx.insert("owner", &modd.owner);
	ctx.insert("icon_url", &modd.icon);
	ctx.insert("description", &modd.description);
	ctx.insert("package_url", &modd.package_url);

	let html = template
		.render("rating.html", &ctx)
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
