use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
    io::{self},
    mem,
};

use crate::protocol::{
    DecoderState, Protocol,
    types::{read_string, read_var_int, string_size, var_int_size, write_string, write_var_int},
};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

#[derive(Debug, Clone, Copy)]
pub enum NextState {
    Status,
    Login,
    Transfer,
}

impl Display for NextState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            NextState::Status => write!(f, "status"),
            NextState::Login => write!(f, "login"),
            NextState::Transfer => write!(f, "transfer"),
        }
    }
}

#[derive(Debug)]
pub struct HandshakePacket<'a> {
    pub version: i32,
    pub address: Cow<'a, str>,
    pub port: u16,
    pub next_state: NextState,
}

impl<'a> Protocol<'a> for HandshakePacket<'a> {
    fn decode_packet(number: i32, src: &mut DecoderState<'a>) -> io::Result<Self> {
        if number != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid packet number",
            ));
        }

        let version = read_var_int(src)?;
        let address = read_string(src)?;
        let port = src.read_u16::<BigEndian>()?;
        let next_state = match read_var_int(src)? {
            1 => NextState::Status,
            2 => NextState::Login,
            3 => NextState::Transfer,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown next state identifier",
                ));
            }
        };

        Ok(HandshakePacket {
            version: version,
            address: Cow::Borrowed(address),
            port: port,
            next_state: next_state,
        })
    }

    fn packet_number(&self) -> i32 {
        0
    }

    fn encoded_size(&self) -> usize {
        var_int_size(self.version) + string_size(&self.address) + mem::size_of::<u16>() + 1
    }

    fn encode_packet(&self, writer: &mut impl io::Write) -> io::Result<()> {
        write_var_int(self.version, writer)?;
        write_string(&self.address, writer)?;
        writer.write_u16::<BigEndian>(self.port)?;
        write_var_int(
            match self.next_state {
                NextState::Status => 1,
                NextState::Login => 2,
                NextState::Transfer => 3,
            },
            writer,
        )?;
        Ok(())
    }
}
