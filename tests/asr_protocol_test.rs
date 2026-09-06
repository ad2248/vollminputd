//! 原生 HTTP ASR 引擎的本地协议测试。
//!
//! 使用进程内 HTTP 服务器模拟 DashScope 多模态生成服务端（不依赖真实云端、桌面或硬件），
//! 覆盖：出站鉴权/头、模型参数、音频 WAV 内容、顶层 text 与 output.text 解析、
//! 空结果、HTTP 错误/限流/畸形响应/缺失字段等行为。所有协议交互均限时 5s，失败即失败，不挂起。

use std::future::Future;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use vollminputd::asr::engine::AsrEngine;
use vollminputd::asr::{create_asr_engine, NativeHttpAsrEngine};
use vollminputd::config::Config;

const OFFLINE_KEY: &str = "offline-test-key";

/// 单次协议交互的超时上限
const INTERACT_TIMEOUT: Duration = Duration::from_secs(5);

/// 限制任意协议交互的最长等待，超时直接 panic 使测试失败而不是挂起
async fn bounded<F: Future>(fut: F) -> F::Output {
    tokio::time::timeout(INTERACT_TIMEOUT, fut)
        .await
        .expect("协议交互超过 5s 未完成")
}

fn test_pcm() -> Vec<u8> {
    (0..160u32).map(|i| (i * 7 % 251) as u8).collect()
}

fn engine(endpoint: String) -> NativeHttpAsrEngine {
    NativeHttpAsrEngine::new(OFFLINE_KEY, endpoint, "qwen-audio-3.0-asr-flash")
}

fn factory_config(endpoint: String) -> Config {
    Config {
        dashscope_api_key: OFFLINE_KEY.to_string(),
        max_recording_seconds: 60,
        audio_sample_rate: 16000,
        audio_channels: 1,
        asr_endpoint: endpoint,
        asr_model: "qwen-audio-3.0-asr-flash".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 通用断言
// ---------------------------------------------------------------------------

/// 校验请求携带的音频是标准 WAV，且 PCM 载荷与原始录音完全一致
fn assert_wav_contains_pcm(wav: &[u8], pcm: &[u8]) {
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1, "PCM 格式");
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1, "单声道");
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16000, "采样率");
    assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16, "位深");
    assert_eq!(&wav[36..40], b"data");
    let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
    assert_eq!(data_len, pcm.len());
    assert_eq!(&wav[44..44 + pcm.len()], pcm, "WAV 载荷应与原始 PCM 一致");
}

// ---------------------------------------------------------------------------
// HTTP 服务端
// ---------------------------------------------------------------------------

struct HttpCapture {
    method: String,
    path: String,
    authorization: String,
    content_type: String,
    x_dashscope_sse: String,
    body_json: serde_json::Value,
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

async fn read_http_request(stream: &mut TcpStream) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk).await.expect("读取请求头失败");
        assert!(n > 0, "连接在读取完整请求前关闭");
        buf.extend_from_slice(&chunk[..n]);
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_lowercase(), v.trim().to_string()));
        }
    }
    let content_length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);

    let body_start = header_end + 4;
    while buf.len() < body_start + content_length {
        let n = stream.read(&mut chunk).await.expect("读取请求体失败");
        assert!(n > 0, "连接在读取完整请求体前关闭");
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[body_start..body_start + content_length].to_vec();
    (method, path, headers, body)
}

/// 处理单个 HTTP 请求并返回固定响应，同时捕获请求供断言使用
async fn serve_http(listener: TcpListener, status: &str, body: String) -> HttpCapture {
    let (mut stream, _) = bounded(listener.accept()).await.expect("accept 失败");
    let (method, path, headers, req_body) = bounded(read_http_request(&mut stream)).await;

    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    bounded(stream.write_all(resp.as_bytes()))
        .await
        .expect("写响应失败");
    bounded(stream.flush()).await.expect("flush 失败");

    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let body_json: serde_json::Value = serde_json::from_slice(&req_body).expect("请求体应为 JSON");
    HttpCapture {
        method,
        path,
        authorization: header("authorization"),
        content_type: header("content-type"),
        x_dashscope_sse: header("x-dashscope-sse"),
        body_json,
    }
}

fn assert_native_request(capture: &HttpCapture, pcm: &[u8]) {
    assert_eq!(capture.method, "POST");
    assert_eq!(capture.path, "/generation");
    assert_eq!(capture.authorization, format!("Bearer {OFFLINE_KEY}"), "出站必须带 Bearer 鉴权");
    assert_eq!(capture.content_type, "application/json");
    assert_eq!(capture.x_dashscope_sse, "disable");

    assert_eq!(capture.body_json["model"], "qwen-audio-3.0-asr-flash");
    assert_eq!(capture.body_json["parameters"]["format"], "wav");
    assert_eq!(capture.body_json["parameters"]["sample_rate"], "16000");

    let content = capture.body_json["input"]["messages"][0]["content"]
        .as_array()
        .expect("content 应为数组");
    assert_eq!(capture.body_json["input"]["messages"][0]["role"], "user");
    assert_eq!(content.len(), 1, "应只有音频内容块");
    let data_url = content[0]["input_audio"]["data"].as_str().expect("data url");
    assert_eq!(content[0]["type"], "input_audio");
    let b64 = data_url.strip_prefix("data:audio/wav;base64,").expect("data url 前缀");
    let wav = BASE64.decode(b64).expect("合法 base64");
    assert_wav_contains_pcm(&wav, pcm);
}

#[tokio::test]
async fn test_native_posts_wav_and_returns_top_level_text() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let pcm = test_pcm();
    let response = serde_json::json!({
        "text": "明天天气很好。",
        "output": { "text": "明天天气很好。" },
        "sentence_end_time": 1234,
        "request_id": "req-001",
        "usage": { "input_tokens": 1, "output_tokens": 1 }
    });
    let server = tokio::spawn(serve_http(listener, "200 OK", response.to_string()));

    let engine = create_asr_engine(&factory_config(format!("http://{addr}/generation")));
    let result = bounded(engine.recognize(&pcm)).await.unwrap();
    assert_eq!(result, "明天天气很好。");

    let capture = bounded(server).await.unwrap();
    assert_native_request(&capture, &pcm);
}

#[tokio::test]
async fn test_native_output_text_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let response = serde_json::json!({
        "output": { "text": "回退文本" },
        "request_id": "req-002"
    });
    let server = tokio::spawn(serve_http(listener, "200 OK", response.to_string()));

    let result = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm()))
        .await
        .unwrap();
    assert_eq!(result, "回退文本");
    bounded(server).await.unwrap();
}

#[tokio::test]
async fn test_native_empty_text_is_empty_success() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let response = serde_json::json!({ "text": "", "output": { "text": "" } });
    let server = tokio::spawn(serve_http(listener, "200 OK", response.to_string()));

    let result = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm()))
        .await
        .unwrap();
    assert!(result.is_empty(), "空转录应返回 Ok(\"\") 以与应用失败路径一致");
    bounded(server).await.unwrap();
}

#[tokio::test]
async fn test_native_http_401_is_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(serve_http(
        listener,
        "401 Unauthorized",
        r#"{"error":{"message":"invalid key"}}"#.to_string(),
    ));

    let err = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm()))
        .await
        .expect_err("401 应返回错误");
    let msg = err.to_string();
    assert!(msg.contains("401"), "错误应包含状态码: {msg}");
    assert!(msg.contains("invalid key"), "错误应包含响应体: {msg}");
    bounded(server).await.unwrap();
}

#[tokio::test]
async fn test_native_http_rate_limit_and_unavailable_are_errors() {
    for (code, status, body) in [
        ("429", "429 Too Many Requests", "rate limited"),
        ("503", "503 Service Unavailable", "server busy"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let payload = format!(r#"{{"error":{{"message":"{body}"}}}}"#);
        let server = tokio::spawn(serve_http(listener, status, payload));

        let err = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm()))
            .await
            .expect_err("非 2xx 应返回错误");
        let msg = err.to_string();
        assert!(msg.contains(code), "错误应包含状态码: {msg}");
        assert!(msg.contains(body), "错误应包含响应体: {msg}");
        bounded(server).await.unwrap();
    }
}

#[tokio::test]
async fn test_native_malformed_json_response_is_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(serve_http(listener, "200 OK", "not json at all".to_string()));

    let err = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm()))
        .await
        .expect_err("畸形响应应返回错误");
    assert!(err.to_string().contains("解析"), "错误应说明解析失败: {}", err);
    bounded(server).await.unwrap();
}

#[tokio::test]
async fn test_native_missing_text_schema_is_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(serve_http(listener, "200 OK", "{}".to_string()));

    let err = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm()))
        .await
        .expect_err("缺少文本字段的响应应返回错误");
    assert!(err.to_string().contains("文本字段"), "{}", err);
    bounded(server).await.unwrap();
}

#[tokio::test]
async fn test_native_connect_refused_is_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener); // 端口无人监听 → 连接被拒绝

    let result = bounded(engine(format!("http://{addr}/generation")).recognize(&test_pcm())).await;
    assert!(result.is_err(), "连接失败应返回错误");
}
