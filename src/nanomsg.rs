use napi::{
    bindgen_prelude::*,
    threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use std::{
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
    client: nng::Socket,
    connected: bool,
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
        Ok(Socket {
            client: Self::create_client(&opt)?,
            connected: false,
            options: opt,
        })
    }

    pub fn create_client(opt: &SocketOptions) -> Result<nng::Socket> {
        nng::Socket::new(Protocol::Pair1)
            .map(|client| {
                let recv_timeout = opt.recv_timeout.unwrap_or(5000);
                let send_timeout = opt.send_timeout.unwrap_or(5000);
                let _ = client.set_opt::<RecvTimeout>(Some(Duration::from_millis(recv_timeout.try_into().unwrap())));
                let _ = client.set_opt::<SendTimeout>(Some(Duration::from_millis(send_timeout.try_into().unwrap())));
                client
            })
            .map_err(|e| {
                let error_message = format!("Initiate socket failed: {}", e);
                eprintln!("{}", error_message);
                Error::from_reason(error_message)
            })
    }

    #[napi]
    pub fn connect(&mut self, url: String) -> Result<()> {
        let ret = self
            .client
            .dial(&url)
            .map_err(|e| {
                let error_message = format!("Connect {} failed: {}", url, e);
                eprintln!("{}", error_message);
                Error::from_reason(error_message)
            });
        self.connected = ret.is_ok();
        return ret;
    }

    #[napi]
    pub fn send(&self, req: Buffer) -> Result<Buffer> {
        let msg = nng::Message::from(&req[..]);
        self
            .client
            .send(msg)
            .map_err(|(_, e)| {
                let error_message = format!("Send rpc failed: {}", e);
                eprintln!("{}", error_message);
                Error::from_reason(error_message)
            })?;
        self
            .client
            .recv()
            .map(|msg| {
                msg.as_slice().into()
            })
            .map_err(|e| {
                let error_message = format!("Recv rpc failed: {}", e);
                eprintln!("{}", error_message);
                Error::from_reason(error_message)
            })
    }

    #[napi]
    pub fn close(&mut self) {
        self.client.close();
        self.connected = false;
        eprintln!("Socket closed");
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
        client.set_opt::<RecvTimeout>(None).map_err(|e| {
            let error_message = format!("Failed to set recv_timeout: {}", e);
            eprintln!("{}", error_message);
            Error::from_reason(error_message)
        })?;

        client.set_opt::<SendTimeout>(None).map_err(|e| {
            let error_message = format!("Failed to set send_timeout: {}", e);
            eprintln!("{}", error_message);
            Error::from_reason(error_message)
        })?;

        client.dial(&url).map_err(|e| {
            let error_message = format!("Failed to connect: {}", e);
            eprintln!("{}", error_message);
            Error::new(Status::GenericFailure, error_message)
        })?;
        
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || loop {
            if let Ok(_) = rx.try_recv() {
                eprintln!("Stopping message reception, closing client");
                client.close();
                break;
            }
            match client.recv() {
                Ok(msg) => {
                    callback.clone().call(
                        Ok(msg.as_slice().into()),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
                Err(e) => {
                    eprintln!("Error receiving message: {}", e);
                    callback.clone().call(
                        Err(Error::from_reason(format!("Recv failed: {}", e))),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                    if let nng::Error::Closed = e {
                        eprintln!("Socket closed, stopping message reception");
                        return;
                    }
                }
            }
        });
        return Ok(MessageRecvDisposable { closed: false, tx });
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
            self.tx.send(()).map_err(|e| {
                let error_message = format!("Failed to stop msg channel: {}", e);
                eprintln!("{}", error_message);
                Error::from_reason(error_message)
            })?;
            self.closed = true;
        }
        Ok(())
    }
}