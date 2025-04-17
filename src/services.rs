use std::{io::Read, sync::Mutex};

use actix_files::NamedFile;
use actix_web::{
	Either, HttpResponse, Responder, get,
	http::{Method, StatusCode},
	web::{Data, Html},
};
use tera::{Context, Tera};

pub mod ratings;
pub mod users;

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

pub async fn not_logged_in() -> actix_web::Result<Html> {
	let mut file = NamedFile::open("static/not_logged_in.html")?;
	let mut buf = String::new();
	file.read_to_string(&mut buf)?;
	Ok(Html::new(buf))
}

#[get("/favicon.ico")]
pub async fn favicon() -> actix_web::Result<impl Responder> {
	Ok(NamedFile::open("static/favicon.ico")?)
}

pub async fn default_handler(req_method: Method) -> actix_web::Result<impl Responder> {
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
