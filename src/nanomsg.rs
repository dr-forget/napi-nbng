use napi::{
    bindgen_prelude::*,
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use nng::{Socket, Protocol, Error as NngError};
use nng::options::Options;
use napi_derive::napi;
use core::time::Duration;

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
    pub fn connect(
        &mut self,
        protocol: ProtocolType,
        url: String,
        recv_timeout: u32,  // 修改为 u32
        send_timeout: u32,
    ) -> Result<bool> {
        let socket = Socket::new(protocol.into()).map_err(|err| {
            napi::Error::new(napi::Status::GenericFailure, format!("Socket creation failed: {:?}", err))
        })?;
    
        // 这里处理接收超时和发送超时
        let recv_timeout_duration = if recv_timeout == 0 {
            None // 无限超时
        } else {
            Some(Duration::from_millis(recv_timeout as u64))
        };
        
        let send_timeout_duration = if send_timeout == 0 {
            None // 无限超时
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
        Ok(true) // 返回连接成功
    }

    #[napi]
    pub fn send(&self, message: Buffer) -> Result<Buffer> {
        if let Some(socket) = &self.socket {
            let msg = nng::Message::from(&message[..]);

            // 添加发送超时错误处理
            socket.send(msg).map_err(|(_, e)| {
                eprintln!("Failed to send message: {:?}", e);
                napi::Error::new(napi::Status::GenericFailure, format!("Send error: {:?}", e))
            })?;
            
            // 添加接收超时错误处理
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
        let socket = self.socket.clone(); // Clone socket to move into thread

        std::thread::spawn(move || {
            if let Some(socket) = socket {
                loop {
                    match socket.recv() {
                        Ok(message) => {
                            let buffer = message.as_slice().into();
                            let _ = callback.call(Ok(buffer), ThreadsafeFunctionCallMode::NonBlocking);
                        },
                        Err(NngError::TimedOut) => {
                            eprintln!("Receive timed out.");
                            // 你可以选择在这里触发回调或采取其他操作
                        }
                        Err(e) => {
                            eprintln!("Error receiving message: {:?}", e);
                        }
                    }
                    // 可选：你可以在这里加一些 sleep，以防止 CPU 使用过高
                }
            } else {
                eprintln!("Socket is not connected.");
            }
        });
        Ok(())
    }

    #[napi]
    pub fn close(&mut self) {
        if let Some(socket) = self.socket.take() {
            let _ = socket.close();
            println!("Socket closed");
        } else {
            println!("Socket was already closed or not connected");
        }
    }

    #[napi]
    pub fn is_connect(&self) -> bool {
        self.socket.is_some() // 如果 socket 是 Some，则表示连接成功
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