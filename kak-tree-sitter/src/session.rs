use std::{collections::HashMap, fs::File, io::Write, process::Stdio};

use mio::Token;

use crate::{error::OhNo, response::Response};

/// Session tracker,
///
/// Responsible for tracking sessions (by names) along with the associated command token and buffer token.
#[derive(Debug, Default)]
pub struct SessionTracker {
  sessions: HashMap<String, Session>,
  fifos: HashMap<Token, Fifo>,
}

impl SessionTracker {
  pub fn is_empty(&self) -> bool {
    self.sessions.is_empty()
  }

  pub fn track(
    &mut self,
    session_name: impl Into<String>,
    session: Session,
    cmd_fifo: Fifo,
    buf_fifo: Fifo,
  ) {
    self.fifos.insert(session.cmd_token, cmd_fifo);
    self.fifos.insert(session.buf_token, buf_fifo);
    self.sessions.insert(session_name.into(), session);
  }

  pub fn untrack(
    &mut self,
    session_name: impl AsRef<str>,
  ) -> Option<(Session, Option<Fifo>, Option<Fifo>)> {
    if let Some(session) = self.sessions.remove(session_name.as_ref()) {
      let cmd_fifo = self.fifos.remove(&session.cmd_token);
      let buf_fifo = self.fifos.remove(&session.buf_token);
      Some((session, cmd_fifo, buf_fifo))
    } else {
      None
    }
  }

  pub fn by_token(&mut self, token: Token) -> Option<(&mut Session, &mut Fifo)> {
    self.fifos.get_mut(&token).and_then(|fifo| {
      self
        .sessions
        .get_mut(fifo.session())
        .map(|session| (session, fifo))
    })
  }
}

/// An (active) session.
#[derive(Debug)]
pub struct Session {
  name: String,
  state: SessionState,
  cmd_token: Token,
  buf_token: Token,
}

impl Session {
  pub fn new(name: impl Into<String>, cmd_token: Token, buf_token: Token) -> Self {
    Self {
      name: name.into(),
      state: SessionState::Idle,
      cmd_token,
      buf_token,
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn cmd_token(&self) -> Token {
    self.cmd_token
  }

  pub fn buf_token(&self) -> Token {
    self.buf_token
  }

  pub fn state(&self) -> &SessionState {
    &self.state
  }

  pub fn state_mut(&mut self) -> &mut SessionState {
    &mut self.state
  }

  pub fn send_response(&self, client: Option<&str>, resp: &Response) -> Result<(), OhNo> {
    let resp = resp.to_kak_cmd(client);

    match resp {
      Some(resp) => self.send_response_raw(&resp),
      _ => Ok(()),
    }
  }

  pub fn send_non_connected_response(
    session: &str,
    client: Option<&str>,
    resp: &Response,
  ) -> Result<(), OhNo> {
    let resp = resp.to_kak_cmd(client);

    match resp {
      Some(resp) => Self::send_non_connected_response_raw(session, &resp),
      _ => Ok(()),
    }
  }

  pub fn send_response_raw(&self, resp: &str) -> Result<(), OhNo> {
    Self::send_non_connected_response_raw(&self.name, resp)
  }

  pub fn send_non_connected_response_raw(session: &str, resp: &str) -> Result<(), OhNo> {
    let mut child = std::process::Command::new("kak")
      .args(["-p", session])
      .stdin(Stdio::piped())
      .spawn()
      .map_err(|err| OhNo::CannotSendRequest {
        err: err.to_string(),
      })?;
    let child_stdin = child
      .stdin
      .as_mut()
      .ok_or_else(|| OhNo::CannotSendRequest {
        err: "cannot pipe data to kak -p".to_owned(),
      })?;

    child_stdin
      .write_all(resp.as_bytes())
      .map_err(|err| OhNo::CannotSendRequest {
        err: err.to_string(),
      })?;

    child_stdin.flush().map_err(|err| OhNo::CannotSendRequest {
      err: err.to_string(),
    })?;

    child.wait()?;
    Ok(())
  }
}

/// A FIFO recognized by the server.
///
/// Currently, a FIFO can be used to either send commands, or stream buffer content.
#[derive(Debug)]
pub enum Fifo {
  Cmd {
    session_name: String,
    file: File,
    buffer: String,
  },

  Buf {
    session_name: String,
    file: File,
    buffer: String,
  },
}

impl Fifo {
  pub fn file(&self) -> &File {
    match self {
      Fifo::Cmd { file, .. } => file,
      Fifo::Buf { file, .. } => file,
    }
  }

  pub fn session(&self) -> &str {
    match self {
      Fifo::Cmd { session_name, .. } => session_name.as_str(),
      Fifo::Buf { session_name, .. } => session_name.as_str(),
    }
  }
}

/// State machine used in sessions.
#[derive(Debug)]
pub enum SessionState {
  /// The session is idle.
  Idle,

  /// The session requested highlighting and we are waiting for the buffer content.
  HighlightingWaiting {
    client: String,
    buffer: String,
    lang: String,
    timestamp: u64,
  },
}

impl SessionState {
  pub fn idle(&mut self) {
    *self = SessionState::Idle
  }
}
