use actix_web::{
	HttpMessage, HttpResponse, Responder,
	dev::ServiceRequest,
	get, post,
	web::{Data, Json},
};
use actix_web_httpauth::extractors::{
	AuthenticationError,
	basic::BasicAuth,
	bearer::{self, BearerAuth},
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
	id: i32,
}

pub async fn validator(
	req: ServiceRequest,
	credentials: BearerAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
	let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET is not set");
	let key: Hmac<Sha256> = Hmac::new_from_slice(jwt_secret.as_bytes()).unwrap();
	let token_string = credentials.token();

	let claims: Result<TokenClaims, _> = token_string.verify_with_key(&key);

	match claims {
		Ok(value) => {
			req.extensions_mut().insert(value);
			Ok(req)
		}
		Err(_) => {
			let config = req
				.app_data::<bearer::Config>()
				.cloned()
				.unwrap_or_default()
				.scope("");

			Err((AuthenticationError::from(config).into(), req))
		}
	}
}

#[derive(Deserialize)]
struct CreateUserBody {
	username: String,
	password: String,
}

#[post("/user")]
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

#[derive(Serialize, FromRow)]
struct AuthUser {
	id: i32,
	username: String,
	password: String,
}

#[get("/auth")]
async fn basic_auth(db: Data<Database>, credentials: BasicAuth) -> impl Responder {
	let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET is not set");
	let key: Hmac<Sha256> = Hmac::new_from_slice(jwt_secret.as_bytes()).unwrap();

	let username = credentials.user_id();
	let password = match credentials.password() {
		Some(password) => password,
		None => return HttpResponse::Unauthorized().json("Must provide username and password"),
	};

	let user = match db.find_user(username).await {
		Ok(Some(user)) => user,
		Ok(None) => return HttpResponse::Unauthorized().json("Incorrect username or password"),
		Err(_) => return HttpResponse::InternalServerError().finish(),
	};

	let hash = PasswordHash::new(&user.password_hash).unwrap();
	let argon2 = Argon2::default();
	let is_valid = argon2.verify_password(password.as_bytes(), &hash).is_ok();

	if is_valid {
		let claims = TokenClaims { id: user.id };
		let token_str = claims.sign_with_key(&key).unwrap();
		HttpResponse::Ok().json(token_str)
	} else {
		HttpResponse::Unauthorized().json("Incorrect username or password")
	}
}
