use byteorder::{ReadBytesExt, WriteBytesExt};
use std::{
    io::{self, Read, Write},
    mem,
};

use crate::protocol::DecoderState;

pub fn read_var_int(src: &mut impl Read) -> io::Result<i32> {
    let mut value = 0;
    for i in 0..5 {
        let byte = src.read_u8()? as i32;
        value |= (byte & 0x7f) << (i * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "variant takes up too many bytes",
    ))
}

// Return the size of a var int when encoded
pub fn var_int_size(int: i32) -> usize {
    let bits = mem::size_of::<i32>() * 8 - int.leading_zeros() as usize;
    usize::max((bits + 6) / 7, 1)
}

pub fn write_var_int(mut int: i32, dest: &mut impl Write) -> io::Result<()> {
    loop {
        let byte = (int & 0x7f) as u8;
        int >>= 7;
        if int != 0 {
            dest.write_u8(byte | 0x80)?;
        } else {
            dest.write_u8(byte)?;
            break;
        }
    }
    Ok(())
}

pub fn read_string<'a>(src: &mut DecoderState<'a>) -> io::Result<&'a str> {
    let len = read_var_int(src)?;
    if len < 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid length"));
    }

    let bytes = src.bytes(len as usize)?;
    let result = str::from_utf8(&bytes[..len as usize])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "string is not valid UTF-8"))?;

    Ok(result)
}

pub fn string_size(string: &str) -> usize {
    assert!(string.len() < i32::MAX as usize);
    return var_int_size(string.len() as i32) + string.len();
}

pub fn write_string(string: &str, dest: &mut impl Write) -> io::Result<()> {
    assert!(string.len() < i32::MAX as usize);
    write_var_int(string.len() as i32, dest)?;
    dest.write_all(string.as_bytes())?;
    Ok(())
}
