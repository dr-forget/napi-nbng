use napi::{
  bindgen_prelude::*,
  threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use std::{
  sync::mpsc::{self, Sender},
  thread,
};

use nng::{
  options::{Options, RecvTimeout, SendTimeout},
  Protocol,
};

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub struct SocketOptions {
  pub recv_timeout: Option<i32>,
  pub send_timeout: Option<i32>,
}

#[napi]
#[derive(Clone, Debug)]
pub struct Socket {
  client: nng::Socket,
  connected: bool,
  pub options: SocketOptions,
}

#[napi]
impl Socket {
  #[napi(constructor)]
  pub fn new(options: Option<SocketOptions>) -> Result<Self> {
    let opt = options.unwrap_or_default();
    Ok(Socket {
      client: Self::create_client(&opt)?,
      connected: false,
      options: opt,
    })
  }

  pub fn create_client(opt: &SocketOptions) -> Result<nng::Socket> {
    nng::Socket::new(Protocol::Pair1)
      .map(|client| {
        let _ = client.set_opt::<RecvTimeout>(Some(std::time::Duration::from_millis(
          opt.recv_timeout.and_then(|i| i.try_into().ok()).unwrap_or(5000),
        )));
        let _ = client.set_opt::<SendTimeout>(Some(std::time::Duration::from_millis(
          opt.send_timeout.and_then(|i| i.try_into().ok()).unwrap_or(5000),
        )));
        client
      })
      .map_err(|e| Error::from_reason(format!("Initiate socket failed: {}", e)))
  }

  #[napi]
  pub fn connect(&mut self, url: String) -> Result<()> {
    let ret = self
      .client
      .dial(&url)
      .map_err(|e| Error::from_reason(format!("Connect {} failed: {}", url, e)));
    self.connected = ret.is_ok();
    ret
  }

  #[napi]
  pub fn send(&self, req: Buffer) -> Result<Buffer> {
    let msg = nng::Message::from(&req[..]);
    self
      .client
      .send(msg)
      .map_err(|(_, e)| Error::from_reason(format!("Send rpc failed: {}", e)))?;
    self
      .client
      .recv()
      .map(|msg| msg.as_slice().into())
      .map_err(|e| Error::from_reason(format!("Recv rpc failed: {}", e)))
  }

  #[napi]
  pub fn close(&mut self) {
    self.client.close();
    self.connected = false;
  }

  #[napi]
  pub fn connected(&self) -> bool {
    self.connected
  }

  #[napi(ts_args_type = "callback: (err: null | Error, bytes: Buffer) => void")]
  pub fn recv_message(
    url: String,
    options: Option<SocketOptions>,
    callback: ThreadsafeFunction<Buffer, ErrorStrategy::CalleeHandled>,
  ) -> Result<MessageRecvDisposable> {
    let client = Self::create_client(&options.unwrap_or_default())?;
    client.dial(&url).map_err(|e| {
      eprintln!("Failed to connect: {}", e);
      Error::new(Status::GenericFailure, format!("Failed to connect: {}", e))
    })?;

    let (tx, rx) = mpsc::channel::<()>();

    thread::spawn(move || loop {
      if let Ok(_) = rx.try_recv() {
        client.close();
        break;
      }
      match client.recv() {
        Ok(msg) => {
          callback.call(
            Ok(msg.as_slice().into()),
            ThreadsafeFunctionCallMode::NonBlocking,
          );
        }
        Err(e) => {
          match e {
            nng::Error::Closed => {
              eprintln!("Connection closed by the server");
              callback.call(
                Err(Error::new(Status::GenericFailure, "Connection closed by the server")),
                ThreadsafeFunctionCallMode::NonBlocking,
              );
              return;
            }
            nng::Error::TimedOut => {
              eprintln!("Receive operation timed out");
              callback.call(
                Err(Error::new(Status::GenericFailure, "Receive operation timed out")),
                ThreadsafeFunctionCallMode::NonBlocking,
              );
              return;
            }
            _ => {
              eprintln!("Receive error: {}", e);
              callback.call(
                Err(Error::new(Status::GenericFailure, format!("Receive error: {}", e))),
                ThreadsafeFunctionCallMode::NonBlocking,
              );
              return;
            }
          }
        }
      }
    });
    Ok(MessageRecvDisposable { closed: false, tx })
  }
}

#[napi]
pub struct MessageRecvDisposable {
  closed: bool,
  tx: Sender<()>,
}

#[napi]
impl MessageRecvDisposable {
  #[napi]
  pub fn dispose(&mut self) -> Result<()> {
    if !self.closed {
      self
        .tx
        .send(())
        .map_err(|e| Error::from_reason(format!("Failed to stop msg channel: {}", e)))?;
      self.closed = true;
    }
    Ok(())
  }
}