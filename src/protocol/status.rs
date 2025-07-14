use std::{borrow::Cow, io, mem};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::protocol::{
    Protocol,
    types::{read_string, string_size, write_string},
};

use super::DecoderState;

#[derive(Debug)]
pub enum ServerBound {
    StatusRequest,
    PingRequest(i64),
}

impl Protocol<'_> for ServerBound {
    fn decode_packet(number: i32, src: &mut DecoderState<'_>) -> io::Result<Self> {
        match number {
            0 => Ok(ServerBound::StatusRequest),
            1 => Ok(ServerBound::PingRequest(src.read_i64::<BigEndian>()?)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "got an invalid packet number",
            )),
        }
    }

    fn packet_number(&self) -> i32 {
        match self {
            ServerBound::StatusRequest => 0,
            ServerBound::PingRequest(_) => 1,
        }
    }

    fn encoded_size(&self) -> usize {
        match self {
            ServerBound::StatusRequest => 0,
            ServerBound::PingRequest(_) => mem::size_of::<i64>(),
        }
    }

    fn encode_packet(&self, writer: &mut impl io::Write) -> io::Result<()> {
        match self {
            ServerBound::StatusRequest => {}
            ServerBound::PingRequest(ts) => writer.write_i64::<BigEndian>(*ts)?,
        };
        Ok(())
    }
}

#[derive(Debug)]
pub enum ClientBound<'a> {
    StatusResponse { json_response: Cow<'a, str> },
    PingResponse(i64),
}

impl<'a> Protocol<'a> for ClientBound<'a> {
    fn decode_packet(number: i32, src: &mut DecoderState<'a>) -> io::Result<Self> {
        match number {
            0 => Ok(ClientBound::StatusResponse {
                json_response: Cow::Borrowed(read_string(src)?),
            }),
            1 => Ok(ClientBound::PingResponse(src.read_i64::<BigEndian>()?)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "got an invalid packet number",
            )),
        }
    }

    fn packet_number(&self) -> i32 {
        match self {
            ClientBound::StatusResponse { .. } => 0,
            ClientBound::PingResponse(_) => 1,
        }
    }

    fn encoded_size(&self) -> usize {
        match self {
            ClientBound::StatusResponse { json_response } => string_size(&json_response),
            ClientBound::PingResponse(_) => mem::size_of::<i64>(),
        }
    }

    fn encode_packet(&self, writer: &mut impl io::Write) -> io::Result<()> {
        match self {
            ClientBound::StatusResponse { json_response } => write_string(&json_response, writer)?,
            ClientBound::PingResponse(ts) => writer.write_i64::<BigEndian>(*ts)?,
        }
        Ok(())
    }
}
