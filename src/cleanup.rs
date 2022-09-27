use crate::{mutex::MutexExt, Transfers};
use actix::{Actor, AsyncContext, Context, Handler, Message, MessageResult, SpawnHandle};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tracing::debug;
use uuid::Uuid;

const TRANSFER_DURATION: Duration = Duration::from_secs(10 * 60);
const STATUS_DURATION: Duration = Duration::from_secs(60);

pub struct Cleanup {
    transfers: Transfers,
    statuses: HashMap<Uuid, (Instant, bool)>,
    pending: Vec<(Uuid, Instant)>,

    pending_handle: Option<SpawnHandle>,
    status_handle: Option<SpawnHandle>,
}

impl Cleanup {
    pub fn new(transfers: Transfers) -> Self {
        Self {
            transfers,
            statuses: HashMap::new(),
            pending: Vec::new(),
            pending_handle: None,
            status_handle: None,
        }
    }

    fn process_pending(&mut self, ctx: &mut Context<Self>) {
        debug!("Processing pending transfers");
        let mut transfers = self.transfers.0.always_lock();
        let now = Instant::now();
        self.pending.retain(|(id, start)| {
            if now - *start > TRANSFER_DURATION {
                transfers.remove(id);
                false
            } else {
                true
            }
        });
        self.pending_handle = if self.pending.is_empty() {
            None
        } else {
            let next_check = self
                .pending
                .iter()
                .min_by_key(|t| t.1)
                .map(|t| t.1)
                .unwrap_or_else(Instant::now)
                .saturating_duration_since(Instant::now())
                .max(Duration::from_secs(1));
            dbg!(&self.pending);
            dbg!(&next_check);
            Some(ctx.run_later(next_check, Self::process_pending))
        }
    }

    fn process_statuses(&mut self, ctx: &mut Context<Self>) {
        debug!("Processing pending statuses");
        let now = Instant::now();
        self.statuses
            .retain(|_, (start, _)| now - *start < STATUS_DURATION);

        self.status_handle = if self.statuses.is_empty() {
            None
        } else {
            let next_check = self
                .statuses
                .iter()
                .min_by_key(|(_, (i, _))| i)
                .map(|(_, (i, _))| *i)
                .unwrap_or_else(Instant::now)
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(1));
            Some(ctx.run_later(next_check, Self::process_statuses))
        }
    }
}

#[derive(Message)]
#[rtype("()")]
pub struct TrackTransfer(pub Uuid);

#[derive(Message)]
#[rtype("()")]
pub struct PutStatus(pub Uuid, pub bool);

#[derive(Message)]
#[rtype("Option<bool>")]
pub struct GetStatus(pub Uuid);

impl Actor for Cleanup {
    type Context = Context<Self>;
}

impl Handler<TrackTransfer> for Cleanup {
    type Result = ();

    fn handle(
        &mut self,
        TrackTransfer(id): TrackTransfer,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.pending_handle.is_none() {
            self.pending_handle = Some(ctx.run_later(TRANSFER_DURATION, Self::process_pending));
        }
        self.pending.push((id, Instant::now()));
    }
}

impl Handler<PutStatus> for Cleanup {
    type Result = ();

    fn handle(
        &mut self,
        PutStatus(id, status): PutStatus,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.pending_handle.is_none() {
            self.pending_handle = Some(ctx.run_later(STATUS_DURATION, Self::process_statuses));
        }
        self.statuses.insert(id, (Instant::now(), status));
    }
}

impl Handler<GetStatus> for Cleanup {
    type Result = MessageResult<GetStatus>;

    fn handle(&mut self, GetStatus(id): GetStatus, _: &mut Self::Context) -> Self::Result {
        MessageResult(self.statuses.get(&id).map(|(_, s)| *s))
    }
}
