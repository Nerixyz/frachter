mod bounded_body;
mod cleanup;
mod config;
mod jwt;
mod middleware;
mod mutex;
mod serde_util;
mod transfer;

use crate::{
    cleanup::{Cleanup, PutStatus, TrackTransfer},
    config::parse_config,
    jwt::{EncodeConfig, TransferClaims},
    middleware::{JwtDecoder, RequireToken},
    transfer::{ReceiverInfo, SendTransfer, Transfers},
};
use actix::{Actor, Addr};
use actix_files::Files;
use actix_web::{
    cookie::CookieBuilder,
    dev::Service,
    error::PayloadError,
    get,
    http::{
        header,
        header::{ContentDisposition, ContentType, DispositionParam, DispositionType},
    },
    post, put, web,
    web::{Payload, ReqData},
    App, HttpRequest, HttpResponse, HttpServer,
};
use futures::StreamExt;
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use std::{io, sync::Arc, time::Duration};
use tracing_actix_web::TracingLogger;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(thiserror::Error, Debug, actix_web_error::Json)]
#[error("Invalid or missing token")]
#[status(401)]
struct BadToken;

#[derive(Serialize)]
struct CreateTransfer {
    id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTransferBody {
    filename: String,
    #[serde(with = "serde_util::mime")]
    content_type: mime::Mime,
}

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
#[status(500)]
enum CreateTransferError {
    #[error("Couldn't create jwt")]
    Jwt,
    #[error("Couldn't create cleanup")]
    Actix,
}

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
enum WaitTransferError {
    #[error("This transfer doesn't exist or is started already")]
    #[status(400)]
    NoTransfer,
    #[error("No receiver connected in 60s, try again")]
    #[status(504)]
    Timeout,
    #[error("The transfer was closed")]
    #[status(400)]
    TransferClosed,
}

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
enum ReceiveError {
    #[error("This transfer doesn't exist")]
    #[status(400)]
    NoTransfer,
    #[error("The sender disconnected before sending the first byte")]
    #[status(400)]
    SenderDisconnected,
}

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
enum SendError {
    #[error("The receiver disconnected")]
    #[status(400)]
    ReceiverDisconnected,
    #[error("The payload couldn't be processed: {0}")]
    #[status(400)]
    PayloadError(PayloadError),
    #[error("Transfer timed out")]
    #[status(400)]
    Timeout,
}

#[put("")]
async fn create_transfer(
    transfers: web::Data<Transfers>,
    web::Json(body): web::Json<CreateTransferBody>,
    cleanup: web::Data<Addr<Cleanup>>,
    encode_config: web::Data<EncodeConfig>,
) -> Result<HttpResponse, CreateTransferError> {
    let id = transfers.new_transfer(body.filename, body.content_type);
    cleanup
        .send(TrackTransfer(id))
        .await
        .map_err(|_| CreateTransferError::Actix)?;
    let token = jwt::encode_token(
        &encode_config,
        &TransferClaims::sender(id, time::Duration::minutes(10)),
    )
    .map_err(|_| CreateTransferError::Jwt)?;

    Ok(HttpResponse::Ok()
        .cookie(
            CookieBuilder::new("frachter-transfer", token)
                .max_age(time::Duration::minutes(10))
                .http_only(true)
                .finish(),
        )
        .json(CreateTransfer { id }))
}

#[get("/wait")]
async fn wait_transfer(
    transfers: web::Data<Transfers>,
    claims: ReqData<TransferClaims>,
) -> Result<HttpResponse, WaitTransferError> {
    let mut rx = transfers
        .receiver_rx(&claims.id)
        .ok_or(WaitTransferError::NoTransfer)?;
    match tokio::time::timeout(Duration::from_secs(60), rx.changed()).await {
        Ok(Ok(_)) => match *rx.borrow() {
            true => Ok(HttpResponse::NoContent().finish()),
            false => Err(WaitTransferError::Timeout),
        },
        Ok(Err(_)) => Err(WaitTransferError::TransferClosed),
        Err(_) => Err(WaitTransferError::Timeout),
    }
}

#[get("/{id}")]
async fn receive(
    transfers: web::Data<Transfers>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ReceiveError> {
    let ReceiverInfo {
        filename,
        content_type,
        content_length_rx,
        body,
    } = transfers.receive(&id, 1).ok_or(ReceiveError::NoTransfer)?;
    let content_length =
        match tokio::time::timeout(Duration::from_secs(5 * 60), content_length_rx).await {
            Ok(Ok(x)) => x,
            _ => return Err(ReceiveError::SenderDisconnected),
        };

    let mut res = HttpResponse::Ok();
    res.insert_header((
        header::CONTENT_DISPOSITION,
        ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![DispositionParam::Filename(filename)],
        },
    ))
    .insert_header((header::CONTENT_TYPE, ContentType(content_type)));
    if let Some(length) = content_length {
        res.insert_header((header::CONTENT_LENGTH, length));
    }

    Ok(res.body(body))
}

#[post("/send")]
async fn send(
    cleanup: web::Data<Addr<Cleanup>>,
    SendTransfer(mut info): SendTransfer,
    claims: ReqData<TransferClaims>,
    mut payload: Payload,
    req: HttpRequest,
) -> Result<HttpResponse, SendError> {
    if info
        .content_length_tx
        .send(
            req.headers()
                .get(header::CONTENT_LENGTH)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<usize>().ok()),
        )
        .is_err()
    {
        cleanup.send(PutStatus(claims.id, false)).await.ok();
        return Err(SendError::ReceiverDisconnected);
    }

    let timeout_ts = tokio::time::Instant::now() + Duration::from_secs(10);

    loop {
        match tokio::time::timeout_at(timeout_ts, payload.next()).await {
            // got payload
            Ok(Some(Ok(buf))) => {
                if info.sender.send(buf).await.is_err() {
                    cleanup.send(PutStatus(claims.id, false)).await.ok();
                    return Err(SendError::ReceiverDisconnected);
                }
            }
            // payload error
            Ok(Some(Err(e))) => {
                cleanup.send(PutStatus(claims.id, false)).await.ok();
                return Err(SendError::PayloadError(e));
            }
            // finished sending
            Ok(None) => {
                cleanup.send(PutStatus(claims.id, true)).await.ok();
                return Ok(HttpResponse::NoContent().finish());
            }
            // timeout
            Err(_) => {
                cleanup.send(PutStatus(claims.id, false)).await.ok();
                return Err(SendError::Timeout);
            }
        }
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = parse_config();
    tracing_subscriber::fmt()
        .with_env_filter(match &config.log_filter {
            Some(f) => EnvFilter::new(f),
            None => EnvFilter::from_default_env(),
        })
        .init();
    let transfers = Transfers::new();
    let cleanup = Cleanup::new(transfers.clone()).start();

    let (transfers, cleanup) = (web::Data::new(transfers), web::Data::new(cleanup));
    let encode_config = web::Data::new((
        EncodingKey::from_base64_secret(&config.jwt_secret).unwrap(),
        jsonwebtoken::Header::default(),
    ));
    let decode_config = Arc::new((
        DecodingKey::from_base64_secret(&config.jwt_secret).unwrap(),
        jsonwebtoken::Validation::default(),
    ));
    let token = Arc::new(config.token);
    HttpServer::new(move || {
        let token = token.clone();
        App::new()
            .wrap(TracingLogger::default())
            .app_data(transfers.clone())
            .app_data(cleanup.clone())
            .app_data(encode_config.clone())
            .service(
                web::scope("/api")
                    .service(
                        web::scope("/transfers")
                            .wrap(RequireToken(token.clone()))
                            .service(create_transfer),
                    )
                    .service(
                        web::scope("/transfer")
                            .wrap(RequireToken(token))
                            .wrap(JwtDecoder(decode_config.clone()))
                            .service(wait_transfer)
                            .service(send),
                    )
                    .service(web::scope("/receive").service(receive)),
            )
            .service(
                Files::new("/", "static")
                    .prefer_utf8(true)
                    .use_etag(false)
                    .index_file("index.html"),
            )
    })
    .bind(&config.bind)?
    .run()
    .await
}
