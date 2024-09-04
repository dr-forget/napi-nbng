use napi::{
    bindgen_prelude::*,
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use nng::{Socket, Protocol, Error as NngError};
use napi_derive::napi;

#[napi]
pub struct SocketWrapper {
    socket: Option<Socket>,
}

#[napi]
impl SocketWrapper {
    #[napi(constructor)]
    pub fn new() -> Self {
        println!("SocketWrapper created");
        SocketWrapper { socket: None }
    }

    #[napi]
    pub fn connect(&mut self, protocol: ProtocolType, url: String) -> Result<()> {
        let socket = Socket::new(protocol.into())
            .map_err(|err| napi::Error::from(NngErrorWrapper(err)))?;
        socket.dial(&url)
            .map_err(|err| napi::Error::from(NngErrorWrapper(err)))?;
        self.socket = Some(socket);
        println!("Connected to {}", url);
        Ok(())
    }

    #[napi]
    pub fn send(&self, message: Buffer) -> Result<Buffer> {
        if let Some(socket) = &self.socket {
            let msg = nng::Message::from(&message[..]);
            socket.send(msg).map_err(|(_, e)| {
                eprintln!("Failed to send message: {:?}", e);
                napi::Error::from(NngErrorWrapper(e))
            })?;
            println!("Message sent, waiting for response...");
            let response = socket.recv().map_err(|e| {
                eprintln!("Failed to receive response: {:?}", e);
                napi::Error::from(NngErrorWrapper(e))
            })?;
            println!("Response received: {:?}", response);
            Ok(response.as_slice().into())
        } else {
            eprintln!("Socket not connected");
            Err(napi::Error::new(napi::Status::GenericFailure, "Socket not connected".to_string()))
        }
    }

    #[napi]
    pub fn recv(&self, callback: ThreadsafeFunction<Buffer>) -> Result<()> {
        let socket = self.socket.clone(); // Clone socket to move into thread
    
        // 启动一个线程来接收消息
        std::thread::spawn(move || {
            if let Some(socket) = socket { // 解包 Option<Socket>
                println!("Started receiving messages");
                loop {
                    // 调用 recv 方法
                    match socket.recv() {
                        Ok(message) => {
                            // 将消息转换为 JsBuffer
                            let buffer = message.as_slice().into();
    
                            // 调用回调函数，将消息发送到 Node.js
                            // 调用回调函数，将消息发送到 Node.js
                           let _ = callback.call(Ok(buffer), ThreadsafeFunctionCallMode::NonBlocking);
                        },
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
            println!("Closing socket");
            let _ = socket.close();
            println!("Socket closed");
        } else {
            println!("Socket was already closed or not connected");
        }
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