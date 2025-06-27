use std::sync::Mutex;

use actix_files::NamedFile;
use actix_web::{
	HttpMessage, Responder,
	body::{BoxBody, MessageBody},
	dev::{ServiceRequest, ServiceResponse},
	middleware::Next,
	web::Data,
};

use crate::{db::Database, services::users::TokenClaims};

#[derive(Debug, Default, Clone)]
pub struct ImportStatus {
	pub import_requested: bool,
	pub import_in_progress: bool,
}

pub async fn privilege_validator(
	req: ServiceRequest,
	next: Next<BoxBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
	let db = req.app_data::<Data<Database>>().ok_or_else(|| {
		actix_web::error::ErrorInternalServerError("Server error (can't find db)")
	})?;

	let user = {
		let ext = req.extensions();
		let token_claims = ext.get::<TokenClaims>().ok_or_else(|| {
			actix_web::error::ErrorInternalServerError("Server error (can't find token)")
		})?;

		let user = db
			.find_user_by_id(token_claims.id)
			.await?
			.ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;
		user
	};

	// TODO:
	if user.username != "admin" {
		let err = actix_web::error::ErrorUnauthorized("You don't have permission to use this");
		return Err(err);
	}

	next.call(req).await
}

pub async fn import_mods_page(import_status: Data<Mutex<ImportStatus>>) -> impl Responder {
	let import_status = import_status.lock().unwrap();
	let already_requested = import_status.import_requested;
	let in_progress = import_status.import_in_progress;

	if already_requested || in_progress {
		return NamedFile::open("static/import_in_progress.html");
	}

	NamedFile::open("static/import_mods.html")
}

pub async fn import_mods(import_status: Data<Mutex<ImportStatus>>) -> impl Responder {
	log::info!("Mod reimport requested");
	import_status.lock().unwrap().import_requested = true;
	NamedFile::open("static/import_in_progress.html")
}
