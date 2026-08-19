//! Textübersetzung für die Audio-Übersetzung (TP3).
//!
//! Bewusst über die vorhandene Post-Process-Provider-Infrastruktur: der
//! Nutzer konfiguriert Provider/Modell/Key an einer Stelle, und „Custom"
//! zeigt per Default auf das lokale Ollama (http://localhost:11434/v1) —
//! damit bleibt auch die Übersetzung vollständig lokal möglich.

use crate::settings::AppSettings;

/// Ein Prompt, der nur die Übersetzung zurückverlangt — keine Erklärungen,
/// keine Anführungszeichen, Ton und Namen bleiben erhalten.
pub fn translation_prompt(target_lang: &str, text: &str) -> String {
    format!(
        "Translate the following text into {target_lang}. Reply with ONLY the \
         translation - no explanations, no quotation marks around it. Keep \
         names, numbers and formatting unchanged and match the tone of the \
         original.\n\n{text}"
    )
}

/// Übersetzt über den aktiven Post-Process-Provider.
pub async fn translate(
    settings: &AppSettings,
    text: &str,
    target_lang: &str,
) -> Result<String, String> {
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or_else(|| {
            "Kein Post-Processing-Provider konfiguriert (Einstellungen → Post Process)".to_string()
        })?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    if model.trim().is_empty() {
        return Err(format!(
            "Für Provider '{}' ist kein Modell konfiguriert — für lokale Übersetzung: Provider 'Custom' (Ollama) plus Modellname",
            provider.id
        ));
    }
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let prompt = translation_prompt(target_lang, text);
    // Übersetzen ist Umformen, kein Nachdenken: Reasoning aus, wo steuerbar
    // (gleiche Provider-Sonderfälle wie beim Post-Processing).
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };
    match crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        prompt,
        reasoning_effort,
        reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                Err("Übersetzung kam leer zurück".into())
            } else {
                Ok(trimmed)
            }
        }
        Ok(None) => Err("Übersetzungsantwort ohne Inhalt".into()),
        Err(e) => Err(format!("Übersetzung fehlgeschlagen: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn prompt_names_the_target_language_and_forbids_chatter() {
        let p = translation_prompt("German", "Hello world");
        assert!(p.contains("into German"));
        assert!(p.contains("ONLY the"));
        assert!(p.ends_with("Hello world"));
    }

    /// Mock eines OpenAI-kompatiblen /chat/completions-Endpunkts.
    async fn spawn_llm_mock(reply: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let mut read = 0usize;
                    loop {
                        let n = sock.read(&mut buf[read..]).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        read += n;
                        let text = String::from_utf8_lossy(&buf[..read]).to_lowercase();
                        if let Some(header_end) = text.find("\r\n\r\n") {
                            let content_length = text
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length: "))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            if read >= header_end + 4 + content_length {
                                let body = format!(
                                    r#"{{"choices":[{{"message":{{"role":"assistant","content":"{reply}"}}}}]}}"#
                                );
                                let head = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = sock.write_all(head.as_bytes()).await;
                                let _ = sock.write_all(body.as_bytes()).await;
                                let _ = sock.shutdown().await;
                                break;
                            }
                        }
                    }
                });
            }
        });
        port
    }

    fn settings_with_mock_provider(port: u16) -> crate::settings::AppSettings {
        let mut settings = get_default_settings();
        settings.post_process_provider_id = "custom".into();
        if let Some(custom) = settings.post_process_provider_mut("custom") {
            custom.base_url = format!("http://127.0.0.1:{port}/v1");
        }
        settings
            .post_process_models
            .insert("custom".into(), "test-model".into());
        settings
    }

    #[tokio::test]
    async fn translate_returns_the_models_reply() {
        let port = spawn_llm_mock("Hallo Welt").await;
        let settings = settings_with_mock_provider(port);
        let out = translate(&settings, "Hello world", "German").await.unwrap();
        assert_eq!(out, "Hallo Welt");
    }

    #[tokio::test]
    async fn translate_without_configured_model_fails_with_guidance() {
        let mut settings = get_default_settings();
        settings.post_process_provider_id = "custom".into();
        settings
            .post_process_models
            .insert("custom".into(), "".into());
        let err = translate(&settings, "Hello", "German").await.unwrap_err();
        assert!(err.contains("kein Modell konfiguriert"), "war: {err}");
        assert!(
            err.contains("Ollama"),
            "Fehlermeldung nennt den lokalen Weg"
        );
    }
}
