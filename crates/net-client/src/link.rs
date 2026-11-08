//! Framed packet reader and writer over a `TcpStream` (compression, no encryption).

use ferrocraft_protocol::frame::{FrameDecoder, FrameEncoder};
use ferrocraft_protocol::{Error, PacketSet};
use std::io::{self, Read, Write};
use std::net::TcpStream;

pub struct Reader {
    stream: TcpStream,
    decoder: FrameDecoder,
}

impl Reader {
    pub fn new(stream: TcpStream) -> Reader {
        Reader {
            stream,
            decoder: FrameDecoder::default(),
        }
    }

    pub fn set_compression(&mut self, threshold: i32) {
        self.decoder.set_compression(threshold);
    }

    /// Next packet; an id that `P` does not know is an error.
    pub fn recv<P: PacketSet>(&mut self) -> io::Result<P> {
        let frame = self.frame()?;
        P::decode(&frame).map_err(to_io)
    }

    /// Like `recv`, but a packet id missing from `P` gives `None`.
    pub fn recv_or_skip<P: PacketSet>(&mut self) -> io::Result<Option<P>> {
        let frame = self.frame()?;
        match P::decode(&frame) {
            Ok(p) => Ok(Some(p)),
            Err(Error::UnknownPacket(_)) => Ok(None),
            Err(e) => Err(to_io(e)),
        }
    }

    fn frame(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = [0u8; 16384];
        loop {
            if let Some(f) = self.decoder.next_frame().map_err(to_io)? {
                return Ok(f);
            }
            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            self.decoder.feed(&buf[..n]);
        }
    }
}

pub struct Writer {
    stream: TcpStream,
    encoder: FrameEncoder,
}

impl Writer {
    pub fn new(stream: TcpStream) -> Writer {
        Writer {
            stream,
            encoder: FrameEncoder::default(),
        }
    }

    pub fn set_compression(&mut self, threshold: i32) {
        self.encoder.set_compression(threshold);
    }

    pub fn send<P: PacketSet>(&mut self, packet: &P) -> io::Result<()> {
        let mut out = Vec::new();
        self.encoder
            .encode(&packet.encode(), &mut out)
            .map_err(to_io)?;
        self.stream.write_all(&out)
    }
}

fn to_io(e: Error) -> io::Error {
    match e {
        Error::Io(e) => e,
        e => io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
    }
}
