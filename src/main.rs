use std::{borrow::Cow, net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use futures::{SinkExt, StreamExt};
use tokio::{
    io::{self, AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task,
    time::timeout,
};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::instrument;

use crate::{
    error::Error,
    external_process::ExternalProcess,
    protocol::{
        PacketDecoder, PacketEncoder,
        handshake::{HandshakePacket, NextState},
        login, status,
    },
};

mod error;
mod external_process;
mod protocol;

const STATUS_RESPONSE: &'static str = r#"{
    "version": {
        "name": "1.21.7",
        "protocol": 772
    },
    "players": {
        "max": 0,
        "online": 0
    },
    "description": "Not a Minecraft server",
    "enforceSecureProfile": false
}"#;

#[instrument(skip_all)]
async fn status_handler<Read: AsyncRead + Unpin, Write: AsyncWrite + Unpin>(
    mut reader: FramedRead<Read, PacketDecoder<status::ServerBound>>,
    mut writer: FramedWrite<Write, PacketEncoder<status::ClientBound<'_>>>,
) -> Result<(), Error> {
    let mut status_sent = false;
    let mut ping_sent = false;
    while let Some(req) = timeout(Duration::from_secs(5), reader.next()).await?
        && !ping_sent
    {
        // TODO: When up, just forward
        let req = req?;
        let resp = match *req {
            status::ServerBound::StatusRequest => {
                if status_sent {
                    tracing::debug!("Connection sent more than one status request");
                    break;
                }
                status_sent = true;
                status::ClientBound::StatusResponse {
                    json_response: Cow::Borrowed(STATUS_RESPONSE),
                }
            }
            status::ServerBound::PingRequest(timestamp) => {
                ping_sent = true;
                status::ClientBound::PingResponse(timestamp)
            }
        };
        writer.send(resp).await?;
    }

    writer.close().await?;
    Ok(())
}

#[instrument(skip_all)]
async fn login_handler<Read: AsyncRead + Unpin, Write: AsyncWrite + Unpin>(
    mut reader: FramedRead<Read, PacketDecoder<login::ServerBound<'_>>>,
    mut writer: FramedWrite<Write, PacketEncoder<login::ClientBound<'_>>>,
) -> Result<(), Error> {
    let mut disconnect_sent = false;
    while let Some(req) = timeout(Duration::from_secs(5), reader.next()).await?
        && !disconnect_sent
    {
        let req = req?;
        let resp = match *req {
            login::ServerBound::LoginStart(ref login_start) => {
                tracing::info!(
                    name = display(&login_start.name),
                    uuid = display(login_start.uuid),
                    "Player connected"
                );
                disconnect_sent = true;
                login::ClientBound::Disconnect(Cow::Borrowed(
                    "\"Server is starting, please try again later\"",
                ))
            }
        };
        writer.send(resp).await?;
    }

    writer.close().await?;
    Ok(())
}

#[instrument(skip_all)]
async fn connection_handler(
    mut socket: TcpStream,
    peer: &SocketAddr,
    forward_addr: &SocketAddr,
    start_command: Arc<ExternalProcess>,
) -> Result<(), Error> {
    let (read_half, write_half) = socket.split();

    let mut reader = FramedRead::new(read_half, PacketDecoder::<HandshakePacket<'_>>::new());
    // The FramedRead interface is not really ideal for single packets, but oh well
    let handshake_packet = timeout(Duration::from_secs(5), reader.next())
        .await?
        .ok_or(io::Error::from(io::ErrorKind::UnexpectedEof))
        .and_then(|r| r)?;

    tracing::info!(
        peer = %peer,
        server = %&handshake_packet.address,
        port = %handshake_packet.port,
        next_state = %handshake_packet.next_state,
        "Handling new connection from client"
    );

    // TODO: At this point, we should look at the actual server location
    if let Ok(mut forward) = TcpStream::connect(forward_addr).await {
        tracing::debug!(peer = %peer, forward = %forward_addr, "Successfully connected to backend");
        forward.write_all(&handshake_packet.buffer()).await?;
        drop(handshake_packet);

        io::copy_bidirectional(&mut socket, &mut forward).await?;
        return Ok(());
    }

    tracing::debug!(peer = %peer, backend = %forward_addr, "Forward is down, running start command");
    start_command.spawn_once().await?;

    // We drop the handshake packet as soon as possible to reclaim space in the receive buffer
    let next_state = handshake_packet.next_state;
    drop(handshake_packet);

    match next_state {
        NextState::Status => {
            status_handler(
                reader.map_decoder(|_| PacketDecoder::new()),
                FramedWrite::new(write_half, PacketEncoder::new()),
            )
            .await?
        }
        NextState::Login | NextState::Transfer => {
            login_handler(
                reader.map_decoder(|_| PacketDecoder::new()),
                FramedWrite::new(write_half, PacketEncoder::new()),
            )
            .await?
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    // Preliminary command line handling, will be improved later
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        eprintln!(
            "Usage: {} <listen address> <forward address> <start command>",
            args[0]
        );
        return Err("invalid command line arguments".into());
    }

    let listen_addr =
        SocketAddr::from_str(&args[1]).map_err(|_| "could not parse listen address")?;
    let forward_addr =
        SocketAddr::from_str(&args[2]).map_err(|_| "could not parse forward address")?;
    let start_command = Arc::new(ExternalProcess::new(args[3].clone()));

    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!(address = %listen_addr, "Accepting TCP connections");

    loop {
        let (socket, peer) = listener.accept().await?;
        let cmd = Arc::clone(&start_command);
        task::spawn(async move {
            if let Err(err) = connection_handler(socket, &peer, &forward_addr, cmd).await {
                tracing::error!(error = %err, peer = %peer, "Error in connection handler")
            }
        });
    }
}
