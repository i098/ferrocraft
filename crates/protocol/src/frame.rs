//! Packet framing: length prefix, optional zlib compression, optional AES-128/CFB8.

use crate::codec::{Error, PacketSet, Result, VarInt, Wire};
use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use aes::Aes128;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

/// Vanilla limit for one packet, compressed or not.
pub const MAX_PACKET: usize = 1 << 21;

/// AES-128 in CFB8 mode; the shared secret is both key and IV.
pub struct Cfb8 {
    aes: Aes128,
    iv: [u8; 16],
}

impl Cfb8 {
    pub fn new(secret: &[u8; 16]) -> Self {
        Cfb8 {
            aes: Aes128::new(GenericArray::from_slice(secret)),
            iv: *secret,
        }
    }

    fn step(&mut self, input: u8, is_cipher: bool) -> u8 {
        let mut block = GenericArray::clone_from_slice(&self.iv);
        self.aes.encrypt_block(&mut block);
        let out = input ^ block[0];
        self.iv.copy_within(1.., 0);
        self.iv[15] = if is_cipher { input } else { out };
        out
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        data.iter_mut().for_each(|b| *b = self.step(*b, false));
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        data.iter_mut().for_each(|b| *b = self.step(*b, true));
    }
}

#[derive(Default)]
pub struct FrameEncoder {
    threshold: Option<usize>,
    cipher: Option<Cfb8>,
}

impl FrameEncoder {
    /// A negative threshold turns compression off.
    pub fn set_compression(&mut self, threshold: i32) {
        self.threshold = usize::try_from(threshold).ok();
    }

    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.cipher = Some(Cfb8::new(secret));
    }

    /// Appends one frame holding `packet` (packet id plus body) to `out`.
    pub fn encode(&mut self, packet: &[u8], out: &mut Vec<u8>) -> Result<()> {
        let start = out.len();
        let mut inner = Vec::new();
        let payload: &[u8] = match self.threshold {
            None => packet,
            Some(t) if packet.len() >= t => {
                VarInt(packet.len() as i32).write(&mut inner);
                let mut z = ZlibEncoder::new(inner, Compression::default());
                z.write_all(packet)?;
                inner = z.finish()?;
                &inner
            }
            Some(_) => {
                inner.push(0);
                inner.extend_from_slice(packet);
                &inner
            }
        };
        if payload.len() >= MAX_PACKET {
            return Err(Error::FrameTooLarge(payload.len()));
        }
        VarInt(payload.len() as i32).write(out);
        out.extend_from_slice(payload);
        if let Some(c) = &mut self.cipher {
            c.encrypt(&mut out[start..]);
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct FrameDecoder {
    threshold: Option<usize>,
    cipher: Option<Cfb8>,
    buf: Vec<u8>,
}

impl FrameDecoder {
    /// A negative threshold turns compression off. Applies to frames not yet extracted.
    pub fn set_compression(&mut self, threshold: i32) {
        self.threshold = usize::try_from(threshold).ok();
    }

    /// Decrypts bytes fed after this call; bytes already fed stay as they are.
    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.cipher = Some(Cfb8::new(secret));
    }

    /// Adds bytes received from the network.
    pub fn feed(&mut self, data: &[u8]) {
        let start = self.buf.len();
        self.buf.extend_from_slice(data);
        if let Some(c) = &mut self.cipher {
            c.decrypt(&mut self.buf[start..]);
        }
    }

    /// Returns the next whole packet (id plus body), or `None` if more bytes are needed.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        // The frame length is at most 3 bytes, as in vanilla.
        let mut len = 0usize;
        let mut head = 0;
        loop {
            let Some(&b) = self.buf.get(head) else {
                return Ok(None);
            };
            len |= usize::from(b & 0x7F) << (7 * head);
            head += 1;
            if b & 0x80 == 0 {
                break;
            }
            if head == 3 {
                return Err(Error::VarIntTooLong);
            }
        }
        let end = head + len;
        if self.buf.len() < end {
            return Ok(None);
        }
        let body = &self.buf[head..end];
        let packet = match self.threshold {
            None => body.to_vec(),
            Some(t) => inflate(body, t)?,
        };
        self.buf.drain(..end);
        Ok(Some(packet))
    }
}

fn inflate(body: &[u8], threshold: usize) -> Result<Vec<u8>> {
    let mut r = crate::codec::Reader::new(body);
    let size = VarInt::read(&mut r)?.0;
    let data = r.rest();
    if size == 0 {
        return Ok(data.to_vec());
    }
    let size = usize::try_from(size).map_err(|_| Error::NegativeLength(i64::from(size)))?;
    if size < threshold {
        return Err(Error::BadCompression(format!(
            "size {size} below threshold {threshold}"
        )));
    }
    if size > MAX_PACKET {
        return Err(Error::FrameTooLarge(size));
    }
    let mut out = Vec::with_capacity(size);
    ZlibDecoder::new(data)
        .take(size as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| Error::BadCompression(e.to_string()))?;
    if out.len() != size {
        return Err(Error::BadCompression(format!(
            "expected {size} bytes, got {}",
            out.len()
        )));
    }
    Ok(out)
}

/// Blocking packet connection over any byte stream.
///
/// The caller picks the packet set for each call, so it owns the state machine
/// (handshake, status, login, play).
pub struct Connection<S> {
    stream: S,
    encoder: FrameEncoder,
    decoder: FrameDecoder,
}

impl<S: Read + Write> Connection<S> {
    pub fn new(stream: S) -> Self {
        Connection {
            stream,
            encoder: FrameEncoder::default(),
            decoder: FrameDecoder::default(),
        }
    }

    pub fn stream(&self) -> &S {
        &self.stream
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    /// Apply a `SetCompression` threshold to both directions.
    pub fn set_compression(&mut self, threshold: i32) {
        self.encoder.set_compression(threshold);
        self.decoder.set_compression(threshold);
    }

    /// Encrypt all later traffic in both directions with the shared secret.
    /// Call after sending the encryption response and before reading the next packet.
    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.encoder.enable_encryption(secret);
        self.decoder.enable_encryption(secret);
    }

    pub fn send<P: PacketSet>(&mut self, packet: &P) -> Result<()> {
        let mut out = Vec::new();
        self.encoder.encode(&packet.encode(), &mut out)?;
        self.stream.write_all(&out)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn recv<P: PacketSet>(&mut self) -> Result<P> {
        let frame = self.recv_frame()?;
        P::decode(&frame)
    }

    /// Next packet as raw bytes (id plus body).
    pub fn recv_frame(&mut self) -> Result<Vec<u8>> {
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(f) = self.decoder.next_frame()? {
                return Ok(f);
            }
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
            }
            self.decoder.feed(&chunk[..n]);
        }
    }
}
