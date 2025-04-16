use actix_files::NamedFile;
use actix_web::{
	HttpMessage, HttpResponse, Responder,
	body::MessageBody,
	cookie::Cookie,
	dev::{ServiceRequest, ServiceResponse},
	get,
	http::StatusCode,
	middleware::Next,
	post,
	web::{Data, Form, Json},
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

use crate::db::Database;

#[derive(FromRow)]
pub struct User {
	pub id: i32,
	pub username: String,
	pub password_hash: String,
}

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
	} else {
		log::info!("login-cookie not found");
	}

	next.call(req).await
}

#[derive(Deserialize)]
struct CreateUserBody {
	username: String,
	password: String,
}

#[post("/create-user")]
async fn create_user(db: Data<Database>, body: Json<CreateUserBody>) -> impl Responder {
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
		Ok(_) => HttpResponse::Ok(),
		// TODO: return "invalid request" if username is already taken
		Err(_) => HttpResponse::InternalServerError(),
	}
}

#[derive(Deserialize)]
struct LoginCredentials {
	username: String,
	password: String,
}

#[post("/auth")]
async fn basic_auth(db: Data<Database>, body: Form<LoginCredentials>) -> impl Responder {
	let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET is not set");
	let key: Hmac<Sha256> = Hmac::new_from_slice(jwt_secret.as_bytes()).unwrap();

	let user = match db.find_user(&body.username).await {
		Ok(Some(user)) => user,
		Ok(None) => return HttpResponse::Unauthorized().json("Incorrect username or password"),
		Err(_) => return HttpResponse::InternalServerError().finish(),
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

		HttpResponse::Ok().cookie(cookie).json("Login successful")
	} else {
		HttpResponse::Unauthorized().json("Incorrect username or password")
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
