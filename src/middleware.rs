use crate::{jwt, Service};
use actix_web::dev::{ServiceRequest, ServiceResponse, Transform};
use futures::future::Either;
use std::{
    future::{ready, Ready},
    sync::Arc,
};
use actix_web::HttpMessage;

pub struct RequireToken(pub Arc<String>);

pub struct JwtDecoder(pub Arc<jwt::DecodeConfig>);

pub struct RequireTokenMiddleware<S> {
    service: S,
    token: Arc<String>,
}

pub struct JwtDecoderMiddleware<S> {
    service: S,
    config: Arc<jwt::DecodeConfig>,
}

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
#[status(401)]
pub enum RequireTokenError {
    #[error("Missing token")]
    NoToken,
    #[error("Bad token")]
    BadToken,
}

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
#[status(401)]
pub enum JwtDecodeError {
    #[error("Missing token")]
    NoToken,
    #[error("Bad token")]
    BadToken,
}

impl<S, B> Transform<S, ServiceRequest> for RequireToken
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = RequireTokenMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(Self::Transform {
            service,
            token: self.0.clone(),
        }))
    }
}

impl<S, B> Service<ServiceRequest> for RequireTokenMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let header = req.headers().get("x-frachter-token");
        let header = match header {
            Some(h) => h,
            None => return Either::Left(ready(Err(RequireTokenError::NoToken.into()))),
        };
        if header.as_bytes() == self.token.as_bytes() {
            Either::Right(self.service.call(req))
        } else {
            Either::Left(ready(Err(RequireTokenError::BadToken.into())))
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for JwtDecoder
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = JwtDecoderMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(JwtDecoderMiddleware {
            service,
            config: self.0.clone(),
        }))
    }
}

impl<S, B> Service<ServiceRequest> for JwtDecoderMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let cookie = req.cookie("frachter-transfer");
        let cookie = match cookie {
            Some(c) => c,
            None => return Either::Left(ready(Err(JwtDecodeError::NoToken.into()))),
        };
        let claims = match jwt::decode_token(&self.config, cookie.value()) {
            Ok(c) => c,
            Err(_) => return Either::Left(ready(Err(JwtDecodeError::BadToken.into()))),
        };
        req.request().extensions_mut().insert(claims);

        Either::Right(self.service.call(req))
    }
}
