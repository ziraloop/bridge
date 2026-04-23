use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Maximum allowed absolute skew between the signed timestamp and the
/// receiver's clock, in seconds. Signatures whose timestamp lies outside
/// this window are rejected as stale/replayed.
pub const MAX_TIMESTAMP_AGE_SECS: i64 = 300;

/// Errors produced when verifying a webhook signature with freshness.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignError {
    #[error("webhook timestamp is stale or too far in the future")]
    StaleTimestamp,
    #[error("webhook signature mismatch")]
    InvalidSignature,
}

/// Sign a webhook payload with HMAC-SHA256.
/// Message format: "{timestamp}.{payload}"
/// Returns base64-encoded signature.
pub fn sign_webhook(payload: &[u8], secret: &str, timestamp: i64) -> String {
    let payload_str = std::str::from_utf8(payload).unwrap_or("");
    let message = format!("{}.{}", timestamp, payload_str);
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    base64::engine::general_purpose::STANDARD.encode(result.into_bytes())
}

/// Verify a webhook signature using constant-time comparison.
#[deprecated(note = "use verify_with_freshness; this function does not enforce a timestamp window")]
pub fn verify_webhook(payload: &[u8], secret: &str, timestamp: i64, signature: &str) -> bool {
    let expected = sign_webhook(payload, secret, timestamp);
    expected.as_bytes().ct_eq(signature.as_bytes()).into()
}

/// Verify a webhook signature and enforce a freshness window.
///
/// Rejects the request if `(now - timestamp).abs() > MAX_TIMESTAMP_AGE_SECS`,
/// then performs a constant-time HMAC-SHA256 comparison against the expected
/// signature over `"{timestamp}.{payload}"`.
pub fn verify_with_freshness(
    secret: &[u8],
    timestamp: i64,
    payload: &[u8],
    signature: &str,
    now: i64,
) -> Result<(), SignError> {
    if (now - timestamp).abs() > MAX_TIMESTAMP_AGE_SECS {
        return Err(SignError::StaleTimestamp);
    }
    let secret_str = std::str::from_utf8(secret).unwrap_or("");
    let expected = sign_webhook(payload, secret_str, timestamp);
    let matches: bool = expected.as_bytes().ct_eq(signature.as_bytes()).into();
    if matches {
        Ok(())
    } else {
        Err(SignError::InvalidSignature)
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_webhook_deterministic() {
        let payload = b"hello world";
        let secret = "my-secret";
        let timestamp = 1700000000;

        let sig1 = sign_webhook(payload, secret, timestamp);
        let sig2 = sign_webhook(payload, secret, timestamp);

        assert_eq!(
            sig1, sig2,
            "sign_webhook should produce deterministic output"
        );
        assert!(!sig1.is_empty(), "signature should not be empty");
    }

    #[test]
    fn test_verify_webhook_valid_signature() {
        let payload = b"test payload";
        let secret = "webhook-secret";
        let timestamp = 1700000000;

        let signature = sign_webhook(payload, secret, timestamp);
        assert!(
            verify_webhook(payload, secret, timestamp, &signature),
            "verify_webhook should return true for a valid signature"
        );
    }

    #[test]
    fn test_verify_webhook_tampered_payload() {
        let payload = b"original payload";
        let secret = "webhook-secret";
        let timestamp = 1700000000;

        let signature = sign_webhook(payload, secret, timestamp);
        let tampered = b"tampered payload";
        assert!(
            !verify_webhook(tampered, secret, timestamp, &signature),
            "verify_webhook should return false for a tampered payload"
        );
    }

    #[test]
    fn test_verify_webhook_wrong_secret() {
        let payload = b"test payload";
        let secret = "correct-secret";
        let wrong_secret = "wrong-secret";
        let timestamp = 1700000000;

        let signature = sign_webhook(payload, secret, timestamp);
        assert!(
            !verify_webhook(payload, wrong_secret, timestamp, &signature),
            "verify_webhook should return false for a wrong secret"
        );
    }

    #[test]
    fn test_verify_with_freshness_valid_within_window() {
        let payload = b"fresh payload";
        let secret = "secret";
        let now = 1_700_000_000;
        let signature = sign_webhook(payload, secret, now);
        assert_eq!(
            verify_with_freshness(secret.as_bytes(), now, payload, &signature, now),
            Ok(()),
        );
        assert_eq!(
            verify_with_freshness(
                secret.as_bytes(),
                now - MAX_TIMESTAMP_AGE_SECS + 1,
                payload,
                &sign_webhook(payload, secret, now - MAX_TIMESTAMP_AGE_SECS + 1),
                now
            ),
            Ok(()),
        );
    }

    #[test]
    fn test_verify_with_freshness_rejects_stale() {
        let payload = b"stale payload";
        let secret = "secret";
        let timestamp = 1_700_000_000;
        let now = timestamp + MAX_TIMESTAMP_AGE_SECS + 1;
        let signature = sign_webhook(payload, secret, timestamp);
        assert_eq!(
            verify_with_freshness(secret.as_bytes(), timestamp, payload, &signature, now),
            Err(SignError::StaleTimestamp),
        );
    }

    #[test]
    fn test_verify_with_freshness_rejects_future_skew() {
        let payload = b"future payload";
        let secret = "secret";
        let now = 1_700_000_000;
        let timestamp = now + MAX_TIMESTAMP_AGE_SECS + 1;
        let signature = sign_webhook(payload, secret, timestamp);
        assert_eq!(
            verify_with_freshness(secret.as_bytes(), timestamp, payload, &signature, now),
            Err(SignError::StaleTimestamp),
        );
    }

    #[test]
    fn test_verify_with_freshness_rejects_bad_signature() {
        let payload = b"payload";
        let secret = "secret";
        let now = 1_700_000_000;
        assert_eq!(
            verify_with_freshness(secret.as_bytes(), now, payload, "not-a-signature", now),
            Err(SignError::InvalidSignature),
        );
    }
}
