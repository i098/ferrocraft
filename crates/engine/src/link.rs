//! The server connection: a login on a worker thread (`GuiConnecting`), then the live session.

use ferrocraft_net_client::{Auth, Session};
use std::io;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

#[derive(Default)]
pub enum Link {
    #[default]
    Offline,
    /// A login in progress; dropping the link abandons it.
    Connecting(Receiver<io::Result<Session>>),
    Live(Session),
}

impl Link {
    /// Log in to `addr` (`host:port`) as `username` (with `auth` for online-mode servers) without blocking the render loop.
    pub fn connect(addr: String, username: String, auth: Option<Auth>) -> Link {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(Session::connect(&addr, &username, auth.as_ref()));
        });
        Link::Connecting(rx)
    }

    /// Moves a finished login to `Live`. Returns the login error, once.
    pub fn poll_login(&mut self) -> Option<io::Error> {
        let Link::Connecting(rx) = self else {
            return None;
        };
        match rx.try_recv() {
            Ok(Ok(s)) => *self = Link::Live(s),
            Ok(Err(e)) => {
                *self = Link::Offline;
                return Some(e);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                *self = Link::Offline;
                return Some(io::Error::other("login thread ended"));
            }
        }
        None
    }

    pub fn session(&mut self) -> Option<&mut Session> {
        match self {
            Link::Live(s) => Some(s),
            _ => None,
        }
    }
}
