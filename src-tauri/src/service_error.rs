use crate::config::{is_loopback_endpoint, ProviderConfig};
use tokio_tungstenite::tungstenite::Error as WsError;

fn target(endpoint: &str) -> String {
    match url::Url::parse(endpoint) {
        Ok(url) => {
            let host = if is_loopback_endpoint(endpoint) {
                "localhost"
            } else {
                url.host_str().unwrap_or("服务端")
            };
            match url.port_or_known_default() {
                Some(port) => format!("{host}:{port}"),
                None => host.into(),
            }
        }
        Err(_) => "服务地址".into(),
    }
}
fn unavailable(name: &str, endpoint: &str) -> String {
    let address = target(endpoint);
    if is_loopback_endpoint(endpoint) {
        format!("{name} 本机服务未启动或未监听（{address}）。请先启动对应的模型服务，或在「识别引擎」切换云服务并填写该服务的 Key；VoiceBar 不内置本地识别模型。")
    } else {
        format!("无法连接 {name}（{address}）。请检查服务地址、网络、代理和防火墙；连接尚未建立，尚不能判断 Key 是否有效。")
    }
}
pub fn http_status(name: &str, status: u16) -> String {
    let action = match status {
        401 => "鉴权失败，请检查当前服务的 API Key；讯飞还需核对同一应用的 APISecret 和系统时间",
        403 => "服务拒绝访问，请检查 Key 所属地域、项目、模型权限和 IP 白名单",
        404 => "接口不存在，请检查完整服务地址和模型名称",
        400 | 422 => "请求参数不被支持，请核对识别协议、模型名称和服务地址",
        405 => "请求方法不受支持，请确认服务地址与所选协议一致",
        408 | 504 => "服务响应超时，请检查网络或稍后重试",
        429 => "请求受到限流或额度限制，请检查余额、配额并稍后重试",
        500..=599 => "服务暂时不可用，请稍后重试",
        300..=399 => "服务返回重定向，请直接填写最终接口地址",
        _ => "请求失败，请检查服务地址、协议和服务方状态",
    };
    format!("{name} 返回 HTTP {status}：{action}。")
}
pub fn websocket(config: &ProviderConfig, error: &WsError) -> String {
    let name = config.name();
    match error {
        WsError::Http(response)=>http_status(name,response.status().as_u16()),
        WsError::Io(io)=>{
            match io.kind() {
                std::io::ErrorKind::ConnectionRefused=>unavailable(name,&config.endpoint),
                std::io::ErrorKind::TimedOut=>timeout(name,&config.endpoint),
                std::io::ErrorKind::NotFound|std::io::ErrorKind::AddrNotAvailable=>format!("{name} 域名解析失败，请检查服务地址和 DNS。"),
                _=>format!("{name} 网络连接失败（{}）；请检查网络、DNS、代理和防火墙，连接失败不代表 Key 无效。",target(&config.endpoint))
            }
        }
        WsError::Tls(_)=>format!("{name} TLS 安全连接失败，请检查证书、系统时间和代理证书。"),
        WsError::Url(_)=>format!("{name} 地址无效，流式识别需要 ws:// 或 wss:// 地址。"),
        WsError::Protocol(_)=>format!("{name} WebSocket 握手或协议错误，请核对服务地址、所选识别协议和子协议。"),
        _=>format!("{name} WebSocket 连接失败，请检查网络及服务协议。")
    }
}
pub fn timeout(name: &str, endpoint: &str) -> String {
    format!(
        "连接 {name}（{}）超时，请检查服务是否启动、网络和代理。",
        target(endpoint)
    )
}
pub fn http_request(name: &str, endpoint: &str, error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return timeout(name, endpoint);
    }
    if error.is_connect() {
        return unavailable(name, endpoint);
    }
    if error.is_builder() {
        return format!("{name} 请求配置无效，请检查地址和密钥格式。");
    }
    format!("{name} HTTP 请求失败，请检查网络、服务地址和返回格式。")
}
pub fn redact(message: &str, secrets: &[&str], endpoint: &str) -> String {
    let mut text = message.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            text = text.replace(secret, "[已隐藏]");
        }
    }
    if let Ok(url) = url::Url::parse(endpoint) {
        for (name, value) in url.query_pairs() {
            if [
                "authorization",
                "api_key",
                "apikey",
                "key",
                "token",
                "access_token",
                "secret",
                "signature",
                "password",
            ]
            .contains(&name.to_ascii_lowercase().as_str())
                && !value.is_empty()
            {
                text = text.replace(value.as_ref(), "[已隐藏]");
                let encoded: String =
                    url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
                text = text.replace(&encoded, "[已隐藏]");
            }
        }
    }
    text
}
