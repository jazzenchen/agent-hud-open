//! Keychain-held signing key and certificate for the LAN TLS identity.
use super::*;
use core_foundation::{
    base::{CFType, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::CFDictionary,
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use security_framework::{
    certificate::SecCertificate,
    key::{Algorithm, SecKey},
    policy::SecPolicy,
    secure_transport::SslProtocolSide,
    trust::SecTrust,
};
use security_framework_sys::{item::*, keychain_item::SecItemCopyMatching};
extern "C" {
    static kSecAttrApplicationTag: CFStringRef;
    static kSecPrivateKeyAttrs: CFStringRef;
}
fn key(value: CFStringRef) -> CFType {
    unsafe { CFString::wrap_under_get_rule(value) }.as_CFType()
}
pub fn load(device: &str, directory: &Path) -> Result<Identity> {
    let tag = CFData::from_buffer(format!("app.agenthud.local-network.{device}").as_bytes());
    let secret = unsafe {
        let query = CFDictionary::from_CFType_pairs(&[
            (key(kSecClass), key(kSecClassKey)),
            (key(kSecAttrApplicationTag), tag.as_CFType()),
            (key(kSecAttrKeyType), key(kSecAttrKeyTypeECSECPrimeRandom)),
            (key(kSecReturnRef), CFBoolean::true_value().as_CFType()),
        ]);
        let mut found = std::ptr::null();
        let status = SecItemCopyMatching(query.as_concrete_TypeRef(), &mut found);
        if status == 0 && !found.is_null() {
            SecKey::wrap_under_create_rule(found as _)
        } else if status == -25300 {
            let private = CFDictionary::from_CFType_pairs(&[
                (
                    key(kSecAttrIsPermanent),
                    CFBoolean::true_value().as_CFType(),
                ),
                (key(kSecAttrApplicationTag), tag.as_CFType()),
                (
                    key(kSecAttrLabel),
                    CFString::new("Agent HUD local connection").as_CFType(),
                ),
            ]);
            let attrs = CFDictionary::from_CFType_pairs(&[
                (key(kSecAttrKeyType), key(kSecAttrKeyTypeECSECPrimeRandom)),
                (
                    key(kSecAttrKeySizeInBits),
                    CFNumber::from(256_i32).as_CFType(),
                ),
                (key(kSecPrivateKeyAttrs), private.as_CFType()),
            ]);
            #[allow(deprecated)]
            SecKey::generate(attrs.to_untyped()).map_err(|_| "Cannot create persistent LAN key")?
        } else {
            return Err("Cannot read persistent LAN key".into());
        }
    };
    let public = secret
        .public_key()
        .and_then(|key| key.external_representation())
        .ok_or("LAN public key unavailable")?;
    let certificate_file = directory.join("certificate.der");
    let stored = std::fs::read(&certificate_file)
        .ok()
        .filter(|bytes| current(bytes, public.bytes(), device));
    let certificate = match stored {
        Some(bytes) => bytes,
        None => {
            let certificate = super::certificate::certificate(public.bytes(), device, |message| {
                secret
                    .create_signature(Algorithm::ECDSASignatureMessageX962SHA256, message)
                    .map_err(|_| "LAN certificate signature".into())
            })?;
            crate::fs::private_dir(directory).map_err(|e| e.to_string())?;
            let temp = directory.join(format!(".certificate-{}", uuid::Uuid::new_v4()));
            std::fs::write(&temp, &certificate).map_err(|_| "LAN certificate write")?;
            std::fs::rename(temp, certificate_file).map_err(|_| "LAN certificate commit")?;
            certificate
        }
    };
    Identity::from_signer(certificate, Arc::new(KeySigner(secret)))
}
fn current(bytes: &[u8], public: &[u8], device: &str) -> bool {
    let Ok(certificate) = SecCertificate::from_der(bytes) else {
        return false;
    };
    let policy = SecPolicy::create_ssl(
        SslProtocolSide::SERVER,
        Some(&format!("agenthud-{device}.local")),
    );
    let Ok(mut trust) =
        SecTrust::create_with_certificates(std::slice::from_ref(&certificate), &[policy])
    else {
        return false;
    };
    if trust.set_anchor_certificates(&[certificate]).is_err()
        || trust.set_trust_anchor_certificates_only(true).is_err()
        || trust.set_network_fetch_allowed(false).is_err()
        || trust.evaluate_with_error().is_err()
    {
        return false;
    }
    trust
        .copy_public_key()
        .ok()
        .and_then(|key| key.external_representation())
        .is_some_and(|value| value.bytes() == public)
}
#[derive(Debug)]
struct KeySigner(SecKey);
impl rustls::sign::SigningKey for KeySigner {
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        offered
            .contains(&rustls::SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| Box::new(KeySigner(self.0.clone())) as Box<dyn rustls::sign::Signer>)
    }
    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        rustls::SignatureAlgorithm::ECDSA
    }
}
impl rustls::sign::Signer for KeySigner {
    fn sign(&self, message: &[u8]) -> std::result::Result<Vec<u8>, rustls::Error> {
        self.0
            .create_signature(Algorithm::ECDSASignatureMessageX962SHA256, message)
            .map_err(|_| rustls::Error::General("Keychain LAN signature failed".into()))
    }
    fn scheme(&self) -> rustls::SignatureScheme {
        rustls::SignatureScheme::ECDSA_NISTP256_SHA256
    }
}
