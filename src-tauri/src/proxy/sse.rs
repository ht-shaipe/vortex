use actix_web::web::Bytes;
use futures::StreamExt;
use reqwest::Response;

pub async fn stream_sse_response(
    upstream_response: Response,
    source_format: &str,
) -> actix_web::HttpResponse {
    let content_type = upstream_response.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/event-stream")
        .to_string();

    match source_format {
        "anthropic" => stream_anthropic_as_openai(upstream_response).await,
        "gemini" => stream_gemini_as_openai(upstream_response).await,
        _ => {
            let stream = upstream_response.bytes_stream().map(|result| {
                match result {
                    Ok(bytes) => Ok::<actix_web::web::Bytes, actix_web::Error>(Bytes::from(bytes)),
                    Err(_) => Ok::<actix_web::web::Bytes, actix_web::Error>(Bytes::from("data: [DONE]\n\n")),
                }
            });

            actix_web::HttpResponse::Ok()
                .insert_header((actix_web::http::header::CONTENT_TYPE, content_type))
                .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
                .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
                .streaming(stream)
        }
    }
}

async fn stream_anthropic_as_openai(upstream: Response) -> actix_web::HttpResponse {
    let stream = upstream.bytes_stream().scan(String::new(), |buffer, result| {
        let bytes = match result {
            Ok(b) => b,
            Err(_) => {
                return std::future::ready(Some(Ok::<Bytes, actix_web::Error>(
                    Bytes::from("data: [DONE]\n\n"),
                )));
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&bytes));
        let mut output = String::new();

        while let Some(pos) = buffer.find("\n\n") {
            let chunk = buffer[..pos].to_string();
            buffer.drain(..pos + 2);

            for line in chunk.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        output.push_str("data: [DONE]\n\n");
                        continue;
                    }
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
                    if let Ok(event) = parsed {
                        if let Some(event_type) = event.get("type").and_then(|t| t.as_str()) {
                            match event_type {
                                "message_start" => {
                                    if let Some(msg) = event.get("message") {
                                        let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("chatcmpl-0");
                                        let model = msg.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");
                                        let openai_chunk = serde_json::json!({
                                            "id": id,
                                            "object": "chat.completion.chunk",
                                            "created": chrono::Utc::now().timestamp(),
                                            "model": model,
                                            "choices": [{"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": null}]
                                        });
                                        output.push_str(&format!("data: {}\n\n", openai_chunk));
                                    }
                                }
                                "content_block_delta" => {
                                    if let Some(delta) = event.get("delta") {
                                        if let Some(text) = delta.get("text") {
                                            let content = text.as_str().unwrap_or("");
                                            let openai_chunk = serde_json::json!({
                                                "object": "chat.completion.chunk",
                                                "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}]
                                            });
                                            output.push_str(&format!("data: {}\n\n", openai_chunk));
                                        }
                                    }
                                }
                                "message_delta" => {
                                    if let Some(delta) = event.get("delta") {
                                        let stop_reason = delta.get("stop_reason").and_then(|v| v.as_str()).unwrap_or("stop");
                                        let reason = if stop_reason == "end_turn" { "stop" } else { stop_reason };
                                        let openai_chunk = serde_json::json!({
                                            "object": "chat.completion.chunk",
                                            "choices": [{"index": 0, "delta": {}, "finish_reason": reason}]
                                        });
                                        output.push_str(&format!("data: {}\n\n", openai_chunk));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        if output.is_empty() {
            std::future::ready(None)
        } else {
            std::future::ready(Some(Ok::<Bytes, actix_web::Error>(Bytes::from(output))))
        }
    }).filter_map(|r| async move {
        match r {
            Ok(bytes) => Some(Ok::<Bytes, actix_web::Error>(bytes)),
            Err(e) => Some(Err::<Bytes, actix_web::Error>(e)),
        }
    });

    actix_web::HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .streaming(stream)
}

async fn stream_gemini_as_openai(upstream: Response) -> actix_web::HttpResponse {
    let stream = upstream.bytes_stream().scan(String::new(), |buffer, result| {
        let bytes = match result {
            Ok(b) => b,
            Err(_) => {
                return std::future::ready(Some(Ok::<Bytes, actix_web::Error>(
                    Bytes::from("data: [DONE]\n\n"),
                )));
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&bytes));
        let mut output = String::new();

        while let Some(pos) = buffer.find("\n") {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..pos + 1);

            if line.is_empty() {
                continue;
            }

            let json_str = if line.starts_with('"') && line.ends_with('"') && line.len() > 2 {
                let inner = &line[1..line.len()-1];
                inner.replace("\\\"", "\"").replace("\\\\", "\\")
            } else {
                line
            };

            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
            if let Ok(event) = parsed {
                if let Some(candidates) = event.get("candidates").and_then(|c| c.as_array()) {
                    for candidate in candidates {
                        let finish_reason = candidate.get("finishReason")
                            .and_then(|f| f.as_str())
                            .map(|r| if r == "STOP" { "stop" } else { r })
                            .unwrap_or("null");

                        if let Some(parts) = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                            for part in parts {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    let openai_chunk = serde_json::json!({
                                        "object": "chat.completion.chunk",
                                        "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": finish_reason}]
                                    });
                                    output.push_str(&format!("data: {}\n\n", openai_chunk));
                                }
                            }
                        }
                    }
                }
            }
        }

        if output.is_empty() {
            std::future::ready(None)
        } else {
            output.push_str("data: [DONE]\n\n");
            std::future::ready(Some(Ok::<Bytes, actix_web::Error>(Bytes::from(output))))
        }
    }).filter_map(|r| async move {
        match r {
            Ok(bytes) => Some(Ok::<Bytes, actix_web::Error>(bytes)),
            Err(e) => Some(Err::<Bytes, actix_web::Error>(e)),
        }
    });

    actix_web::HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .streaming(stream)
}
