

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tokio::time::{Duration, timeout};


const SAFE_TX_MAX_PAYLOAD_SIZE: i32 = 1300; // Make buggy Minecraft RCON implementation happy
const SAFE_RX_MAX_PACKET_SIZE: i32 = 64 * 1024;
const RCON_PACKET_OVERHEAD_SIZE: usize = 10;
const SERVER_RESPONSE_CHUNK_SIZE: usize = 4096;
const RESPONSE_IDLE_TIMEOUT: Duration = Duration::from_millis(150);
const PROBE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_END_PROBE_COMMAND: &str = "/time query gametime";

#[derive(Debug)]
pub enum RconError {
    AuthenticationFailed,
    NotAuthenticated,
    CommandExecutionError(String),
    IoError(std::io::Error),
    InvalidPacket(String),
}

impl std::fmt::Display for RconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RconError::AuthenticationFailed => write!(f, "RCON authentication failed"),
            RconError::NotAuthenticated => write!(f, "RCON session is not authenticated"),
            RconError::CommandExecutionError(msg) => write!(f, "RCON command execution error: {}", msg),
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
        if payload.len() > SAFE_TX_MAX_PAYLOAD_SIZE as usize {
            return Err(RconError::InvalidPacket(format!("RconPacket payload is too large ({} bytes), maximum allowed is {}", payload.len(), SAFE_TX_MAX_PAYLOAD_SIZE)));
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
        let mut size_buf = [0u8; 4];
        
        stream.read_exact(&mut size_buf).await.map_err(RconError::IoError)?;

        let packet_size = i32::from_le_bytes(size_buf);
        if packet_size < RCON_PACKET_OVERHEAD_SIZE as i32 {
            return Err(RconError::InvalidPacket(format!(
                "RconPacket size is too small ({packet_size} bytes), minimum allowed is {RCON_PACKET_OVERHEAD_SIZE}"
            )));
        }
        if packet_size > SAFE_RX_MAX_PACKET_SIZE {
            return Err(RconError::InvalidPacket(format!("RconPacket payload is too large ({} bytes), maximum allowed is {}", packet_size, SAFE_RX_MAX_PACKET_SIZE)));
        }

        let mut packet_buf = vec![0u8; packet_size as usize];
        stream.read_exact(&mut packet_buf).await.map_err(RconError::IoError)?;

        let id = i32::from_le_bytes(
            packet_buf[0..4]
                .try_into()
                .map_err(|e| RconError::InvalidPacket(format!("Invalid packet id bytes: {}", e)))?,
        );
        let packet_type = i32::from_le_bytes(
            packet_buf[4..8]
                .try_into()
                .map_err(|e| RconError::InvalidPacket(format!("Invalid packet type bytes: {}", e)))?,
        );

        if packet_buf[packet_buf.len() - 2] != 0 || packet_buf[packet_buf.len() - 1] != 0 {
            return Err(RconError::InvalidPacket("RconPacket payload length is negative".to_string()));
        }

        let payload = packet_buf[8..packet_buf.len() - 2].to_vec();

        Ok(RconPacket { _id: id, _packet_type: packet_type, _payload: payload })
    }
}

impl Into<Vec<u8>> for RconPacket {
    fn into(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        if self._payload.len() > SAFE_TX_MAX_PAYLOAD_SIZE as usize {
            panic!("RconPacket payload is too large ({} bytes), maximum allowed is {}", self._payload.len(), SAFE_TX_MAX_PAYLOAD_SIZE);
        }
        let packet_size = (self._payload.len() + RCON_PACKET_OVERHEAD_SIZE) as i32;
        bytes.extend_from_slice(&packet_size.to_le_bytes());
        bytes.extend_from_slice(&self._id.to_le_bytes());
        bytes.extend_from_slice(&self._packet_type.to_le_bytes());
        bytes.extend_from_slice(&self._payload);
        bytes.push(0); // Null byte at the end of the payload
        bytes.push(0); // Null byte at the end of the packet
        bytes
    }
}

async fn write_packet_async<T: AsyncWrite + Unpin>(stream: &mut T, packet: RconPacket) -> Result<()> {
    let packet_bytes: Vec<u8> = packet.into();
    stream.write_all(&packet_bytes).await.map_err(RconError::IoError)
}

async fn read_packet_with_timeout<T: AsyncRead + Unpin>(
    stream: &mut T,
    duration: Duration,
) -> Result<RconPacket> {
    match timeout(duration, RconPacket::read_packet_async(stream)).await {
        Ok(result) => result,
        Err(_) => Err(RconError::IoError(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Timed out waiting for RCON packet",
        ))),
    }
}

async fn exec_command_on_stream<T: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut T,
    next_id: &mut i32,
    command: &str,
) -> Result<String> {
    let command_id = *next_id;
    *next_id = i32::wrapping_add(*next_id, 1);
    write_packet_async(stream, RconPacket::exec_command(command_id, command)?).await?;

    let mut response_text = String::new();
    let mut probe_id = None;
    let mut has_received_response = false;

    loop {
        let packet = match probe_id {
            Some(_) => read_packet_with_timeout(stream, PROBE_RESPONSE_TIMEOUT).await?,
            None if !has_received_response => RconPacket::read_packet_async(stream).await?,
            None => match read_packet_with_timeout(stream, RESPONSE_IDLE_TIMEOUT).await {
                Ok(packet) => packet,
                Err(RconError::IoError(err)) if err.kind() == std::io::ErrorKind::TimedOut => {
                    let id = *next_id;
                    *next_id = i32::wrapping_add(*next_id, 1);
                    write_packet_async(stream, RconPacket::exec_command(id, RESPONSE_END_PROBE_COMMAND)?).await?;
                    probe_id = Some(id);
                    continue;
                }
                Err(err) => return Err(err),
            },
        };

        if packet.id() == command_id {
            let chunk = String::from_utf8_lossy(packet.payload()).to_string();
            has_received_response = true;
            response_text.push_str(&chunk);

            if probe_id.is_none() && packet.payload().len() < SERVER_RESPONSE_CHUNK_SIZE {
                break;
            }

            continue;
        }

        if probe_id == Some(packet.id()) {
            break;
        }
    }

    if response_text.starts_with("Error executing") {
        Err(RconError::CommandExecutionError(response_text))
    } else {
        Ok(response_text)
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
        stream.set_nodelay(true).map_err(|e| RconError::IoError(e))?;
        let mut session = RconSession { next_id: 1, authenticated: false, stream };
        session.authenticate(password).await?;
        Ok(session)
    }

    async fn authenticate(&mut self, password: &str) -> Result<()> {
        let auth_id = self.next_id;
        self.next_id = i32::wrapping_add(self.next_id, 1);
        write_packet_async(&mut self.stream, RconPacket::auth_request(auth_id, password)?).await?;

        loop {
            let response_packet = RconPacket::read_packet_async(&mut self.stream).await?;

            if response_packet.id() == -1 {
                return Err(RconError::AuthenticationFailed);
            }

            if response_packet.id() == auth_id && response_packet.packet_type() == 2 {
                self.authenticated = true;
                return Ok(());
            }
        }
    }

    pub async fn exec_command(&mut self, command: &str) -> Result<String> {
        if !self.authenticated {
            return Err(RconError::NotAuthenticated);
        }
        exec_command_on_stream(&mut self.stream, &mut self.next_id, command).await
    }
}
