use actix_web::body::{BodySize, MessageBody};
use bytes::Bytes;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

pub fn new<E: Into<Box<dyn std::error::Error>>>(size: usize) -> (Sender<E>, impl MessageBody) {
    let (tx, rx) = mpsc::channel(size);
    (Sender::new(tx), Receiver::new(rx))
}

/// A channel-like sender for body chunks.
#[derive(Debug, Clone)]
pub struct Sender<E> {
    tx: mpsc::Sender<Result<Bytes, E>>,
}

impl<E> Sender<E> {
    fn new(tx: mpsc::Sender<Result<Bytes, E>>) -> Self {
        Self { tx }
    }

    /// Submits a chunk of bytes to the response body stream.
    ///
    /// # Errors
    /// Errors if other side of channel body was dropped, returning `chunk`.
    pub async fn send(&mut self, chunk: Bytes) -> Result<(), Bytes> {
        self.tx
            .send(Ok(chunk))
            .await
            .map_err(|mpsc::error::SendError(err)| match err {
                Ok(chunk) => chunk,
                Err(_) => unreachable!(), // we always send Ok(chunk)
            })
    }

    /// Closes the stream, optionally sending an error.
    ///
    /// # Errors
    /// Errors if closing with error and other side of channel body was dropped, returning `error`.
    #[allow(unused)]
    pub async fn close(self, error: Option<E>) -> Result<(), E> {
        if let Some(err) = error {
            return self
                .tx
                .send(Err(err))
                .await
                .map_err(|mpsc::error::SendError(err)| match err {
                    Ok(_) => unreachable!(), // we always send Err(err)
                    Err(err) => err,
                });
        }

        Ok(())
    }
}

#[derive(Debug)]
struct Receiver<E> {
    rx: mpsc::Receiver<Result<Bytes, E>>,
}

impl<E> Receiver<E> {
    fn new(rx: mpsc::Receiver<Result<Bytes, E>>) -> Self {
        Self { rx }
    }
}

impl<E> MessageBody for Receiver<E>
where
    E: Into<Box<dyn std::error::Error>>,
{
    type Error = E;

    fn size(&self) -> BodySize {
        BodySize::Stream
    }

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        self.rx.poll_recv(cx)
    }
}
