use tokio::time::{delay_queue, DelayQueue, Error};
use super::UStub;

use futures_03::ready;
use std::collections::HashMap;
use std::task::{Context, Poll};
use std::time::Duration;

pub type CacheKey = std::net::SocketAddr;

pub struct Cache {
    entries: HashMap<CacheKey, (UStub, delay_queue::Key)>,
    expirations: DelayQueue<CacheKey>,
}

const TTL_SECS: u64 = 60;

impl Cache {
    fn insert(&mut self, key: CacheKey, value: UStub) {
        let delay = self.expirations
            .insert(key.clone(), Duration::from_secs(TTL_SECS));

        self.entries.insert(key, (value, delay));
    }

    fn get(&self, key: &CacheKey) -> Option<&UStub> {
        self.entries.get(key)
            .map(|&(ref v, _)| v)
    }

    fn remove(&mut self, key: &CacheKey) {
        if let Some((_, cache_key)) = self.entries.remove(key) {
            self.expirations.remove(&cache_key);
        }
    }

    fn poll_purge(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        while let Some(res) = ready!(self.expirations.poll_expired(cx)) {
            let entry = res?;
            self.entries.remove(entry.get_ref());
        }

        Poll::Ready(Ok(()))
    }
}
