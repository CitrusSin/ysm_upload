

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;

#[derive(Debug)]
pub enum RconError {
    AuthenticationFailed,
    NotAuthenticated,
    IoError(std::io::Error),
    InvalidPacket(String),
}

impl std::fmt::Display for RconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RconError::AuthenticationFailed => write!(f, "RCON authentication failed"),
            RconError::NotAuthenticated => write!(f, "RCON session is not authenticated"),
            RconError::IoError(e) => write!(f, "RCON IO error: {}", e),
            RconError::InvalidPacket(msg) => write!(f, "Invalid RCON packet: {}", msg),
        }
    }
}

impl std::error::Error for RconError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RconError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

type Result<T> = std::result::Result<T, RconError>;

pub struct RconPacket {
    _id: i32,
    _packet_type: i32,
    _payload: Vec<u8>
}

impl RconPacket {
    pub fn new(id: i32, packet_type: i32, payload: Vec<u8>) -> Result<Self> {
        if payload.len() > i32::MAX as usize {
            return Err(RconError::InvalidPacket("RconPacket payload is too large".to_string()));
        }
        Ok(RconPacket { _id: id, _packet_type: packet_type, _payload: payload })
    }

    pub fn auth_request(id: i32, password: &str) -> Result<Self> {
        RconPacket::new(id, 3, password.as_bytes().to_vec())
    }

    pub fn exec_command(id: i32, command: &str) -> Result<Self> {
        RconPacket::new(id, 2, command.as_bytes().to_vec())
    }

    pub fn id(&self) -> i32 {
        self._id
    }

    pub fn packet_type(&self) -> i32 {
        self._packet_type
    }

    pub fn payload(&self) -> &[u8] {
        &self._payload
    }

    pub async fn read_packet_async<T: AsyncRead + Unpin>(stream: &mut T) -> Result<Self> {
        let mut header_buf = [0u8; 12];
        
        stream.read_exact(&mut header_buf).await.map_err(|e| RconError::IoError(e))?;
        let payload_len = i32::from_le_bytes(
            header_buf[0..4]
                .try_into()
                .map_err(|e| RconError::InvalidPacket(format!("Invalid payload length bytes: {}", e)))?,
        );
        if payload_len < 0 {
            return Err(RconError::InvalidPacket("RconPacket payload length is negative".to_string()));
        }
        let payload_len = payload_len as usize;

        let id = i32::from_le_bytes(
            header_buf[4..8]
                .try_into()
                .map_err(|e| RconError::InvalidPacket(format!("Invalid packet id bytes: {}", e)))?,
        ) as i32;
        let packet_type = i32::from_le_bytes(
            header_buf[8..12]
                .try_into()
                .map_err(|e| RconError::InvalidPacket(format!("Invalid packet type bytes: {}", e)))?,
        ) as i32;
        let mut payload_buf = vec![0u8; payload_len + 2]; // Includes two trailing null bytes

        stream.read_exact(&mut payload_buf).await.map_err(|e| RconError::IoError(e))?;

        Ok(RconPacket { _id: id, _packet_type: packet_type, _payload: payload_buf[..payload_len].to_vec() })
    }
}

impl Into<Vec<u8>> for RconPacket {
    fn into(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        if self._payload.len() > i32::MAX as usize {
            panic!("RconPacket payload is too large");
        }
        let payload_len = self._payload.len() as i32;
        bytes.extend_from_slice(&payload_len.to_le_bytes());
        bytes.extend_from_slice(&self._id.to_le_bytes());
        bytes.extend_from_slice(&self._packet_type.to_le_bytes());
        bytes.extend_from_slice(&self._payload);
        bytes.push(0); // Null byte at the end of the payload
        bytes.push(0); // Null byte at the end of the packet
        bytes
    }
}


pub struct RconSession {
    next_id: i32,
    authenticated: bool,
    stream: tokio::net::TcpStream
}

impl RconSession {
    pub async fn connect(addr: impl ToSocketAddrs, password: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await.map_err(|e| RconError::IoError(e))?;
        let mut session = RconSession { next_id: 1, authenticated: false, stream };
        session.authenticate(password).await?;
        Ok(session)
    }

    async fn authenticate(&mut self, password: &str) -> Result<()> {
        let auth_packet = RconPacket::auth_request(self.next_id, password)?;
        self.next_id += 1;
        let auth_bytes: Vec<u8> = auth_packet.into();
        self.stream.write_all(&auth_bytes).await.map_err(|e| RconError::IoError(e))?;

        let response_packet = RconPacket::read_packet_async(&mut self.stream).await?;

        if response_packet.id() == -1 {
            return Err(RconError::AuthenticationFailed);
        }
        self.authenticated = true;
        Ok(())
    }

    pub async fn exec_command(&mut self, command: &str) -> Result<String> {
        if !self.authenticated {
            return Err(RconError::NotAuthenticated);
        }
        let cmd_packet = RconPacket::exec_command(self.next_id, command)?;
        self.next_id += 1;
        let cmd_bytes: Vec<u8> = cmd_packet.into();
        self.stream.write_all(&cmd_bytes).await.map_err(|e| RconError::IoError(e))?;
        
        let response_packet = RconPacket::read_packet_async(&mut self.stream).await?;
        
        Ok(String::from_utf8_lossy(response_packet.payload()).to_string())
    }
}
