use napi::{
    bindgen_prelude::*,
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use nng::{Socket, Protocol, Error as NngError};
use nng::options::Options;
use napi_derive::napi;
use core::time::Duration;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[napi]
pub struct SocketWrapper {
    socket: Option<Socket>,
    listening: Arc<AtomicBool>,
    url: Option<String>, // 新增字段保存 URL
}

#[napi]
impl SocketWrapper {
    #[napi(constructor)]
    pub fn new() -> Self {
        SocketWrapper {
            socket: None,
            listening: Arc::new(AtomicBool::new(false)),
            url: None, // 初始化为 None
        }
    }

    #[napi]
    pub fn connect(
        &mut self,
        protocol: ProtocolType,
        url: String,
        recv_timeout: u32,
        send_timeout: u32,
    ) -> Result<bool> {
        let socket = Socket::new(protocol.into()).map_err(|err| {
            napi::Error::new(napi::Status::GenericFailure, format!("Socket creation failed: {:?}", err))
        })?;

        self.url = Some(url.clone()); // 保存连接的 URL

        let recv_timeout_duration = if recv_timeout == 0 {
            None
        } else {
            Some(Duration::from_millis(recv_timeout as u64))
        };

        let send_timeout_duration = if send_timeout == 0 {
            None
        } else {
            Some(Duration::from_millis(send_timeout as u64))
        };

        if let Some(timeout) = recv_timeout_duration {
            socket.set_opt::<nng::options::RecvTimeout>(Some(timeout))
                .map_err(|err| napi::Error::new(napi::Status::GenericFailure, format!("Failed to set receive timeout: {:?}", err)))?;
        }

        if let Some(timeout) = send_timeout_duration {
            socket.set_opt::<nng::options::SendTimeout>(Some(timeout))
                .map_err(|err| napi::Error::new(napi::Status::GenericFailure, format!("Failed to set send timeout: {:?}", err)))?;
        }

        socket.dial(&url).map_err(|err| {
            napi::Error::new(napi::Status::GenericFailure, format!("Connection failed: {:?}", err))
        })?;

        self.socket = Some(socket);
        Ok(true)
    }

    #[napi]
    pub fn close(&mut self) {
        self.listening.store(false, Ordering::Relaxed);
        if let Some(socket) = self.socket.take() {
            // 打印 URL
            if let Some(url) = &self.url {
                println!("Closing socket connected to URL: {}", url);
            }
            let _ = socket.close();
            println!("Socket closed");
        } else {
            println!("Socket was already closed or not connected");
        }
    }

    #[napi]
    pub fn send(&self, message: Buffer) -> Result<Buffer> {
        if let Some(socket) = &self.socket {
            let msg = nng::Message::from(&message[..]);

            socket.send(msg).map_err(|(_, e)| {
                eprintln!("Failed to send message: {:?}", e);
                napi::Error::new(napi::Status::GenericFailure, format!("Send error: {:?}", e))
            })?;

            let response = socket.recv().map_err(|e| {
                match e {
                    NngError::TimedOut => {
                        napi::Error::new(napi::Status::GenericFailure, "Receive timeout".to_string())
                    }
                    _ => {
                        napi::Error::new(napi::Status::GenericFailure, format!("Receive error: {:?}", e))
                    }
                }
            })?;

            Ok(response.as_slice().into())
        } else {
            eprintln!("Socket not connected");
            Err(napi::Error::new(napi::Status::GenericFailure, "Socket not connected".to_string()))
        }
    }

    #[napi]
    pub fn recv(&self, callback: ThreadsafeFunction<Buffer>) -> Result<()> {
        let socket = self.socket.clone();
        let listening = Arc::clone(&self.listening);

        listening.store(true, Ordering::Relaxed);

        std::thread::spawn(move || {
            if let Some(socket) = socket {
                while listening.load(Ordering::Relaxed) {
                    match socket.recv() {
                        Ok(message) => {
                            let buffer = message.as_slice().into();
                            let _ = callback.call(Ok(buffer), ThreadsafeFunctionCallMode::NonBlocking);
                        },
                        Err(NngError::TimedOut) => {
                            eprintln!("Receive timed out.");
                        }
                        Err(e) => {
                            if listening.load(Ordering::Relaxed) { // 只有在未主动关闭时才打印错误
                                eprintln!("Error receiving message: {:?}", e);
                            }
                        }
                    }
                }
            } else {
                eprintln!("Socket is not connected.");
            }
        });
        Ok(())
    }

    #[napi]
    pub fn is_connect(&self) -> bool {
        self.socket.is_some()
    }
}

pub struct NngErrorWrapper(NngError);

impl From<NngErrorWrapper> for napi::Error {
    fn from(err: NngErrorWrapper) -> Self {
        napi::Error::new(napi::Status::GenericFailure, err.0.to_string())
    }
}

#[napi]
pub enum ProtocolType {
    Pair0,
    Pair1,
    Pub0,
    Sub0,
    Req0,
    Rep0,
    Surveyor0,
    Push0,
    Pull0,
    Bus0,
}

impl From<ProtocolType> for Protocol {
    fn from(protocol_type: ProtocolType) -> Self {
        match protocol_type {
            ProtocolType::Pair0 => Protocol::Pair0,
            ProtocolType::Pair1 => Protocol::Pair1,
            ProtocolType::Pub0 => Protocol::Pub0,
            ProtocolType::Sub0 => Protocol::Sub0,
            ProtocolType::Req0 => Protocol::Req0,
            ProtocolType::Rep0 => Protocol::Rep0,
            ProtocolType::Surveyor0 => Protocol::Surveyor0,
            ProtocolType::Push0 => Protocol::Push0,
            ProtocolType::Pull0 => Protocol::Pull0,
            ProtocolType::Bus0 => Protocol::Bus0,
        }
    }
}