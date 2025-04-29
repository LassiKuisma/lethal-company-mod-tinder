use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{
	Either, HttpResponse, Responder, get,
	http::{
		Method, StatusCode,
		header::{self, TryIntoHeaderPair},
	},
	web::{Data, Html, ReqData},
};
use tera::{Context, Tera};
use users::TokenClaims;

use crate::db::Database;

pub mod ratings;
pub mod users;
pub mod settings;

fn header_redirect_to(to_url: &str) -> impl TryIntoHeaderPair {
	(header::REFRESH, format!("0; url={to_url}"))
}

#[get("/")]
async fn home_page(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
	req_user: ReqData<TokenClaims>,
) -> Result<impl Responder, actix_web::Error> {
	let mut ctx = Context::new();

	match db.find_user_by_id(req_user.id).await {
		Ok(Some(user)) => ctx.insert("username", &user.username),
		Ok(None) => {
			let response = HttpResponse::BadRequest()
				.insert_header(header_redirect_to("/login-error"))
				.finish();
			return Ok(Either::Left(response));
		}
		Err(_) => return Err(actix_web::error::ErrorInternalServerError("Database error")),
	}

	let html = template
		.lock()
		.unwrap()
		.render("index.html", &ctx)
		.map_err(|_| actix_web::error::ErrorInternalServerError("Template error"))?;

	Ok(Either::Right(Html::new(html)))
}

#[get("/login-error")]
async fn login_error_page() -> Result<impl Responder, actix_web::Error> {
	Ok(NamedFile::open("static/login_error.html")?
		.customize()
		.with_status(StatusCode::INTERNAL_SERVER_ERROR))
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
