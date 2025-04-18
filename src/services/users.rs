use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{
	CustomizeResponder, Either, HttpMessage, HttpResponse, Responder,
	body::MessageBody,
	cookie::Cookie,
	dev::{ServiceRequest, ServiceResponse},
	get,
	http::{StatusCode, header},
	middleware::Next,
	post,
	web::{Data, Form},
};
use argon2::{
	Argon2,
	password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use hmac::{Hmac, Mac};
use jwt::{SignWithKey, VerifyWithKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::prelude::FromRow;
use tera::Tera;

use crate::{db::Database, services::home_page};

#[derive(FromRow)]
pub struct User {
	pub id: i32,
	pub username: String,
	pub password_hash: String,
}

#[derive(Debug)]
pub struct UserNoId {
	pub username: String,
	pub password_hash: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TokenClaims {
	pub id: i32,
}

pub async fn validator(
	req: ServiceRequest,
	next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
	let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET is not set");
	let key: Hmac<Sha256> = Hmac::new_from_slice(jwt_secret.as_bytes()).unwrap();

	if let Some(cookie) = req.cookie("lcmt-login") {
		let token_string = cookie.value();
		let claims: Result<TokenClaims, _> = token_string.verify_with_key(&key);

		match claims {
			Ok(value) => {
				req.extensions_mut().insert(value);
			}
			Err(_) => {
				return Err(actix_web::error::ErrorUnauthorized(
					"Incorrect username or password",
				));
			}
		}
	}

	next.call(req).await
}

#[derive(Deserialize)]
struct CreateUserBody {
	username: String,
	password: String,
}

#[post("/create-user")]
async fn create_user(db: Data<Database>, body: Form<CreateUserBody>) -> impl Responder {
	let user = body.into_inner();

	let argon2 = Argon2::default();
	let salt = SaltString::generate(&mut OsRng);
	let password_hash = argon2
		.hash_password(user.password.as_bytes(), &salt)
		.unwrap()
		.to_string();

	let user = UserNoId {
		username: user.username,
		password_hash,
	};

	match db.insert_user(&user).await {
		// TODO: return jwt token? (log in when creating account)
		Ok(true) => HttpResponse::Ok().finish(),
		Ok(false) => HttpResponse::Conflict().json("That username is already taken"),
		Err(_) => HttpResponse::InternalServerError().json("Database error"),
	}
}

#[derive(Deserialize)]
struct LoginCredentials {
	username: String,
	password: String,
}

#[post("/auth")]
async fn basic_auth(
	db: Data<Database>,
	body: Form<LoginCredentials>,
) -> Result<Either<HttpResponse, CustomizeResponder<NamedFile>>, actix_web::Error> {
	let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET is not set");
	let key: Hmac<Sha256> = Hmac::new_from_slice(jwt_secret.as_bytes()).unwrap();

	let user = match db.find_user(&body.username).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			return Ok(Either::Right(
				NamedFile::open("static/login_failed.html")?
					.customize()
					.with_status(StatusCode::UNAUTHORIZED),
			));
		}
		Err(_) => {
			return Ok(Either::Right(
				NamedFile::open("static/error.html")?
					.customize()
					.with_status(StatusCode::INTERNAL_SERVER_ERROR),
			));
		}
	};

	let hash = PasswordHash::new(&user.password_hash).unwrap();
	let argon2 = Argon2::default();
	let is_valid = argon2
		.verify_password(body.password.as_bytes(), &hash)
		.is_ok();

	if is_valid {
		let claims = TokenClaims { id: user.id };
		let token_str = claims.sign_with_key(&key).unwrap();
		let cookie = Cookie::build("lcmt-login", &token_str)
			.secure(true)
			.http_only(true)
			.finish();

		Ok(Either::Left(
			HttpResponse::Ok()
				.cookie(cookie)
				.append_header((header::REFRESH, "0; url=/"))
				.finish(),
		))
	} else {
		Ok(Either::Right(
			NamedFile::open("static/login_failed.html")?
				.customize()
				.with_status(StatusCode::UNAUTHORIZED),
		))
	}
}

#[get("/login")]
async fn login_page() -> Result<impl Responder, actix_web::Error> {
	Ok(NamedFile::open("static/login.html")?
		.customize()
		.with_status(StatusCode::OK))
}

#[get("/create-user")]
async fn create_user_page() -> Result<impl Responder, actix_web::Error> {
	Ok(NamedFile::open("static/create_user.html")?
		.customize()
		.with_status(StatusCode::OK))
}

#[get("/logout")]
async fn logout(
	template: Data<Mutex<Tera>>,
	db: Data<Database>,
) -> Result<impl Responder, actix_web::Error> {
	// client sent login-data-cookie with this request, don't pass that to render-home-page function
	// as we will erase that cookie with this response.
	// TODO: should we show some kind of "logged out - click here to go back to main menu"
	let html = home_page(template, db, None).await?;

	let mut clear_login = Cookie::new("lcmt-login", "");
	clear_login.make_removal();

	let response = html.customize().add_cookie(&clear_login);
	Ok(response)
}
