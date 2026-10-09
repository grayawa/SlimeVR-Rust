//! Disk acknowledgements wait here while the pose loop keeps advancing.
use crate::{
    api::{error_wire, Wire},
    config::Completion,
};
use std::collections::VecDeque;
use tokio::sync::oneshot;

struct Reply {
    revision: u64,
    wire: Vec<Wire>,
    sender: oneshot::Sender<Vec<Wire>>,
}
#[derive(Default)]
pub(super) struct Replies {
    pending: VecDeque<Reply>,
}
impl Replies {
    pub fn full(&mut self) -> bool {
        // Release slots belonging to disconnected or timed-out clients.
        self.pending.retain(|reply| !reply.sender.is_closed());
        self.pending.len() >= 32
    }
    pub fn defer(&mut self, revision: u64, wire: Vec<Wire>, sender: oneshot::Sender<Vec<Wire>>) {
        self.pending.push_back(Reply {
            revision,
            wire,
            sender,
        });
    }
    pub fn complete(&mut self, completion: Completion) {
        while self
            .pending
            .front()
            .is_some_and(|reply| reply.revision <= completion.revision)
        {
            let reply = self.pending.pop_front().unwrap();
            let wire = match &completion.result {
                Ok(()) => reply.wire,
                Err(error) => vec![error_wire(error.clone())],
            };
            let _ = reply.sender.send(wire);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledgements_wait_for_durable_revision_and_errors_do_not_claim_success() {
        let mut pending = Replies::default();
        let (one, mut first) = oneshot::channel();
        let (two, mut second) = oneshot::channel();
        pending.defer(1, vec![Wire::Text("first".into())], one);
        pending.defer(3, vec![Wire::Text("second".into())], two);
        assert!(first.try_recv().is_err());
        pending.complete(Completion {
            revision: 2,
            result: Ok(()),
            previous_error: None,
        });
        assert!(matches!(&first.try_recv().unwrap()[0], Wire::Text(value) if value=="first"));
        assert!(second.try_recv().is_err());
        pending.complete(Completion {
            revision: 3,
            result: Err("disk failed".into()),
            previous_error: None,
        });
        assert!(
            matches!(&second.try_recv().unwrap()[0], Wire::Text(value) if value.contains("backend_error") && value.contains("disk failed"))
        );
        assert!(pending.pending.is_empty());
    }
    #[test]
    fn disconnected_clients_release_slots_even_when_disk_has_not_completed() {
        let mut pending = Replies::default();
        let mut clients = Vec::new();
        for revision in 1..=32 {
            let (sender, client) = oneshot::channel();
            pending.defer(revision, Vec::new(), sender);
            clients.push(client);
        }
        assert!(pending.full());
        drop(clients.pop());
        assert!(!pending.full());
        let (sender, client) = oneshot::channel();
        pending.defer(33, Vec::new(), sender);
        clients.push(client);
        assert!(pending.full());
        drop(clients);
        assert!(!pending.full());
        assert!(pending.pending.is_empty());
    }
}
