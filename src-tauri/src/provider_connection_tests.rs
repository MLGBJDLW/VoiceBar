//! 连接/密钥修复回归测试（由 providers.rs 以 #[cfg(test)] #[path] 引入为 connection_tests）。
//! 约束：所有端点都是本机回环 一次夹具或纯请求构造，假密钥不出本机；
//! 只验证 WS 握手 / HTTP 端点响应与错误话术脱敏，不发送音频、不声称模型识别通过。
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const FAKE_KEY_PREFIX: &str = "fake-key";
const FAKE_SECRET_PREFIX: &str = "fake-secret";
const WS_KINDS: [&str; 5] = [
    "dashscope",
    "openai-realtime",
    "deepgram",
    "xfyun",
    "funasr",
];
const HTTP_KINDS: [&str; 2] = ["openai-compatible", "whispercpp"];

fn fake_config(kind: &str, endpoint: &str) -> ProviderConfig {
    ProviderConfig {
        kind: kind.into(),
        endpoint: endpoint.into(),
        model: "test-model".into(),
        api_key: format!("{FAKE_KEY_PREFIX}-{kind}-8f3a"),
        app_id: format!("fake-app-{kind}"),
        api_secret: format!("{FAKE_SECRET_PREFIX}-{kind}-91bc"),
        language: "zh".into(),
    }
}
fn assert_no_key_leak(message: &str) {
    assert!(!message.contains(FAKE_KEY_PREFIX), "{message}");
    assert!(!message.contains(FAKE_SECRET_PREFIX), "{message}");
}
fn request_query(kind: &str, endpoint: &str) -> (String, Vec<(String, String)>) {
    let request = websocket_request(&fake_config(kind, endpoint)).unwrap();
    let raw = request.uri().query().unwrap_or("").to_string();
    let pairs = url::form_urlencoded::parse(raw.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    (raw, pairs)
}
fn count(pairs: &[(String, String)], name: &str) -> usize {
    pairs.iter().filter(|(k, _)| k == name).count()
}
fn built_request(
    kind: &str,
    endpoint: &str,
    key: &str,
) -> tokio_tungstenite::tungstenite::http::Request<()> {
    let mut config = fake_config(kind, endpoint);
    config.api_key = key.into();
    websocket_request(&config).unwrap()
}
/// 本机夹具：读完请求头后按 status 拒绝（WS 握手拒绝与 HTTP 拒绝复用）。
async fn rejecting_endpoint(status: u16) -> String {
    let listener = tokio::net::TcpListener::bind("localhost:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _=tokio::time::timeout(Duration::from_secs(4),async{
            let(mut tcp,_)=listener.accept().await.unwrap();
            let mut seen=Vec::new();
            loop{
                let mut buf=[0u8;1024];
                match tokio::time::timeout(Duration::from_secs(2),tcp.read(&mut buf)).await{
                    Ok(Ok(n)) if n>0=>{seen.extend_from_slice(&buf[..n]);if seen.windows(4).any(|w|w==b"\r\n\r\n"){break;}}
                    _=>break,
                }
            }
            let _=tcp.write_all(format!("HTTP/1.1 {status} Rejected\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").as_bytes()).await;
        }).await;
    });
    address.to_string()
}
async fn refused_address() -> String {
    let listener = tokio::net::TcpListener::bind("localhost:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    address
}
async fn check(kind: &str, endpoint: &str) -> Result<String> {
    tokio::time::timeout(
        Duration::from_secs(5),
        check_connection(fake_config(kind, endpoint)),
    )
    .await
    .expect("check_connection 超时")
}

#[tokio::test]
async fn ws_adapters_map_rejected_handshakes_without_leaking_keys() {
    for kind in WS_KINDS {
        for (status, hint) in [(401, "鉴权失败"), (403, "拒绝访问"), (429, "限流")] {
            let endpoint = format!("ws://{}/asr", rejecting_endpoint(status).await);
            let error = check(kind, &endpoint).await.unwrap_err().to_string();
            assert!(
                error.contains(&format!("HTTP {status}")) && error.contains(hint),
                "{kind} {status}: {error}"
            );
            assert_no_key_leak(&error);
        }
    }
}

#[tokio::test]
async fn http_adapters_map_rejected_endpoints_and_reachable_whitelist() {
    for kind in HTTP_KINDS {
        for (status, hint) in [(401, "鉴权失败"), (403, "拒绝访问"), (429, "限流")] {
            let endpoint = format!("http://{}/transcribe", rejecting_endpoint(status).await);
            let error = check(kind, &endpoint).await.unwrap_err().to_string();
            assert!(
                error.contains(&format!("HTTP {status}")) && error.contains(hint),
                "{kind} {status}: {error}"
            );
            assert_no_key_leak(&error);
        }
        // 400/405/422 视为“端点已接通”，不应误报为坏 Key。
        let ok = check(
            kind,
            &format!("http://{}/transcribe", rejecting_endpoint(405).await),
        )
        .await
        .unwrap();
        assert!(
            ok.contains("HTTP 405") && ok.contains("未发送音频"),
            "{kind}: {ok}"
        );
    }
}

#[tokio::test]
async fn refused_loopback_is_not_reported_as_bad_key_for_every_kind() {
    for kind in WS_KINDS.iter().chain(HTTP_KINDS.iter()).copied() {
        let address = refused_address().await;
        let endpoint = if HTTP_KINDS.contains(&kind) {
            format!("http://{address}/transcribe")
        } else {
            format!("ws://{address}/asr")
        };
        let error = check(kind, &endpoint).await.unwrap_err().to_string();
        assert!(error.contains("本机服务未启动"), "{kind}: {error}");
        assert!(!error.contains("鉴权失败"), "{kind}: {error}");
        assert_no_key_leak(&error);
    }
}

#[test]
fn refused_remote_socket_says_key_cannot_be_judged_yet() {
    let error = crate::service_error::websocket(
        &fake_config("xfyun", "wss://iat-api.example.invalid/v2/iat"),
        &tokio_tungstenite::tungstenite::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        )),
    );
    assert!(error.contains("不能判断 Key 是否有效"), "{error}");
    assert_no_key_leak(&error);
}

#[test]
fn auth_schemes_follow_protocol_and_funasr_negotiates_binary() {
    for kind in ["dashscope", "openai-realtime"] {
        let request = built_request(
            kind,
            "wss://asr.example.invalid/v1/stream",
            &format!("{FAKE_KEY_PREFIX}-{kind}-8f3a"),
        );
        assert_eq!(
            request
                .headers()
                .get("Authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            format!("Bearer {FAKE_KEY_PREFIX}-{kind}-8f3a").as_str()
        );
    }
    let deepgram = built_request(
        "deepgram",
        "wss://api.example.invalid/v1/listen",
        &format!("{FAKE_KEY_PREFIX}-deepgram-8f3a"),
    );
    assert_eq!(
        deepgram
            .headers()
            .get("Authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Token {FAKE_KEY_PREFIX}-deepgram-8f3a").as_str()
    );
    let xfyun = built_request(
        "xfyun",
        "wss://iat-api.example.invalid/v2/iat",
        &format!("{FAKE_KEY_PREFIX}-xfyun-8f3a"),
    );
    assert!(
        xfyun.headers().get("Authorization").is_none(),
        "讯飞签名走 query，不应带 Authorization 头"
    );
    let funasr = built_request(
        "funasr",
        "ws://localhost:10095/asr",
        &format!("{FAKE_KEY_PREFIX}-funasr-8f3a"),
    );
    assert_eq!(
        funasr
            .headers()
            .get("Authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Bearer {FAKE_KEY_PREFIX}-funasr-8f3a").as_str()
    );
    assert_eq!(
        funasr
            .headers()
            .get("Sec-WebSocket-Protocol")
            .unwrap()
            .to_str()
            .unwrap(),
        "binary"
    );
    let anonymous = built_request("funasr", "ws://localhost:10095/asr", "");
    assert!(
        anonymous.headers().get("Authorization").is_none()
            && anonymous
                .headers()
                .get("Sec-WebSocket-Protocol")
                .unwrap()
                .to_str()
                .unwrap()
                == "binary"
    );
}

#[test]
fn xfyun_signature_replaces_stale_query_without_duplicates() {
    let (raw, pairs) = request_query(
        "xfyun",
        "wss://iat-api.example.invalid/v2/iat?authorization=stale&date=stale&host=stale&biz=iat",
    );
    assert_eq!(
        (
            count(&pairs, "authorization"),
            count(&pairs, "date"),
            count(&pairs, "host")
        ),
        (1, 1, 1),
        "{pairs:?}"
    );
    assert!(!raw.contains("stale"), "{raw}");
    assert!(
        pairs.iter().any(|(k, v)| k == "biz" && v == "iat")
            && pairs
                .iter()
                .any(|(k, v)| k == "host" && v == "iat-api.example.invalid"),
        "{pairs:?}"
    );
    let authorization = STANDARD
        .decode(
            pairs
                .iter()
                .find(|(k, _)| k == "authorization")
                .unwrap()
                .1
                .clone(),
        )
        .unwrap();
    let authorization = String::from_utf8(authorization).unwrap();
    assert!(
        authorization.contains("algorithm=\"hmac-sha256\"")
            && authorization.contains("headers=\"host date request-line\""),
        "{authorization}"
    );
    assert!(
        authorization.contains(&format!("api_key=\"{FAKE_KEY_PREFIX}-xfyun-8f3a\""))
            && authorization.contains("signature=\"")
            && !authorization.contains("signature=\"\""),
        "{authorization}"
    );
    assert!(!raw.contains(FAKE_SECRET_PREFIX), "{raw}");
}

#[test]
fn deepgram_query_is_rebuilt_without_stale_duplicates() {
    let (_, pairs) = request_query(
        "deepgram",
        "wss://api.example.invalid/v1/listen?model=old&language=en&tier=nova&punctuate=true",
    );
    assert_eq!(
        (count(&pairs, "model"), count(&pairs, "language")),
        (1, 1),
        "{pairs:?}"
    );
    assert!(
        pairs.iter().any(|(k, v)| k == "model" && v == "test-model")
            && pairs.iter().any(|(k, v)| k == "language" && v == "zh"),
        "{pairs:?}"
    );
    assert!(
        pairs.iter().any(|(k, v)| k == "tier" && v == "nova")
            && pairs.iter().any(|(k, v)| k == "punctuate" && v == "true"),
        "{pairs:?}"
    );
    let mut auto = fake_config(
        "deepgram",
        "wss://api.example.invalid/v1/listen?model=old&language=en",
    );
    auto.language = "auto".into();
    let request = websocket_request(&auto).unwrap();
    let pairs: Vec<(String, String)> =
        url::form_urlencoded::parse(request.uri().query().unwrap().as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    assert_eq!(
        (count(&pairs, "model"), count(&pairs, "language")),
        (1, 0),
        "{pairs:?}"
    );
}

#[test]
fn error_text_redacts_signed_query_values() {
    let message = crate::service_error::redact(
        "dial abc123 failed token xyz",
        &["other"],
        "wss://iat-api.example.invalid/v2/iat?authorization=abc123&token=xyz",
    );
    assert!(
        !message.contains("abc123") && !message.contains("xyz"),
        "{message}"
    );
    assert!(message.contains("[已隐藏]"), "{message}");
}

#[tokio::test]
async fn ws_handshake_success_reports_no_audio_was_sent() {
    let listener = tokio::net::TcpListener::bind("localhost:0").await.unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(4), async {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            while socket.next().await.is_some() {}
        })
        .await;
    });
    let ok = check("dashscope", &endpoint).await.unwrap();
    assert!(ok.contains("握手成功") && ok.contains("未发送音频"), "{ok}");
    let _ = tokio::time::timeout(Duration::from_secs(3), server).await;
}

#[tokio::test]
async fn run_surfaces_rejected_xfyun_handshake_without_secret() {
    let endpoint = format!("ws://{}/v2/iat", rejecting_endpoint(401).await);
    let (_tx, rx) = mpsc::channel(1);
    let (out, _results) = mpsc::unbounded_channel();
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        run(fake_config("xfyun", &endpoint), vec![], rx, out),
    )
    .await
    .expect("run 超时")
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("HTTP 401") && error.contains("鉴权失败"),
        "{error}"
    );
    assert_no_key_leak(&error);
}
