//! Per-installation, non-exportable P-256 key in the current user's CNG store.
use super::*;
use sha2::{Digest, Sha256};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{Foundation::NTE_BAD_KEYSET, Security::Cryptography::*};

#[derive(Debug)]
struct Handle(usize);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { NCryptFreeObject(self.0) };
    }
}
fn checked(status: i32, operation: &str) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed (0x{:08x})", status as u32))
    }
}

pub(super) fn load(device: &str, directory: &Path) -> Result<Identity> {
    let key_name: Vec<u16> = format!("app.agenthud.local-network.{device}")
        .encode_utf16()
        .chain([0])
        .collect();
    let key = unsafe {
        let mut provider = 0;
        checked(
            NCryptOpenStorageProvider(&mut provider, MS_KEY_STORAGE_PROVIDER, 0),
            "Open LAN key store",
        )?;
        let provider = Handle(provider);
        let mut key = 0;
        let status = NCryptOpenKey(
            provider.0,
            &mut key,
            key_name.as_ptr(),
            0,
            NCRYPT_SILENT_FLAG,
        );
        if status == NTE_BAD_KEYSET {
            checked(
                NCryptCreatePersistedKey(
                    provider.0,
                    &mut key,
                    NCRYPT_ECDSA_P256_ALGORITHM,
                    key_name.as_ptr(),
                    0,
                    0,
                ),
                "Create LAN key",
            )?;
            let key = Handle(key);
            checked(
                NCryptSetProperty(
                    key.0,
                    NCRYPT_EXPORT_POLICY_PROPERTY,
                    0u32.to_le_bytes().as_ptr(),
                    4,
                    0,
                ),
                "Protect LAN key",
            )?;
            checked(NCryptFinalizeKey(key.0, 0), "Persist LAN key")?;
            key
        } else {
            checked(status, "Read LAN key")?;
            Handle(key)
        }
    };
    let signer = KeySigner(Arc::new(key));
    let public = signer.public()?;
    let certificate_file = directory.join("certificate.der");
    let certificate = match std::fs::read(&certificate_file) {
        Ok(bytes) if current(&bytes, &public, device) => bytes,
        Ok(_) => save_certificate(&signer, &public, device, directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            save_certificate(&signer, &public, device, directory)?
        }
        Err(error) => return Err(format!("Read LAN certificate: {error}")),
    };
    Identity::from_signer(certificate, Arc::new(signer))
}
fn save_certificate(
    signer: &KeySigner,
    public: &[u8],
    device: &str,
    directory: &Path,
) -> Result<Vec<u8>> {
    let certificate =
        super::certificate::certificate(public, device, |message| signer.signature(message))?;
    crate::fs::private_dir(directory)?;
    let temp = directory.join(format!(".certificate-{}", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        std::fs::write(&temp, &certificate)?;
        std::fs::rename(&temp, directory.join("certificate.der"))
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(temp);
        return Err(format!("Save LAN certificate: {error}"));
    }
    Ok(certificate)
}
fn current(bytes: &[u8], public: &[u8], device: &str) -> bool {
    unsafe {
        let cert =
            CertCreateCertificateContext(X509_ASN_ENCODING, bytes.as_ptr(), bytes.len() as u32);
        if cert.is_null() {
            return false;
        }
        let valid = (|| {
            let info = (*cert).pCertInfo.as_ref()?;
            let bits = &info.SubjectPublicKeyInfo.PublicKey;
            if bits.pbData.is_null()
                || bits.cbData as usize != public.len()
                || std::slice::from_raw_parts(bits.pbData, bits.cbData as usize) != public
            {
                return None;
            }
            let mut flags = CERT_STORE_SIGNATURE_FLAG | CERT_STORE_TIME_VALIDITY_FLAG;
            if CertVerifySubjectCertificateContext(cert, cert, &mut flags) == 0 || flags != 0 {
                return None;
            }
            let length = CertGetNameStringW(cert, CERT_NAME_DNS_TYPE, 0, null(), null_mut(), 0);
            if length <= 1 {
                return None;
            }
            let mut name = vec![0u16; length as usize];
            if CertGetNameStringW(
                cert,
                CERT_NAME_DNS_TYPE,
                0,
                null(),
                name.as_mut_ptr(),
                length,
            ) != length
            {
                return None;
            }
            (String::from_utf16_lossy(&name[..name.len() - 1])
                == format!("agenthud-{device}.local"))
            .then_some(())
        })()
        .is_some();
        CertFreeCertificateContext(cert);
        valid
    }
}
#[derive(Clone, Debug)]
struct KeySigner(Arc<Handle>);
impl KeySigner {
    fn public(&self) -> Result<Vec<u8>> {
        let mut blob = [0u8; 72]; // BCRYPT_ECCKEY_BLOB header, X and Y (32 bytes each).
        let mut length = 0;
        checked(
            unsafe {
                NCryptExportKey(
                    self.0 .0,
                    0,
                    BCRYPT_ECCPUBLIC_BLOB,
                    null(),
                    blob.as_mut_ptr(),
                    blob.len() as u32,
                    &mut length,
                    0,
                )
            },
            "Read LAN public key",
        )?;
        if length != 72
            || u32::from_le_bytes(blob[..4].try_into().unwrap()) != BCRYPT_ECDSA_PUBLIC_P256_MAGIC
            || u32::from_le_bytes(blob[4..8].try_into().unwrap()) != 32
        {
            return Err("Invalid LAN P-256 public key".into());
        }
        Ok([&[4u8][..], &blob[8..]].concat())
    }
    fn signature(&self, message: &[u8]) -> Result<Vec<u8>> {
        let hash = Sha256::digest(message);
        let mut raw = [0u8; 64];
        let mut length = 0;
        checked(
            unsafe {
                NCryptSignHash(
                    self.0 .0,
                    null(),
                    hash.as_ptr(),
                    hash.len() as u32,
                    raw.as_mut_ptr(),
                    raw.len() as u32,
                    &mut length,
                    NCRYPT_SILENT_FLAG,
                )
            },
            "Sign LAN message",
        )?;
        if length != 64 {
            return Err("Invalid LAN P-256 signature".into());
        }
        Ok(ecdsa_der(&raw))
    }
}
// CNG returns fixed-width r || s; TLS and X.509 require an ASN.1 SEQUENCE.
fn ecdsa_der(raw: &[u8; 64]) -> Vec<u8> {
    fn integer(bytes: &[u8]) -> Vec<u8> {
        let first = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        let bytes = &bytes[first..];
        let padding = usize::from(bytes[0] & 0x80 != 0);
        let mut result = vec![2, (bytes.len() + padding) as u8];
        if padding != 0 {
            result.push(0);
        }
        result.extend_from_slice(bytes);
        result
    }
    let body = [integer(&raw[..32]), integer(&raw[32..])].concat();
    [vec![0x30, body.len() as u8], body].concat()
}
impl rustls::sign::SigningKey for KeySigner {
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        offered
            .contains(&rustls::SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| Box::new(self.clone()) as Box<dyn rustls::sign::Signer>)
    }
    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        rustls::SignatureAlgorithm::ECDSA
    }
}
impl rustls::sign::Signer for KeySigner {
    fn sign(&self, message: &[u8]) -> std::result::Result<Vec<u8>, rustls::Error> {
        self.signature(message).map_err(rustls::Error::General)
    }
    fn scheme(&self) -> rustls::SignatureScheme {
        rustls::SignatureScheme::ECDSA_NISTP256_SHA256
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signatures_encode_positive_minimal_integers() {
        let mut raw = [0u8; 64];
        raw[31] = 1;
        raw[32] = 0x80;
        let der = ecdsa_der(&raw);
        assert_eq!(&der[..8], &[0x30, 38, 2, 1, 1, 2, 33, 0]);
        assert_eq!(der[8], 0x80);
    }
    #[test]
    fn cng_identity_reopens_and_signs_without_private_key_export() {
        let device = uuid::Uuid::new_v4().to_string();
        let directory = std::env::temp_dir().join(format!("hud-cng-{device}"));
        let first = load(&device, &directory).unwrap();
        let second = load(&device, &directory).unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.certificate, second.certificate);
        // Isolated per-test CNG key, removed from the runner after the check.
        let name: Vec<_> = format!("app.agenthud.local-network.{device}")
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            let mut provider = 0;
            assert_eq!(
                NCryptOpenStorageProvider(&mut provider, MS_KEY_STORAGE_PROVIDER, 0),
                0
            );
            let provider = Handle(provider);
            let mut key = 0;
            assert_eq!(NCryptOpenKey(provider.0, &mut key, name.as_ptr(), 0, 0), 0);
            let mut size = 0;
            assert_ne!(
                NCryptExportKey(
                    key,
                    0,
                    BCRYPT_ECCPRIVATE_BLOB,
                    null(),
                    null_mut(),
                    0,
                    &mut size,
                    0
                ),
                0
            );
            assert_eq!(NCryptDeleteKey(key, 0), 0);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
