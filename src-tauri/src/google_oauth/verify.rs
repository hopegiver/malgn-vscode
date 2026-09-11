// ---------------- Google id_token 서명·클레임 검증 ----------------
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// ⚠️ 절대 손대지 말 것(작업 지시): JWKS 서명검증, `hd` 도메인 강제, 만료·audience·
// issuer 검사 — 최근 사람 승인·검증을 거친 로직이라 이 파일에서도 순서·조건을
// 한 글자도 바꾸지 않았다.

use super::GOOGLE_JWKS_ENDPOINT;

#[derive(serde::Deserialize, Debug)]
pub(super) struct GoogleJwk {
    kid: String,
    n: String,
    e: String,
}

#[derive(serde::Deserialize, Debug)]
pub(super) struct GoogleJwks {
    keys: Vec<GoogleJwk>,
}

pub(super) async fn fetch_google_jwks(client: &reqwest::Client) -> Result<GoogleJwks, String> {
    let resp = client
        .get(GOOGLE_JWKS_ENDPOINT)
        .send()
        .await
        .map_err(|e| format!("Google JWKS를 가져오지 못했습니다: {e}"))?;
    resp.json::<GoogleJwks>()
        .await
        .map_err(|e| format!("JWKS 응답을 해석하지 못했습니다: {e}"))
}

#[derive(serde::Deserialize, Debug, Clone)]
pub(super) struct GoogleIdTokenClaims {
    // iss/aud는 jsonwebtoken::Validation의 set_issuer/set_audience가 디코딩 과정에서
    // 이미 검증한다 — 구조체 필드는 디버그 가시성(로그 등)을 위해서만 남겨둔다.
    #[allow(dead_code)]
    iss: String,
    #[allow(dead_code)]
    aud: String,
    pub(super) email: Option<String>,
    email_verified: Option<bool>,
    pub(super) hd: Option<String>,
    pub(super) name: Option<String>,
}

/// 서명·`iss`·`aud`·`exp`를 `jsonwebtoken`으로 검증한다(`exp`는 라이브러리가 기본
/// 켜져 있는 검증이라 별도 수동 체크가 필요 없다). JWKS에서 토큰 헤더의 `kid`와
/// 일치하는 키를 못 찾으면 실패시킨다 — 서명 검증 없이 클레임만 믿지 않는다.
pub(super) fn verify_google_id_token_signature(
    id_token: &str,
    jwks: &GoogleJwks,
    client_id: &str,
) -> Result<GoogleIdTokenClaims, String> {
    let header = jsonwebtoken::decode_header(id_token)
        .map_err(|e| format!("토큰 헤더를 해석하지 못했습니다: {e}"))?;
    let kid = header
        .kid
        .ok_or_else(|| "토큰 헤더에 kid가 없습니다.".to_string())?;
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == kid)
        .ok_or_else(|| "JWKS에서 일치하는 키를 찾지 못했습니다.".to_string())?;

    let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
        .map_err(|e| format!("공개키를 구성하지 못했습니다: {e}"))?;

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);

    jsonwebtoken::decode::<GoogleIdTokenClaims>(id_token, &decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|e| format!("토큰 서명 또는 클레임 검증에 실패했습니다: {e}"))
}

/// 서명 검증과 분리된 순수 로직 — hd 도메인 제한 + email_verified 확인. hd 클레임은
/// URL 파라미터가 아니라 여기, 서명 검증을 통과한 토큰의 클레임에서만 신뢰한다.
pub(super) fn check_domain_restriction(
    claims: &GoogleIdTokenClaims,
    required_hd: &str,
) -> Result<(), String> {
    if claims.hd.as_deref() != Some(required_hd) {
        return Err(format!(
            "허용되지 않은 조직 도메인입니다(hd={:?}, 필요값={required_hd}).",
            claims.hd
        ));
    }
    if claims.email_verified != Some(true) {
        return Err("이메일이 인증되지 않았습니다(email_verified가 true가 아닙니다).".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- hd 도메인 제한 + email_verified 확인 (서명 검증과 분리된 순수 로직) ----

    fn sample_claims(hd: Option<&str>, email_verified: Option<bool>) -> GoogleIdTokenClaims {
        GoogleIdTokenClaims {
            iss: "https://accounts.google.com".to_string(),
            aud: "test-client-id".to_string(),
            email: Some("dev@malgnsoft.com".to_string()),
            email_verified,
            hd: hd.map(|s| s.to_string()),
            name: Some("Dev".to_string()),
        }
    }

    #[test]
    fn accepts_correct_domain_and_verified_email() {
        let claims = sample_claims(Some("malgnsoft.com"), Some(true));
        assert!(check_domain_restriction(&claims, "malgnsoft.com").is_ok());
    }

    #[test]
    fn rejects_wrong_hd_domain() {
        let claims = sample_claims(Some("gmail.com"), Some(true));
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "다른 조직 도메인은 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_missing_hd_claim() {
        // 개인 Gmail 계정 등 Workspace가 아닌 계정은 hd 클레임 자체가 없다.
        let claims = sample_claims(None, Some(true));
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "hd 클레임이 없으면 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_unverified_email() {
        let claims = sample_claims(Some("malgnsoft.com"), Some(false));
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "email_verified가 false면 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_missing_email_verified() {
        let claims = sample_claims(Some("malgnsoft.com"), None);
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "email_verified 클레임이 없으면 거부되어야 합니다"
        );
    }

    // ---- 서명 검증 (합성 RSA 키쌍으로 실제 서명된 JWT를 만들어 검증) ----
    // 테스트 전용으로 새로 생성한 2048비트 RSA 키쌍이다(운영 키와 무관 — Google의
    // 실제 개인키는 당연히 우리에게 없다). 이 키로 직접 서명한 토큰을 우리
    // verify_google_id_token_signature()에 통과시켜 서명·iss·aud·exp 검증이 실제로
    // 동작하는지 확인한다.
    const TEST_RSA_PRIVATE_KEY_PEM: &str =
        // pragma: allowlist-secret (테스트 전용 합성 키, 실서비스와 무관)
        "-----BEGIN RSA PRIVATE KEY-----
MIIEogIBAAKCAQEAnrvlIKSb3xV5R+JTXVvNsj5cPZWRh9NsV3qfFTeT4IewGBH8
MNDFlG21tb8PShKamFVgReoSp25X2+WlalGeePP7F8dV9Q7UGKQKpubKIbErYjiO
lSlB+g5nle5sIlHVZkg6aAkAC1g5hrCgYLun2MpCdaQx2qc846I1RooBFrsMiubj
jASrLibqcYwqbY2i3gXWh/FnCUtjqL33KG3aEoDEcuKCnarhTqnQ8XBjCye5PNIg
R1TwfcXmCpfuYyx2eWp4ouJ5QI4YHiGH2YberYnCvSQdxtUOi5zP8fTE0EBc081h
olKqb89JD+WobwH2uM1GQ1GE+1yMWW2I2UzUIQIDAQABAoIBAGZpXdQov/Q3W49Y
Y2bJczX76/FDzagvbSgnkfnTaNIlWSS+fdJU8BTqj6EaCthEln+QHdQdyDlEBOV4
DbhBvpfU+fyGfFvmXEslk0XJg0Inl5EAYmW0P8AAiS5/rD6cQ62BDkXPALtRCZRv
4plmmU1SeXyDGjMzUSKgpfTD1x39PkwPurHAMHoVgipwanXUQCuNzyD/Sr2DW9bg
OJm+LeBg2Y0ZuUGVYukswUTk1kZpWuTg+SiPwxdIaSCpEVD9I8J/EpaHu5HohiNI
2wMPHLFJQ9LEy9XqRUFH3retMKEatoQVDTxAj8gkRX+36HAQIWFbshAgWeo239zc
AXRlWN0CgYEAzzDTSvLLao3D4CXxajcF7XbWmOsw0MgRbXfRgXvHUPdb5D799tl+
tICymp69DeX4nkjUFtZsKBe1+wa+5MHupYAUYKWewf2DyOJl1z/Vjh9Bb2K1Q/nU
jwjDaX3our/z8+uZbtyrQoSQTIEUBfvEAHxG8yEGQfLlzZcS+nTZNNMCgYEAxCDC
TGsrDzh5fRNlx/eewgQBv9/0PS1M9uXnF7EHe0rfdLsIRWTGXdeWlKKeV0AtTJkd
rufeExkeiXo/TzUIoSoLmW39gHvDhtg0R8rcKdjqeA4Z1KTnGUKnOJNTKLQTO+Mv
uGxv2BdlDAVf4G+LnphstLJClFvlNT8KK2NBCrsCgYBV26/TgSWWdETVYCPYlhCY
xQRMvjmuaxn9uQdSlw6TmM21mfz4DE0bU7GvrVQ+rCwIu7lX9WdAfgLlkXgNp+fT
IW5QVpGhZgL0fg0h08wVZxJgrBDdqGvTEhiYYJrOuLjJPbqJXFyD5hc9/Mdla11f
riBgpDDJp3Rfa9lrfHx+DQKBgHXu/ObWyl2sp+D9+QX1cBFaN3MZR9RBmTYdqIgm
e0k4DIY0sRSJNH7ZVEKsRmpQvOyCZcb2xiLVx/cC+261hSrkDXWFHhpUUY6UE1vY
L+s59EOctwuW3R/jZIowjKC9J5OrWNac3eQirTA9Sxm5+Uq0fSlqx35Og9Uwwvy0
AjhhAoGAPqEamBCuJ9PAXEPxgCl3SWYVfJFUf8oA2LxLhiqv+tZL9soFIydKEQnJ
G6IzlUwCnzUWtpvSsli3j+KV0DFKug6srMh5peP/4zvfP0dg5UbW5Ts1KSK0yGlA
PZjgfX70Iyke1LFgmwoh3Mq4Yicc57EBfPpH5WAGMtPlrpf1NDM=
-----END RSA PRIVATE KEY-----";
    const TEST_RSA_N: &str = "nrvlIKSb3xV5R-JTXVvNsj5cPZWRh9NsV3qfFTeT4IewGBH8MNDFlG21tb8PShKamFVgReoSp25X2-WlalGeePP7F8dV9Q7UGKQKpubKIbErYjiOlSlB-g5nle5sIlHVZkg6aAkAC1g5hrCgYLun2MpCdaQx2qc846I1RooBFrsMiubjjASrLibqcYwqbY2i3gXWh_FnCUtjqL33KG3aEoDEcuKCnarhTqnQ8XBjCye5PNIgR1TwfcXmCpfuYyx2eWp4ouJ5QI4YHiGH2YberYnCvSQdxtUOi5zP8fTE0EBc081holKqb89JD-WobwH2uM1GQ1GE-1yMWW2I2UzUIQ";
    const TEST_RSA_E: &str = "AQAB";

    fn build_test_jwks() -> GoogleJwks {
        GoogleJwks {
            keys: vec![GoogleJwk {
                kid: "test-kid-1".to_string(),
                n: TEST_RSA_N.to_string(),
                e: TEST_RSA_E.to_string(),
            }],
        }
    }

    #[derive(serde::Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        aud: &'a str,
        email: &'a str,
        email_verified: bool,
        hd: &'a str,
        name: &'a str,
        exp: usize,
    }

    fn sign_test_token(claims: &TestClaims, kid: &str) -> String {
        let encoding_key =
            jsonwebtoken::EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes())
                .expect("테스트 RSA 개인키 파싱 실패");
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(kid.to_string());
        jsonwebtoken::encode(&header, claims, &encoding_key).expect("테스트 토큰 서명 실패")
    }

    fn future_exp() -> usize {
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600) as usize
    }

    fn past_exp() -> usize {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(3600) as usize
    }

    #[test]
    fn verifies_correctly_signed_token_with_matching_claims() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");

        let verified = verify_google_id_token_signature(&token, &jwks, "test-client-id")
            .expect("정상 서명 토큰은 검증을 통과해야 합니다");
        assert_eq!(verified.email.as_deref(), Some("dev@malgnsoft.com"));
        assert_eq!(verified.hd.as_deref(), Some("malgnsoft.com"));
        assert!(check_domain_restriction(&verified, "malgnsoft.com").is_ok());
    }

    #[test]
    fn rejects_token_with_wrong_audience() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "someone-elses-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(
            result.is_err(),
            "우리 client_id가 아닌 aud는 거부되어야 합니다(다른 앱 발급 토큰 재사용 방지)"
        );
    }

    #[test]
    fn rejects_token_with_wrong_issuer() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://evil.example.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(result.is_err(), "Google이 아닌 iss는 거부되어야 합니다");
    }

    #[test]
    fn rejects_expired_token() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: past_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(result.is_err(), "만료된 토큰은 거부되어야 합니다");
    }

    #[test]
    fn rejects_token_signed_with_unknown_kid() {
        let jwks = build_test_jwks(); // kid: "test-kid-1"만 알고 있다
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "unknown-kid-999");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(
            result.is_err(),
            "JWKS에 없는 kid로 서명된 토큰은 검증할 방법이 없어 거부되어야 합니다"
        );
    }
}
