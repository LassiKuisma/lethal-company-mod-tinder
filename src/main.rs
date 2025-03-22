use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{
	App, Either, HttpResponse, HttpServer, Responder, get,
	http::{Method, StatusCode},
	web::{self, Data, Html},
};
use db::Database;
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
