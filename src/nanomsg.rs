use napi::bindgen_prelude::Buffer;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use nng::{Socket, Protocol, Error as NngError};
use napi_derive::napi;
use std::{thread};

#[napi]
pub struct SocketWrapper {
    socket: Option<Socket>,
}

#[napi]
impl SocketWrapper {
    #[napi(constructor)]
    pub fn new() -> Self {
        SocketWrapper { socket: None }
    }

    #[napi]
    pub fn connect(&mut self, protocol: ProtocolType, url: String) -> Result<(), napi::Error> {
        let socket = Socket::new(protocol.into()).map_err(|err| napi::Error::from(NngErrorWrapper(err)))?;
        socket.dial(&url).map_err(|err| napi::Error::from(NngErrorWrapper(err)))?;
        self.socket = Some(socket);
        Ok(())
    }

    #[napi]
    pub fn send(&self, message: Buffer) -> Result<Buffer, napi::Error> {
        if let Some(socket) = &self.socket {
            let msg = nng::Message::from(&message[..]);
            socket.send(msg).map_err(|(_, e)| napi::Error::from(NngErrorWrapper(e)))?;
            let response = socket.recv().map_err(|e| napi::Error::from(NngErrorWrapper(e)))?;
            Ok(response.as_slice().into())
        } else {
            Err(napi::Error::new(napi::Status::GenericFailure, "Socket not connected".to_string()))
        }
    }

    #[napi]
    pub fn recv(&self, callback: ThreadsafeFunction<Buffer>) -> Result<(), napi::Error> {
        if let Some(socket) = self.socket.clone() {
            thread::spawn(move || {
                loop {
                    match socket.recv() {
                        Ok(message) => {
                            let buf: Buffer = message.as_slice().into();
                            callback.call(Ok(buf), ThreadsafeFunctionCallMode::Blocking);
                        }
                        Err(err) => {
                            if err == nng::Error::Closed {
                                // 处理连接断开的情况
                                let napi_err = napi::Error::new(napi::Status::GenericFailure, "Connection closed".to_string());
                                callback.call(Err(napi_err), ThreadsafeFunctionCallMode::Blocking);
                                break;
                            } else {
                                // 处理其他错误
                                let napi_err = napi::Error::from(NngErrorWrapper(err));
                                callback.call(Err(napi_err), ThreadsafeFunctionCallMode::Blocking);
                            }
                        }
                    }
                }
            });
            Ok(())
        } else {
            Err(napi::Error::new(napi::Status::GenericFailure, "Socket not connected".to_string()))
        }
    }

    #[napi]
    pub fn close(&mut self) {
        if let Some(socket) = self.socket.take() {
            socket.close();
        }
    }

    #[napi]
    pub fn is_connect(&mut self) -> bool {
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