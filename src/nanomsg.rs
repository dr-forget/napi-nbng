use napi::{
  bindgen_prelude::*,
  threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use std::{
  sync::{Arc, Mutex},
  sync::mpsc::{self, Sender},
  thread,
  time::Duration,
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

#[napi()]
#[derive(Clone, Debug)]
pub struct Socket {
  client: Arc<Mutex<nng::Socket>>,  // 使用Arc<Mutex<>>来确保线程安全
  connected: Arc<Mutex<bool>>,      // 连接状态使用Arc<Mutex<>>保护
  pub options: SocketOptions,
}

/**
 * A simple Pair1 nanomsg protocol binding
 */
#[napi]
impl Socket {
  #[napi(constructor)]
  pub fn new(options: Option<SocketOptions>) -> Result<Self> {
    let opt = options.unwrap_or_default();
    let client = Self::create_client(&opt)?;
    Ok(Socket {
      client: Arc::new(Mutex::new(client)),
      connected: Arc::new(Mutex::new(false)),
      options: opt,
    })
  }

  pub fn create_client(opt: &SocketOptions) -> Result<nng::Socket> {
    nng::Socket::new(Protocol::Pair1)
      .map(|client| {
        client.set_opt::<RecvTimeout>(Some(Duration::from_millis(
          opt.recv_timeout.unwrap_or(5000) as u64,
        ))).ok();
        client.set_opt::<SendTimeout>(Some(Duration::from_millis(
          opt.send_timeout.unwrap_or(5000) as u64,
        ))).ok();
        client
      })
      .map_err(|e| Error::from_reason(format!("Initiate socket failed: {}", e)))
  }

  #[napi]
  pub fn connect(&mut self, url: String) -> Result<()> {
    let mut client = self.client.lock().unwrap();
    let ret = client.dial(&url)
      .map_err(|e| Error::from_reason(format!("Connect {} failed: {}", url, e)));
    if ret.is_ok() {
      let mut connected = self.connected.lock().unwrap();
      *connected = true;
    }
    ret
  }

  #[napi]
  pub fn send(&self, req: Buffer) -> Result<Buffer> {
    if !*self.connected.lock().unwrap() {
      return Err(Error::from_reason("Socket is not connected"));
    }

    let msg = nng::Message::from(&req[..]);
    let client = self.client.lock().unwrap();
    client
      .send(msg)
      .map_err(|(_, e)| Error::from_reason(format!("Send rpc failed: {}", e)))?;
    client
      .recv()
      .map(|msg| msg.as_slice().into())
      .map_err(|e| Error::from_reason(format!("Recv rpc failed: {}", e)))
  }

  #[napi]
  pub fn close(&mut self) {
    let mut client = self.client.lock().unwrap();
    client.close();
    let mut connected = self.connected.lock().unwrap();
    *connected = false;
  }

  #[napi]
  pub fn connected(&self) -> bool {
    *self.connected.lock().unwrap()
  }

  #[napi(ts_args_type = "callback: (err: null | Error, bytes: Buffer) => void")]
  pub fn recv_message(
    url: String,
    options: Option<SocketOptions>,
    callback: ThreadsafeFunction<Buffer, ErrorStrategy::CalleeHandled>,
  ) -> Result<MessageRecvDisposable> {
    let client = Arc::new(Mutex::new(Self::create_client(&options.unwrap_or_default())?));
    let mut client_lock = client.lock().unwrap();
    client_lock
      .dial(&url)
      .map_err(|e| Error::new(Status::GenericFailure, format!("Failed to connect: {}", e)))?;
    
    let (tx, rx) = mpsc::channel::<()>();
    let client_clone = Arc::clone(&client);

    thread::spawn(move || {
      loop {
        if rx.try_recv().is_ok() {
          client_clone.lock().unwrap().close();
          break;
        }
        
        match client_clone.lock().unwrap().recv() {
          Ok(msg) => {
            callback.clone().call(
              Ok(msg.as_slice().into()),
              ThreadsafeFunctionCallMode::NonBlocking,
            );
          }
          Err(nng::Error::TimedOut) => {
            // 处理超时
            callback.clone().call(
              Err(Error::from_reason("Receive timed out")),
              ThreadsafeFunctionCallMode::NonBlocking,
            );
          }
          Err(nng::Error::Closed) => {
            // 处理关闭情况
            return;
          }
          Err(e) => {
            callback.clone().call(
              Err(Error::from_reason(format!("Receive failed: {}", e))),
              ThreadsafeFunctionCallMode::NonBlocking,
            );
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
      self.tx.send(()).map_err(|e| Error::from_reason(format!("Failed to stop msg channel: {}", e)))?;
      self.closed = true;
    }
    Ok(())
  }
}
