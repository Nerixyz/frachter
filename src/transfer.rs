use crate::{
    bounded_body,
    jwt::{TransferClaims, TransferRole},
    mutex::MutexExt,
};
use actix_web::{body::MessageBody, dev::Payload, web, FromRequest, HttpMessage, HttpRequest};
use std::{
    collections::HashMap,
    convert::Infallible,
    future::{ready, Ready},
    sync::{Arc, Mutex as StdMutex},
};
use tokio::sync::{oneshot, watch};
use uuid::Uuid;

#[derive(Clone)]
pub struct Transfers(pub Arc<StdMutex<HashMap<Uuid, TransferInfo>>>);

#[derive(Debug)]
pub enum TransferInfo {
    WaitingForReceiver {
        filename: String,
        content_type: mime::Mime,
        receiver_tx: watch::Sender<bool>,
        receiver_rx: watch::Receiver<bool>,
    },
    WaitingForSender {
        sender: TransferSender,
        content_length_tx: oneshot::Sender<Option<usize>>,
    },
}

pub type TransferSender = bounded_body::Sender<Infallible>;

pub struct ReceiverInfo<B> {
    pub filename: String,
    pub content_type: mime::Mime,
    pub content_length_rx: oneshot::Receiver<Option<usize>>,
    pub body: B,
}

pub struct SenderInfo {
    pub sender: TransferSender,
    pub content_length_tx: oneshot::Sender<Option<usize>>,
}

impl Transfers {
    pub fn new() -> Self {
        Self(Arc::new(StdMutex::new(HashMap::new())))
    }
    pub fn new_transfer(&self, filename: String, content_type: mime::Mime) -> Uuid {
        let id = Uuid::new_v4();
        let (receiver_tx, receiver_rx) = watch::channel(false);
        self.0.always_lock().insert(
            id,
            TransferInfo::WaitingForReceiver {
                filename,
                content_type,
                receiver_rx,
                receiver_tx,
            },
        );

        id
    }

    pub fn receiver_rx(&self, id: &Uuid) -> Option<watch::Receiver<bool>> {
        match self.0.always_lock().get(id)? {
            TransferInfo::WaitingForReceiver { receiver_rx, .. } => Some(receiver_rx.clone()),
            TransferInfo::WaitingForSender { .. } => None,
        }
    }

    pub fn receive(&self, id: &Uuid, n_buffers: usize) -> Option<ReceiverInfo<impl MessageBody>> {
        let mut lock = self.0.always_lock();
        let transfer = lock.get_mut(id)?;
        if !matches!(transfer, TransferInfo::WaitingForReceiver { .. }) {
            return None;
        }

        let (sender, body) = bounded_body::new(n_buffers);
        let (content_length_tx, content_length_rx) = oneshot::channel();
        let transfer = std::mem::replace(
            transfer,
            TransferInfo::WaitingForSender {
                sender,
                content_length_tx,
            },
        );
        match transfer {
            TransferInfo::WaitingForReceiver {
                receiver_tx,
                content_type,
                filename,
                ..
            } => {
                receiver_tx.send(true).ok();
                Some(ReceiverInfo {
                    filename,
                    content_type,
                    content_length_rx,
                    body,
                })
            }
            _ => unreachable!(),
        }
    }

    pub fn take_sender(&self, id: &Uuid) -> Option<SenderInfo> {
        let mut transfers = self.0.always_lock();
        if !matches!(
            transfers.get(id),
            Some(TransferInfo::WaitingForSender { .. })
        ) {
            return None;
        }

        match transfers.remove(id) {
            Some(TransferInfo::WaitingForSender {
                sender,
                content_length_tx,
            }) => Some(SenderInfo {
                content_length_tx,
                sender,
            }),
            _ => unreachable!(),
        }
    }
}

pub struct SendTransfer(pub SenderInfo);

#[derive(Debug, thiserror::Error, actix_web_error::Json)]
pub enum SendTransferError {
    #[error("Bad token provided")]
    #[status(401)]
    BadToken,
    #[error("This transfer doesn't exist")]
    #[status(400)]
    NoTransfer,
    #[error("No info about transfers")]
    #[status(500)]
    NoRequestInfo,
}

impl FromRequest for SendTransfer {
    type Error = SendTransferError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(
            match (
                req.extensions().get::<TransferClaims>(),
                req.app_data::<web::Data<Transfers>>(),
            ) {
                (Some(claims), Some(transfers)) => {
                    if claims.role != TransferRole::Sender {
                        Err(SendTransferError::BadToken)
                    } else {
                        transfers
                            .take_sender(&claims.id)
                            .map(Self)
                            .ok_or(SendTransferError::NoTransfer)
                    }
                }
                _ => Err(SendTransferError::NoRequestInfo),
            },
        )
    }
}
