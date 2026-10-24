//! Matches replies to the commands waiting for them, in order. A command line
//! with n commands waits for n replies; after a failed one, tmux skips the
//! rest of the line and sends nothing for it.

use std::collections::VecDeque;

use tokio::sync::oneshot;

use crate::Error;
use crate::parser::Reply;

pub(crate) type Slot = oneshot::Receiver<Result<Reply, Error>>;

#[derive(Debug, Default)]
pub(crate) struct Pending {
    queue: VecDeque<Waiter>,
    /// Set when the client exited; no reply will ever come.
    closed: Option<String>,
}

#[derive(Debug)]
struct Waiter {
    tx: oneshot::Sender<Result<Reply, Error>>,
    /// Slots after this one that belong to the same command line.
    left: usize,
}

impl Pending {
    /// Queues n slots for one command line.
    pub(crate) fn push_line(&mut self, n: usize) -> Result<Vec<Slot>, Error> {
        if let Some(reason) = &self.closed {
            return Err(Error::Exited(reason.clone()));
        }
        Ok((0..n)
            .map(|i| {
                let (tx, rx) = oneshot::channel();
                self.queue.push_back(Waiter {
                    tx,
                    left: n - 1 - i,
                });
                rx
            })
            .collect())
    }

    /// Removes the last n slots again, when the line could not be written.
    pub(crate) fn pop_line(&mut self, n: usize) {
        for _ in 0..n {
            self.queue.pop_back();
        }
    }

    pub(crate) fn deliver(&mut self, reply: Reply) {
        let Some(waiter) = self.queue.pop_front() else {
            tracing::warn!("reply with no command waiting for it");
            return;
        };
        let failed = reply.failed;
        let block = reply.block;
        // A dropped receiver means the caller gave up waiting; nothing to do.
        let _ = waiter.tx.send(if failed {
            Err(Error::tmux(&reply.lines))
        } else {
            Ok(reply)
        });
        if failed {
            for _ in 0..waiter.left {
                if let Some(skipped) = self.queue.pop_front() {
                    let _ = skipped.tx.send(Err(Error::Skipped { block }));
                }
            }
        }
    }

    /// Fails every waiting command, when the client exits.
    pub(crate) fn fail_all(&mut self, reason: &str) {
        self.closed = Some(reason.to_owned());
        for waiter in self.queue.drain(..) {
            let _ = waiter.tx.send(Err(Error::Exited(reason.to_owned())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(failed: bool, block: u64) -> Reply {
        Reply {
            lines: vec![b"x".to_vec()],
            failed,
            block,
        }
    }

    #[test]
    fn delivers_in_order() {
        let mut p = Pending::default();
        let mut a = p.push_line(1).unwrap();
        let mut b = p.push_line(1).unwrap();
        p.deliver(reply(false, 1));
        p.deliver(reply(false, 2));
        assert_eq!(a.remove(0).try_recv().unwrap().unwrap().block, 1);
        assert_eq!(b.remove(0).try_recv().unwrap().unwrap().block, 2);
    }

    #[test]
    fn an_error_skips_the_rest_of_its_line_only() {
        let mut p = Pending::default();
        let mut line = p.push_line(3).unwrap();
        let mut next = p.push_line(1).unwrap();
        p.deliver(reply(false, 1));
        p.deliver(reply(true, 2));
        p.deliver(reply(false, 3));
        assert!(line.remove(0).try_recv().unwrap().is_ok());
        assert!(matches!(
            line.remove(0).try_recv().unwrap(),
            Err(Error::Tmux { .. })
        ));
        assert!(matches!(
            line.remove(0).try_recv().unwrap(),
            Err(Error::Skipped { block: 2 })
        ));
        assert_eq!(next.remove(0).try_recv().unwrap().unwrap().block, 3);
    }

    #[test]
    fn pop_line_forgets_an_unwritten_line() {
        let mut p = Pending::default();
        let mut kept = p.push_line(1).unwrap();
        let _dropped = p.push_line(2).unwrap();
        p.pop_line(2);
        p.deliver(reply(false, 1));
        assert!(kept.remove(0).try_recv().unwrap().is_ok());
    }

    #[test]
    fn fail_all_answers_everyone() {
        let mut p = Pending::default();
        let mut slots = p.push_line(2).unwrap();
        p.fail_all("gone");
        for mut s in slots.drain(..) {
            assert!(matches!(s.try_recv().unwrap(), Err(Error::Exited(_))));
        }
        assert!(matches!(p.push_line(1), Err(Error::Exited(_))));
    }
}
