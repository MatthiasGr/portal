use std::{borrow::Cow, error::Error, io, net::SocketAddr, str::FromStr};

use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
    task,
};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::instrument;

use crate::protocol::{
    PacketDecoder, PacketEncoder,
    handshake::{HandshakePacket, NextState},
    login, status,
};

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
) -> io::Result<()> {
    let mut status_sent = false;
    let mut ping_sent = false;
    while let Some(req) = reader.next().await
        && !ping_sent
    {
        // TODO: When up, just forward
        let req = req?;
        let resp = match *req {
            status::ServerBound::StatusRequest => {
                if status_sent {
                    tracing::debug!("Invalid status response");
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
) -> io::Result<()> {
    let mut disconnect_sent = false;
    while let Some(req) = reader.next().await
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
                login::ClientBound::Disconnect(Cow::Borrowed("\"Login is not implemented\""))
            }
        };
        writer.send(resp).await?;
    }

    writer.close().await?;
    Ok(())
}

#[instrument(skip_all)]
async fn connection_handler(mut socket: TcpStream, peer: &SocketAddr) -> io::Result<()> {
    let (read_half, write_half) = socket.split();

    let mut reader = FramedRead::new(read_half, PacketDecoder::<HandshakePacket<'_>>::new());
    // The FramedRead interface is not really ideal for single packets, but oh well
    let handshake_packet = reader
        .next()
        .await
        .ok_or(io::Error::from(io::ErrorKind::UnexpectedEof))
        .and_then(|r| r)?;

    tracing::info!(
        peer = display(peer),
        server = display(&handshake_packet.address),
        port = display(handshake_packet.port),
        next_state = display(handshake_packet.next_state),
        "Handling new connection from client"
    );

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
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    tracing_subscriber::fmt::init();

    // Preliminary command line handling, will be improved later
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        eprintln!("Usage: {} <listen address>", args[0]);
        return Err("invalid command line arguments".into());
    }

    let addr = SocketAddr::from_str(&args[1]).map_err(|_| "could not parse listen address")?;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(address = display(addr), "Accepting TCP connections");

    loop {
        let (socket, peer) = listener.accept().await?;
        task::spawn(async move {
            if let Err(err) = connection_handler(socket, &peer).await {
                tracing::error!(
                    error = display(err),
                    peer = display(peer),
                    "Error in connection handler"
                )
            }
        });
    }
}
