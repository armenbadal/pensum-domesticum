use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub(crate) const CERTIFICATE_FILE_NAME: &str = "teacher.pdcert";
pub(crate) const MAX_CERTIFICATE_BYTES: usize = 16 * 1024;

const CERTIFICATE_VERSION: u32 = 1;
const TEACHER_ROLE: &str = "teacher";
const PILOT_ISSUER_ID: &str = "pensum-pilot";

pub(crate) const PILOT_PUBLIC_KEY: Option<[u8; 32]> = Some([
    0x7f, 0xa1, 0x7b, 0xd0, 0x4b, 0x08, 0x14, 0x6b, 0x50, 0xbb, 0xec, 0xf1, 0x5d, 0x51, 0x08, 0x37,
    0x68, 0x64, 0x4f, 0x87, 0xec, 0x46, 0x5e, 0x3b, 0x04, 0xad, 0x48, 0xb5, 0x8d, 0x81, 0x1c, 0xda,
]);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CertificateEnvelope {
    payload: String,
    signature: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CertificatePayload {
    version: u32,
    role: String,
    issuer_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppRole {
    Student,
    Teacher,
}

impl AppRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Student => "student",
            Self::Teacher => "teacher",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TrustedIssuer {
    pub(crate) issuer_id: &'static str,
    pub(crate) public_key: [u8; 32],
}

pub(crate) fn configured_issuers() -> Vec<TrustedIssuer> {
    PILOT_PUBLIC_KEY
        .map(|public_key| TrustedIssuer {
            issuer_id: PILOT_ISSUER_ID,
            public_key,
        })
        .into_iter()
        .collect()
}

pub(crate) fn certificate_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CERTIFICATE_FILE_NAME)
}

pub(crate) fn role_from_local_certificate(config_dir: &Path) -> AppRole {
    role_from_local_certificate_with_issuers(config_dir, &configured_issuers())
}

pub(crate) fn role_from_local_certificate_with_issuers(
    config_dir: &Path,
    issuers: &[TrustedIssuer],
) -> AppRole {
    let path = certificate_path(config_dir);
    match read_certificate_file(&path) {
        Ok(bytes) => match verify_certificate_bytes(&bytes, issuers) {
            Ok(role) => role,
            Err(error) => {
                eprintln!("Տեղային դասատուի սերտիֆիկատը մերժվեց․ {error}");
                AppRole::Student
            }
        },
        Err(error) => {
            if path.exists() {
                eprintln!("Չհաջողվեց կարդալ դասատուի սերտիֆիկատը․ {error}");
            }
            AppRole::Student
        }
    }
}

pub(crate) fn import_certificate(
    source_path: &Path,
    config_dir: &Path,
    issuers: &[TrustedIssuer],
) -> Result<AppRole, String> {
    let bytes = read_certificate_file(source_path)?;
    let role = verify_certificate_bytes(&bytes, issuers)?;
    save_certificate_bytes(config_dir, &bytes)?;
    Ok(role)
}

pub(crate) fn read_certificate_file(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Չհաջողվեց կարդալ սերտիֆիկատի ֆայլը։ {error}"))?;
    if !metadata.is_file() {
        return Err("Սերտիֆիկատի ուղին ֆայլ չէ։".to_string());
    }
    if metadata.len() == 0 {
        return Err("Սերտիֆիկատի ֆայլը դատարկ է։".to_string());
    }
    if metadata.len() > MAX_CERTIFICATE_BYTES as u64 {
        return Err(format!(
            "Սերտիֆիկատի չափը գերազանցում է {MAX_CERTIFICATE_BYTES} բայթ սահմանը։"
        ));
    }

    let bytes =
        fs::read(path).map_err(|error| format!("Չհաջողվեց կարդալ սերտիֆիկատի ֆայլը։ {error}"))?;
    if bytes.is_empty() {
        return Err("Սերտիֆիկատի ֆայլը դատարկ է։".to_string());
    }
    if bytes.len() > MAX_CERTIFICATE_BYTES {
        return Err(format!(
            "Սերտիֆիկատի չափը գերազանցում է {MAX_CERTIFICATE_BYTES} բայթ սահմանը։"
        ));
    }
    Ok(bytes)
}

pub(crate) fn verify_certificate_bytes(
    bytes: &[u8],
    issuers: &[TrustedIssuer],
) -> Result<AppRole, String> {
    if bytes.is_empty() {
        return Err("Սերտիֆիկատի ֆայլը դատարկ է։".to_string());
    }
    if bytes.len() > MAX_CERTIFICATE_BYTES {
        return Err(format!(
            "Սերտիֆիկատի չափը գերազանցում է {MAX_CERTIFICATE_BYTES} բայթ սահմանը։"
        ));
    }
    if issuers.is_empty() {
        return Err("Հավելվածում վստահված դասատուի հանրային բանալի դեռ սահմանված չէ։".to_string());
    }

    let envelope: CertificateEnvelope = serde_json::from_slice(bytes)
        .map_err(|error| format!("Սերտիֆիկատի JSON-ը սխալ է։ {error}"))?;
    let payload_bytes = STANDARD
        .decode(envelope.payload.as_bytes())
        .map_err(|error| format!("Սերտիֆիկատի payload-ը վավեր base64 չէ։ {error}"))?;
    let signature_bytes = STANDARD
        .decode(envelope.signature.as_bytes())
        .map_err(|error| format!("Սերտիֆիկատի ստորագրությունը վավեր base64 չէ։ {error}"))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| format!("Սերտիֆիկատի ստորագրությունը սխալ երկարություն ունի։ {error}"))?;

    let mut verified_issuer = None;
    for issuer in issuers {
        let verifying_key = VerifyingKey::from_bytes(&issuer.public_key)
            .map_err(|error| format!("Վստահված հանրային բանալին անվավեր է։ {error}"))?;
        if verifying_key
            .verify_strict(&payload_bytes, &signature)
            .is_ok()
        {
            verified_issuer = Some(*issuer);
            break;
        }
    }
    let issuer =
        verified_issuer.ok_or_else(|| "Սերտիֆիկատի ստորագրությունը վավեր չէ։".to_string())?;

    let payload: CertificatePayload = serde_json::from_slice(&payload_bytes)
        .map_err(|error| format!("Սերտիֆիկատի payload-ը սխալ ձևաչափ ունի։ {error}"))?;
    if payload.version != CERTIFICATE_VERSION {
        return Err(format!(
            "Սերտիֆիկատի տարբերակը չի աջակցվում․ {}։",
            payload.version
        ));
    }
    if payload.role != TEACHER_ROLE {
        return Err("Սերտիֆիկատը դասատուի դեր չի հաստատում։".to_string());
    }
    if payload.issuer_id != issuer.issuer_id {
        return Err("Սերտիֆիկատի տրամադրողը չի համապատասխանում ստորագրող բանալուն։".to_string());
    }

    Ok(AppRole::Teacher)
}

fn save_certificate_bytes(config_dir: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(config_dir)
        .map_err(|error| format!("Չհաջողվեց ստեղծել սերտիֆիկատի պանակը։ {error}"))?;
    let mut temporary = NamedTempFile::new_in(config_dir)
        .map_err(|error| format!("Չհաջողվեց ստեղծել ժամանակավոր սերտիֆիկատ։ {error}"))?;
    temporary
        .write_all(bytes)
        .map_err(|error| format!("Չհաջողվեց գրել սերտիֆիկատը։ {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Չհաջողվեց պահպանել սերտիֆիկատը սկավառակի վրա։ {error}"))?;
    temporary
        .persist(certificate_path(config_dir))
        .map_err(|error| format!("Չհաջողվեց վերջնականացնել սերտիֆիկատը։ {}", error.error))?;
    if let Err(error) = sync_parent_directory(config_dir) {
        eprintln!("Չհաջողվեց պահպանել սերտիֆիկատի պանակը սկավառակի վրա։ {error}");
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> std::io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fs;

    const TEST_ISSUER_ID: &str = "pensum-pilot";

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0x42; 32])
    }

    fn other_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0x24; 32])
    }

    fn test_issuers() -> Vec<TrustedIssuer> {
        vec![TrustedIssuer {
            issuer_id: TEST_ISSUER_ID,
            public_key: signing_key().verifying_key().to_bytes(),
        }]
    }

    fn signed_certificate(payload: &[u8], key: &SigningKey) -> Vec<u8> {
        let envelope = CertificateEnvelope {
            payload: STANDARD.encode(payload),
            signature: STANDARD.encode(key.sign(payload).to_bytes()),
        };
        serde_json::to_vec(&envelope).unwrap()
    }

    fn teacher_payload() -> Vec<u8> {
        br#"{"version":1,"role":"teacher","issuer_id":"pensum-pilot"}"#.to_vec()
    }

    fn mutate_first_signature_byte(certificate: &[u8]) -> Vec<u8> {
        let mut envelope: CertificateEnvelope = serde_json::from_slice(certificate).unwrap();
        let mut signature = STANDARD.decode(envelope.signature.as_bytes()).unwrap();
        signature[0] ^= 0x01;
        envelope.signature = STANDARD.encode(signature);
        serde_json::to_vec(&envelope).unwrap()
    }

    #[test]
    fn valid_pilot_certificate_is_accepted() {
        let certificate = signed_certificate(&teacher_payload(), &signing_key());
        assert_eq!(
            verify_certificate_bytes(&certificate, &test_issuers()).unwrap(),
            AppRole::Teacher
        );
    }

    #[test]
    fn wrong_signature_is_rejected() {
        let certificate = signed_certificate(&teacher_payload(), &signing_key());
        let certificate = mutate_first_signature_byte(&certificate);
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn certificate_signed_by_another_key_is_rejected() {
        let certificate = signed_certificate(&teacher_payload(), &other_signing_key());
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn invalid_base64_is_rejected() {
        let envelope = CertificateEnvelope {
            payload: "not-base64!!!".to_string(),
            signature: STANDARD.encode([0u8; 64]),
        };
        let certificate = serde_json::to_vec(&envelope).unwrap();
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn invalid_json_is_rejected() {
        assert!(verify_certificate_bytes(b"not json", &test_issuers()).is_err());
    }

    #[test]
    fn oversized_certificate_is_rejected() {
        let certificate = vec![b'{'; MAX_CERTIFICATE_BYTES + 1];
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn unknown_version_is_rejected() {
        let payload = br#"{"version":2,"role":"teacher","issuer_id":"pensum-pilot"}"#;
        let certificate = signed_certificate(payload, &signing_key());
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn non_teacher_role_is_rejected() {
        let payload = br#"{"version":1,"role":"student","issuer_id":"pensum-pilot"}"#;
        let certificate = signed_certificate(payload, &signing_key());
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn unknown_issuer_is_rejected() {
        let payload = br#"{"version":1,"role":"teacher","issuer_id":"another-issuer"}"#;
        let certificate = signed_certificate(payload, &signing_key());
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn unknown_payload_field_is_rejected() {
        let payload = br#"{"version":1,"role":"teacher","issuer_id":"pensum-pilot","extra":true}"#;
        let certificate = signed_certificate(payload, &signing_key());
        assert!(verify_certificate_bytes(&certificate, &test_issuers()).is_err());
    }

    #[test]
    fn missing_local_certificate_falls_back_to_student() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            role_from_local_certificate_with_issuers(directory.path(), &test_issuers()),
            AppRole::Student
        );
    }

    #[test]
    fn invalid_local_certificate_falls_back_to_student_without_panicking() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(certificate_path(directory.path()), b"invalid").unwrap();
        assert_eq!(
            role_from_local_certificate_with_issuers(directory.path(), &test_issuers()),
            AppRole::Student
        );
    }

    #[test]
    fn invalid_import_does_not_replace_existing_certificate() {
        let directory = tempfile::tempdir().unwrap();
        let existing = signed_certificate(&teacher_payload(), &signing_key());
        let target = certificate_path(directory.path());
        fs::write(&target, &existing).unwrap();

        let invalid_source = directory.path().join("invalid-source.pdcert");
        fs::write(&invalid_source, b"invalid certificate").unwrap();

        assert!(import_certificate(&invalid_source, directory.path(), &test_issuers()).is_err());
        assert_eq!(fs::read(&target).unwrap(), existing);
    }

    #[test]
    fn valid_import_persists_certificate_and_returns_teacher() {
        let directory = tempfile::tempdir().unwrap();
        let certificate = signed_certificate(&teacher_payload(), &signing_key());
        let source = directory.path().join("pilot.pdcert");
        fs::write(&source, &certificate).unwrap();

        assert_eq!(
            import_certificate(&source, directory.path(), &test_issuers()).unwrap(),
            AppRole::Teacher
        );
        assert_eq!(
            fs::read(certificate_path(directory.path())).unwrap(),
            certificate
        );
    }

    #[test]
    fn unsigned_or_arbitrary_input_cannot_grant_teacher_role() {
        for input in [
            b"teacher".as_slice(),
            b"{\"role\":\"teacher\"}".as_slice(),
            b"{\"payload\":\"e30=\",\"signature\":\"\"}".as_slice(),
        ] {
            assert!(verify_certificate_bytes(input, &test_issuers()).is_err());
        }
    }

    #[test]
    fn role_names_are_stable() {
        assert_eq!(AppRole::Student.as_str(), "student");
        assert_eq!(AppRole::Teacher.as_str(), "teacher");
    }
}
