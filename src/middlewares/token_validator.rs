use std::{
	cell::RefCell,
	future::{Ready, ready},
	pin::Pin,
	rc::Rc,
};

use actix_web::{
	HttpMessage, HttpResponse,
	body::{EitherBody, MessageBody},
	dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use hmac::{Hmac, Mac};
use jwt::VerifyWithKey;
use sha2::Sha256;

use crate::services::{header_redirect_to, users::TokenClaims};

pub struct TokenValidator;
impl<S, B> Transform<S, ServiceRequest> for TokenValidator
where
	S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
	B: MessageBody,
{
	type Response = ServiceResponse<EitherBody<B>>;
	type Error = actix_web::Error;
	type InitError = ();
	type Transform = TokenValidatorMiddleware<S>;
	type Future = Ready<Result<Self::Transform, Self::InitError>>;

	fn new_transform(&self, service: S) -> Self::Future {
		ready(Ok(TokenValidatorMiddleware {
			service: Rc::new(RefCell::new(service)),
		}))
	}
}

pub struct TokenValidatorMiddleware<S> {
	service: Rc<RefCell<S>>,
}

impl<S, B> Service<ServiceRequest> for TokenValidatorMiddleware<S>
where
	S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
	B: MessageBody,
{
	type Response = ServiceResponse<EitherBody<B>>;
	type Error = S::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

	forward_ready!(service);

	fn call(&self, req: ServiceRequest) -> Self::Future {
		let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET is not set");
		let key: Hmac<Sha256> = Hmac::new_from_slice(jwt_secret.as_bytes()).unwrap();

		let token_claims = req
			.cookie("lcmt-login")
			.map(|cookie| {
				let token_string = cookie.value();
				let claims: Result<TokenClaims, _> = token_string.verify_with_key(&key);

				claims.ok()
			})
			.flatten();

		match token_claims {
			Some(value) => {
				req.extensions_mut().insert(value);
			}
			// token is either invalid or missing
			None => {
				let response = HttpResponse::Ok()
					.insert_header(header_redirect_to("/login"))
					.finish();

				return Box::pin(async { Ok(req.into_response(response).map_into_right_body()) });
			}
		}

		let fut = self.service.call(req);
		Box::pin(async move {
			let res = fut.await?;
			Ok(res.map_into_left_body())
		})
	}
}
