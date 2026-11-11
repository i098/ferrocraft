//! Online-mode login, as vanilla 1.8.9 does it (`NetHandlerLoginClient.handleEncryptionRequest`,
//! `CryptManager`, `YggdrasilMinecraftSessionService.joinServer`).

use rand_core::{OsRng, RngCore};
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use sha1::{Digest, Sha1};
use std::fmt;
use std::io;

const JOIN_URL: &str = "https://sessionserver.mojang.com/session/minecraft/join";

/// The account that the launcher passes with `--accessToken` and `--uuid`.
#[derive(Clone, PartialEq)]
pub struct Auth {
    access_token: String,
    /// Profile UUID without dashes, as the session server expects it.
    uuid: String,
}

impl Auth {
    pub fn new(access_token: String, uuid: &str) -> Auth {
        Auth {
            access_token,
            uuid: uuid.replace('-', ""),
        }
    }
}

// The token must never reach a log, so `Debug` hides it.
impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Auth")
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .finish()
    }
}

/// What one `EncryptionRequest` needs to answer.
pub struct Handshake {
    pub secret: [u8; 16],
    pub encrypted_secret: Vec<u8>,
    pub encrypted_token: Vec<u8>,
}

/// The Minecraft server hash: SHA-1 over server id, secret and public key, printed as a signed
/// big-endian number in hex (`BigInteger.toString(16)` in Java).
pub fn server_hash(server_id: &str, secret: &[u8], public_key: &[u8]) -> String {
    let mut sha = Sha1::new();
    sha.update(server_id.as_bytes());
    sha.update(secret);
    sha.update(public_key);
    let mut d: [u8; 20] = sha.finalize().into();
    let negative = d[0] & 0x80 != 0;
    if negative {
        // Two's complement negation.
        let mut carry = true;
        for b in d.iter_mut().rev() {
            *b = !*b;
            if carry {
                (*b, carry) = b.overflowing_add(1);
            }
        }
    }
    let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
    let hex = hex.trim_start_matches('0');
    let hex = if hex.is_empty() { "0" } else { hex };
    if negative {
        format!("-{hex}")
    } else {
        hex.to_string()
    }
}

fn denied(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, msg.into())
}

/// Answers an `EncryptionRequest`: makes the shared secret, tells the session server that this
/// account joins, and returns the RSA-encrypted secret and verify token for the response.
pub fn answer_request(
    auth: &Auth,
    server_id: &str,
    public_key: &[u8],
    verify_token: &[u8],
) -> io::Result<Handshake> {
    let key = RsaPublicKey::from_public_key_der(public_key)
        .map_err(|e| denied(format!("bad server public key: {e}")))?;
    let mut secret = [0u8; 16];
    OsRng.fill_bytes(&mut secret);
    join_session(auth, &server_hash(server_id, &secret, public_key))?;
    let encrypt = |data: &[u8]| {
        key.encrypt(&mut OsRng, Pkcs1v15Encrypt, data)
            .map_err(|e| denied(format!("RSA encryption failed: {e}")))
    };
    Ok(Handshake {
        secret,
        encrypted_secret: encrypt(&secret)?,
        encrypted_token: encrypt(verify_token)?,
    })
}

/// `joinServer`: HTTP 204 means success. The error text never contains the token.
fn join_session(auth: &Auth, hash: &str) -> io::Result<()> {
    let body = serde_json::json!({
        "accessToken": auth.access_token,
        "selectedProfile": auth.uuid,
        "serverId": hash,
    });
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .into();
    let resp = agent
        .post(JOIN_URL)
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| io::Error::other(format!("session server unreachable: {e}")))?;
    match resp.status().as_u16() {
        204 => Ok(()),
        403 => Err(denied("Invalid session (try restarting your game)")),
        s => Err(denied(format!("session server answered HTTP {s}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known vectors from wiki.vg (Notch, jeb_, simon): the hash of the name alone.
    #[test]
    fn wiki_vg_vectors() {
        let h = |name| server_hash(name, &[], &[]);
        assert_eq!(h("Notch"), "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
        assert_eq!(h("jeb_"), "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
        assert_eq!(h("simon"), "88e16a1019277b15d58faf0541e11910eb756f6");
    }

    #[test]
    fn parts_are_concatenated_in_order() {
        assert_eq!(
            server_hash("No", b"tc", b"h"),
            server_hash("Notch", &[], &[])
        );
    }
}
