use std::{
    borrow::Cow,
    io::{self, Write},
    mem,
};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use tracing::warn;
use uuid::Uuid;

use crate::protocol::{
    Protocol,
    types::{read_string, string_size, write_string},
};

use super::DecoderState;

#[derive(Debug)]
pub struct LoginStart<'a> {
    pub name: Cow<'a, str>,
    pub uuid: Uuid,
}

#[derive(Debug)]
pub enum ServerBound<'a> {
    LoginStart(LoginStart<'a>),
}

impl<'a> Protocol<'a> for ServerBound<'a> {
    fn decode_packet(number: i32, src: &mut DecoderState<'a>) -> io::Result<Self> {
        match number {
            0 => {
                let name = read_string(src)?;
                let uuid = src.read_u128::<BigEndian>()?;
                Ok(ServerBound::LoginStart(LoginStart {
                    name: Cow::Borrowed(name),
                    uuid: Uuid::from_u128(uuid),
                }))
            }
            1..4 => {
                warn!(
                    "Tried to decode a valid but unsupported packet type {}",
                    number
                );
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "unsupported packet type",
                ))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid packet type",
            )),
        }
    }

    fn packet_number(&self) -> i32 {
        match self {
            ServerBound::LoginStart(_) => 0,
        }
    }

    fn encoded_size(&self) -> usize {
        match self {
            ServerBound::LoginStart(login_start) => {
                string_size(&login_start.name) + mem::size_of::<u128>()
            }
        }
    }

    fn encode_packet(&self, writer: &mut impl Write) -> io::Result<()> {
        match self {
            ServerBound::LoginStart(login_start) => {
                write_string(&login_start.name, writer)?;
                writer.write_u128::<BigEndian>(login_start.uuid.as_u128())?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ClientBound<'a> {
    Disconnect(Cow<'a, str>),
}

impl<'a> Protocol<'a> for ClientBound<'a> {
    fn decode_packet(number: i32, src: &mut DecoderState<'a>) -> io::Result<Self> {
        match number {
            0 => {
                let reason = read_string(src)?;
                Ok(ClientBound::Disconnect(Cow::Borrowed(reason)))
            }
            1..5 => {
                warn!(
                    "Tried to decode a valid but unsupported packet type {}",
                    number
                );
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "unsupported packet type",
                ))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid packet type",
            )),
        }
    }

    fn packet_number(&self) -> i32 {
        match self {
            ClientBound::Disconnect(_) => 0,
        }
    }

    fn encoded_size(&self) -> usize {
        match self {
            ClientBound::Disconnect(reason) => string_size(&reason),
        }
    }

    fn encode_packet(&self, writer: &mut impl io::Write) -> io::Result<()> {
        match self {
            ClientBound::Disconnect(reason) => {
                write_string(&reason, writer)?;
            }
        }
        Ok(())
    }
}
