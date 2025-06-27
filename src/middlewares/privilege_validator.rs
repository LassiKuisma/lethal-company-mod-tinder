use std::{
	cell::RefCell,
	future::{Ready, ready},
	pin::Pin,
	rc::Rc,
};

use actix_web::{
	HttpMessage,
	body::MessageBody,
	dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
	web::Data,
};

use crate::{db::Database, services::users::TokenClaims};

pub struct PrivilegeValidator;
impl<S, B> Transform<S, ServiceRequest> for PrivilegeValidator
where
	S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
	B: MessageBody,
{
	type Response = ServiceResponse<B>;
	type Error = actix_web::Error;
	type InitError = ();
	type Transform = PrivilegeValidatorMiddleware<S>;
	type Future = Ready<Result<Self::Transform, Self::InitError>>;

	fn new_transform(&self, service: S) -> Self::Future {
		ready(Ok(PrivilegeValidatorMiddleware {
			service: Rc::new(RefCell::new(service)),
		}))
	}
}

pub struct PrivilegeValidatorMiddleware<S> {
	service: Rc<RefCell<S>>,
}

impl<S, B> Service<ServiceRequest> for PrivilegeValidatorMiddleware<S>
where
	S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
	B: MessageBody,
{
	type Response = ServiceResponse<B>;
	type Error = S::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

	forward_ready!(service);

	fn call(&self, req: ServiceRequest) -> Self::Future {
		let srv = self.service.clone();

		Box::pin(async move {
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
				let err =
					actix_web::error::ErrorUnauthorized("You don't have permission to use this");
				return Err(err);
			}

			let fut = srv.call(req);
			let res = fut.await?;
			Ok(res)
		})
	}
}
