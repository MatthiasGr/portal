use std::{
    io::{self, Read, Write},
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
};

use tokio_util::{
    bytes::{Bytes, BytesMut},
    codec::{Decoder, Encoder},
};

use crate::protocol::types::{read_var_int, var_int_size, write_var_int};

pub mod types;

pub mod handshake;
pub mod login;
pub mod status;

pub trait Protocol<'a>: Sized {
    fn decode_packet(number: i32, src: &mut DecoderState<'a>) -> io::Result<Self>;
    fn packet_number(&self) -> i32;
    fn encoded_size(&self) -> usize;
    fn encode_packet(&self, writer: &mut impl Write) -> io::Result<()>;
}

/// A container for any packet type.
/// The container keeps a reference to the underlying buffer for zero-copy packets, so it should not
/// be longe lived.
pub struct Packet<T> {
    data: T,
    bytes: Bytes,
}

impl<T> Packet<T> {
    pub fn buffer(&self) -> Bytes {
        self.bytes.clone()
    }
}

impl<T> Deref for Packet<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for Packet<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

pub struct DecoderState<'a> {
    buffer: &'a [u8],
    offset: usize,
}

impl<'a> DecoderState<'a> {
    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8], io::Error> {
        let start = self.offset;
        let end = self.offset + count;

        if self.buffer.len() < end {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }

        self.offset += count;
        Ok(&self.buffer[start..end])
    }
}

impl<'a> Read for DecoderState<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = usize::min(buf.len(), self.buffer.len() - self.offset);
        if len == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }

        let bytes = self.bytes(len).unwrap();
        buf.copy_from_slice(bytes);
        Ok(len)
    }
}

#[derive(Debug)]
pub struct PacketDecoder<T> {
    needed: Option<usize>,
    _phantom: PhantomData<T>,
}

impl<T> PacketDecoder<T> {
    pub fn new() -> PacketDecoder<T> {
        PacketDecoder {
            needed: None,
            _phantom: PhantomData,
        }
    }
}

impl<'a, T> Decoder for PacketDecoder<T>
where
    T: Protocol<'a>,
{
    type Item = Packet<T>;
    // TODO: Make a custom error here?
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(n) = self.needed
            && src.len() < n
        {
            return Ok(None);
        }

        // SAFETY: This transmute is used to convert the (unknown) lifetime of src to the lifetime
        // we expect fo all of our references in the packet, which we return.
        // By tapping the packet in the Packet struct alongside an owning buffer to that memory,
        // this is safe, even if rust does not agree due to the unsafe in the bytes crate.
        let mut state: DecoderState<'a> = DecoderState::<'a> {
            buffer: unsafe { mem::transmute(&src[..]) },
            offset: 0,
        };

        let raw_len = match read_var_int(&mut state) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        };
        if raw_len < 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        let len = raw_len as usize;

        if len + state.offset > src.len() {
            self.needed = Some(len);
            return Ok(None);
        }

        // Hack to shorten the buffer length available to decoder
        // It would be better to have some form of reslice function to do that in a defined way
        state.buffer = &state.buffer[..state.offset + len];

        let kind = match read_var_int(&mut state) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        };

        // We don't convert the EOF error here since we don't expect an EOF in a valid packet here.
        let packet = T::decode_packet(kind, &mut state)?;

        // By splitting the buffer here, we ensure that any pointer into the packet buffer should be
        // remain valid even if the byte buffer is grown at some point.
        // To ensure that the pointers remain valid, we wrap the packet in a Packet object, which
        // keeps the byte object alive while allowing access to the inner types.
        self.needed = None;
        Ok(Some(Packet {
            data: packet,
            bytes: src.split_to(state.offset).freeze(),
        }))
    }
}

pub struct EncoderState<'a> {
    bytes: &'a mut BytesMut,
}

impl<'a> Write for EncoderState<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // No-op
        Ok(())
    }
}

#[derive(Debug)]
pub struct PacketEncoder<T> {
    _phantom: PhantomData<T>,
}

impl<T> PacketEncoder<T> {
    pub fn new() -> PacketEncoder<T> {
        PacketEncoder {
            _phantom: PhantomData,
        }
    }
}

impl<'a, T> Encoder<T> for PacketEncoder<T>
where
    T: Protocol<'a>,
{
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let id = item.packet_number();
        let size = item.encoded_size();

        let total_size = size + var_int_size(id);
        assert!(total_size < i32::MAX as usize);
        dst.reserve(var_int_size(total_size as i32) + total_size);

        let mut state = EncoderState { bytes: dst };
        write_var_int(total_size as i32, &mut state)?;

        let start_len = state.bytes.len();
        write_var_int(id, &mut state)?;
        item.encode_packet(&mut state)?;

        assert!(
            dst.len() - start_len == total_size,
            "Packet size mismatch, expected: {}, actual: {}",
            size,
            dst.len() - start_len
        );
        Ok(())
    }
}
