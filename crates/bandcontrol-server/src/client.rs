//! TCP test client and a client side state mirror, used by integration tests, e2e tests and `bandctl`.

use std::{collections::VecDeque, io, net::SocketAddr, time::Duration};

use bandcontrol_proto::{
    client::{ClientError, ClientEvent, ClientSession},
    pairing::PairingSecret,
    pb,
    random::OsRandom,
    server::SessionId,
};
use tokio::net::{
    TcpStream,
    tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::tcp::{read_packet, write_packet};

#[derive(Debug, thiserror::Error)]
pub enum TcpClientError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Session(#[from] ClientError),
    #[error("timed out")]
    Timeout,
}

#[derive(Debug)]
pub struct TcpClient {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    session: ClientSession,
    queue: VecDeque<pb::ServerMessage>,
    pub session_id: SessionId,
    pub server_name: String,
}

impl TcpClient {
    /// Connects and authenticates.
    pub async fn connect(addr: SocketAddr, secret: &PairingSecret, max_packet: usize) -> Result<Self, TcpClientError> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let (mut reader, mut writer) = stream.into_split();
        let mut session = ClientSession::new(secret, "bandctl", max_packet, &mut OsRandom);
        for p in session.hello() {
            write_packet(&mut writer, &p).await?;
        }
        let mut queue = VecDeque::new();
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), read_packet(&mut reader))
                .await
                .map_err(|_| TcpClientError::Timeout)??;
            for event in session.receive(&packet)? {
                match event {
                    ClientEvent::Send(packets) => {
                        for p in packets {
                            write_packet(&mut writer, &p).await?;
                        }
                    }
                    ClientEvent::Authenticated {
                        session: session_id,
                        server_name,
                    } => {
                        return Ok(Self {
                            reader,
                            writer,
                            session,
                            queue,
                            session_id,
                            server_name,
                        });
                    }
                    ClientEvent::Message(m) => queue.push_back(m),
                }
            }
        }
    }

    pub async fn next(&mut self) -> Result<pb::ServerMessage, TcpClientError> {
        loop {
            if let Some(m) = self.queue.pop_front() {
                return Ok(m);
            }
            let packet = read_packet(&mut self.reader).await?;
            for event in self.session.receive(&packet)? {
                if let ClientEvent::Message(m) = event {
                    self.queue.push_back(m);
                }
            }
        }
    }

    pub async fn next_timeout(&mut self, timeout: Duration) -> Result<pb::ServerMessage, TcpClientError> {
        tokio::time::timeout(timeout, self.next())
            .await
            .map_err(|_| TcpClientError::Timeout)?
    }

    pub async fn send(&mut self, msg: pb::client_message::Msg) -> Result<(), TcpClientError> {
        let packets = self.session.send(&pb::ClientMessage { msg: Some(msg) })?;
        for p in packets {
            write_packet(&mut self.writer, &p).await?;
        }
        Ok(())
    }

    /// Reads messages into `mirror` until `pred` holds.
    pub async fn wait_for(
        &mut self,
        mirror: &mut Mirror,
        timeout: Duration,
        mut pred: impl FnMut(&Mirror) -> bool,
    ) -> Result<(), TcpClientError> {
        let deadline = tokio::time::Instant::now() + timeout;
        while !pred(mirror) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let msg = self.next_timeout(remaining).await?;
            mirror.apply(&msg);
        }
        Ok(())
    }
}

/// What a client knows about the server state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mirror {
    pub state: Option<pb::State>,
    pub transport: Option<pb::Transport>,
    pub states_received: usize,
    pub deltas_received: usize,
}

impl Mirror {
    pub fn apply(&mut self, msg: &pb::ServerMessage) {
        use pb::server_message::Msg;

        match &msg.msg {
            Some(Msg::State(s)) => {
                self.state = Some(s.clone());
                self.states_received += 1;
            }
            Some(Msg::Transport(t)) => self.transport = Some(*t),
            Some(Msg::Gains(g)) => {
                self.deltas_received += 1;
                let Some(state) = &mut self.state else { return };
                for tg in &g.tracks {
                    if let Some(t) = state.tracks.iter_mut().find(|t| t.id == tg.track) {
                        t.volume = tg.volume;
                    }
                }
                for rg in &g.receives {
                    let receive = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.id == rg.track)
                        .and_then(|t| t.receives.get_mut(usize::try_from(rg.index).ok()?));
                    if let Some(r) = receive {
                        r.volume = rg.volume;
                        r.mute = rg.mute;
                    }
                }
            }
            Some(Msg::Welcome(_)) | None => {}
        }
    }

    #[must_use]
    pub fn track(&self, name: &str) -> Option<&pb::Track> {
        self.state.as_ref()?.tracks.iter().find(|t| t.name == name)
    }
}
