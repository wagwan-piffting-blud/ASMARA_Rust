use crate::filter;
use crate::state::ActiveAlert;
use crate::Config;
use base64::Engine;
use chrono::Local;
use inflector::Inflector;
use lazy_static::lazy_static;
use reqwest::header::AUTHORIZATION;
use reqwest::{multipart, Client};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use tokio::process::Command;
use tracing::{info, warn};

lazy_static! {
    static ref json_config: Config =
        Config::from_config_json("/app/config.json").expect("Failed to load config");
    static ref station_name: String = json_config.eas_relay_name.clone();
    static ref STREAM_INDEX_MAP: HashMap<String, usize> = json_config
        .icecast_stream_urls
        .iter()
        .enumerate()
        .map(|(idx, url)| (url.clone(), idx + 1))
        .collect();
}

pub async fn send_alert_webhook(
    url: &str,
    alert: &ActiveAlert,
    _dsame_text: &str,
    _raw_header: &str,
    recording_path: Option<PathBuf>,
) {
    let config_path = json_config.apprise_config_path.to_string();
    let apprise_urls_from_config_array: Vec<String> = match fs::File::open(&config_path) {
        Ok(mut file) => {
            let mut contents = String::new();
            if let Err(err) = file.read_to_string(&mut contents) {
                warn!(
                    "Failed to read AppRise config file at '{}': {}",
                    config_path, err
                );
                return;
            }
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(|line| {
                    line.strip_prefix('-')
                        .map(str::trim_start)
                        .unwrap_or(line)
                        .to_owned()
                })
                .collect()
        }
        Err(err) => {
            warn!(
                "Failed to open AppRise config file at '{}': {}",
                config_path, err
            );
            return;
        }
    };
    let should_relay_dasdec = json_config.should_relay_dasdec;
    let dasdec_url = json_config.dasdec_url.clone();
    let data = &alert.data;
    let event_title = data.event_text.to_title_case();
    let apprise_title = format!("{} has just been issued/received", event_title.as_str());
    let received_timestamp = Local::now().to_rfc3339();
    let attachment_path = if let Some(path) = recording_path {
        match tokio::fs::metadata(&path).await {
            Ok(_) => Some(path),
            Err(err) => {
                warn!(
                    "Recording attachment unavailable at '{}': {}",
                    path.display(),
                    err
                );
                None
            }
        }
    } else {
        None
    };
    let discord_embed_body = build_discord_embed_body(
        &url,
        &event_title,
        &data.originator,
        &received_timestamp,
        &data.eas_text,
        &alert.raw_header,
    );
    let markdown_body = build_markdown_body(
        &event_title,
        &data.originator,
        &received_timestamp,
        &data.eas_text,
        &alert.raw_header,
    );
    let html_body = build_html_body(
        &event_title,
        &data.originator,
        &received_timestamp,
        &data.eas_text,
        &alert.raw_header,
    );
    let text_body = build_plain_body(
        &event_title,
        &data.originator,
        &received_timestamp,
        &data.eas_text,
        &alert.raw_header,
    );

    let filter_relay = filter::should_relay_alert(&alert.raw_header[9..12]);

    if should_relay_dasdec && !dasdec_url.trim().is_empty() && filter_relay {
        let client = Client::new();

        let use_reverse_proxy = json_config.use_reverse_proxy;

        let latest_url = format!(
            "http{}://{}:{}/archive.php?latest_id=true",
            if use_reverse_proxy { "s" } else { "" },
            if use_reverse_proxy {
                json_config.reverse_proxy_url.clone()
            } else {
                json_config.monitoring_bind_host.clone()
            },
            if use_reverse_proxy {
                "443".to_string()
            } else {
                json_config.web_server_port.clone()
            }
        );

        let bearer_token =
            json_config.dashboard_username.clone() + ":" + &json_config.dashboard_password.clone();
        let bearer_token = Engine::encode(&base64::engine::general_purpose::STANDARD, bearer_token);

        let latest_id = match client
            .get(&latest_url)
            .header(AUTHORIZATION, format!("Bearer {}", bearer_token))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(text) => text.trim().to_string(),
                Err(err) => {
                    warn!("Failed to read latest ID response body: {}", err);
                    "0".to_string()
                }
            },
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                warn!(
                    "Failed to fetch latest recording ID with status {}: body='{}'",
                    status, body
                );
                "0".to_string()
            }
            Err(err) => {
                warn!("Failed to send latest ID request: {}", err);
                "0".to_string()
            }
        };

        let audio_deeplink = {
            format!(
                "http{}://{}:{}/archive.php?recording_id={}",
                if use_reverse_proxy { "s" } else { "" },
                if use_reverse_proxy {
                    json_config.reverse_proxy_url.clone()
                } else {
                    json_config.monitoring_bind_host.clone()
                },
                if use_reverse_proxy {
                    "443".to_string()
                } else {
                    json_config.web_server_port.clone()
                },
                latest_id
            )
        };

        let blank_string = String::new();

        let dasdec_payload = [
            ("eas_header", &alert.raw_header),
            ("description", &blank_string),
            ("audio_deeplink", &audio_deeplink),
        ];

        match client.post(&dasdec_url).form(&dasdec_payload).send().await {
            Ok(response) if response.status().is_success() => {
                info!("Successfully relayed alert to DASDEC");
            }
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                warn!(
                    "DASDEC relay failed with status {}: body='{}'",
                    status, body
                );
            }
            Err(err) => {
                warn!("Failed to send DASDEC relay request: {}", err);
            }
        }
    }

    let discord_urls: Vec<&str> = apprise_urls_from_config_array
        .iter()
        .map(|url| url.trim())
        .filter(|url| url.starts_with("discord://"))
        .collect();

    if !discord_urls.is_empty() {
        let client = Client::new();
        let attachment_bytes = if let Some(path) = attachment_path.as_ref() {
            match tokio::fs::read(path).await {
                Ok(bytes) => Some(bytes),
                Err(err) => {
                    warn!(
                        "Failed to read recording attachment at '{}': {}",
                        path.display(),
                        err
                    );
                    None
                }
            }
        } else {
            None
        };

        for discord_url in discord_urls {
            let payload_json = json!({ "embeds": [discord_embed_body.clone()] }).to_string();
            let mut form = multipart::Form::new().text("payload_json", payload_json);

            if let (Some(path), Some(bytes)) = (attachment_path.as_ref(), attachment_bytes.as_ref())
            {
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "recording.bin".to_string());

                match multipart::Part::bytes(bytes.clone())
                    .file_name(file_name)
                    .mime_str("application/octet-stream")
                {
                    Ok(part) => {
                        form = form.part("file", part);
                    }
                    Err(err) => {
                        warn!(
                            "Failed to prepare Discord attachment part for '{}': {}",
                            path.display(),
                            err
                        );
                    }
                }
            }

            let url = format!(
                "https://discord.com/api/webhooks/{}",
                discord_url.trim_start_matches("discord://")
            );

            match client.post(&url).multipart(form).send().await {
                Ok(response) if response.status().is_success() => {}
                Ok(response) => {
                    warn!(
                        "Discord webhook responded with status {} for '{}'",
                        response.status(),
                        discord_url
                    );
                }
                Err(e) => {
                    warn!("Failed to send Discord webhook '{}': {}", discord_url, e);
                }
            }
        }

        return;
    }

    let attempts = [
        ("markdown", markdown_body),
        ("html", html_body),
        ("text", text_body),
    ];

    for (format, body) in attempts.iter() {
        let mut command = Command::new("apprise");
        command.arg("--config").arg(&config_path);
        command.arg("--title").arg(&apprise_title);
        command.arg("--body").arg(body);
        command.arg("--input-format").arg(format);

        if let Some(path) = attachment_path.as_ref() {
            command.arg("--attach").arg(path);
        }

        match command.output().await {
            Ok(output) if output.status.success() => {
                return;
            }
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(
                    "AppRise CLI failed with format {} (exit code {:?}): stdout='{}' stderr='{}'",
                    format,
                    output.status.code(),
                    stdout.trim(),
                    stderr.trim()
                );
            }
            Err(err) => {
                warn!(
                    "Failed to invoke AppRise CLI with format {}: {}",
                    format, err
                );
            }
        }
    }

    warn!("Unable to deliver notification via AppRise after trying all formats");
}

fn build_discord_embed_body(
    stream_id: &str,
    title: &str,
    originator: &str,
    received_timestamp: &str,
    eas_text: &str,
    raw_header: &str,
) -> serde_json::Value {
    let monitor_number = STREAM_INDEX_MAP.get(stream_id).copied().unwrap_or(999);
    let event_code = raw_header[9..12]
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>();
    let filter_name = filter::determine_filter_name(&event_code);

    let img_name = if !raw_header.is_empty() && raw_header.len() >= 12 {
        &event_code
    } else {
        "ZZZ"
    };

    let img_color = if title.to_lowercase().contains("test") {
        "105733"
    } else if title.to_lowercase().contains("advisory") || title.to_lowercase().contains("watch") {
        "FFFF00"
    } else if title.to_lowercase().contains("warning") || title.to_lowercase().contains("emergency")
    {
        "FF0000"
    } else {
        "808080"
    };

    let img_color_dec = u32::from_str_radix(img_color, 16);

    let embed = json!({
        "title": format!("{} has just been issued/received.", title),
        "color": match img_color_dec {
            Ok(value) => format!("{}", value),
            Err(error) => format!("Error during parsing: {}", error),
        },
        "author": {
            "name": format!("{} - Software ENDEC Logs", station_name.as_str()),
            "icon_url": format!("https://wagspuzzle.space/assets/eas-icons/index.php?code={}&hex=0x{}", img_name, img_color),
            "url": "https://github.com/wagwan-piffting-blud/ASMARA_Rust"
        },
        "fields": [
            {
                "name": "Received From:",
                "value": originator,
                "inline": false
            },
            {
                "name": "Received At:",
                "value": received_timestamp,
                "inline": false
            },
            {
                "name": "Monitor",
                "value": format!("#{}", monitor_number),
                "inline": true
            },
            {
                "name": "Filter",
                "value": filter_name,
                "inline": true
            },
            {
                "name": "EAS Text Data:",
                "value": format!("```\n{}\n```", eas_text.trim_end()),
                "inline": false
            },
            {
                "name": "EAS Protocol Data:",
                "value": format!("```\n{}\n```", raw_header.trim_end()),
                "inline": false
            }
        ]
    });

    return embed;
}

fn build_markdown_body(
    title: &str,
    originator: &str,
    received_timestamp: &str,
    eas_text: &str,
    raw_header: &str,
) -> String {
    format!(
        "**{} - Software ENDEC Logs**\n\n**{}** has just been received from: {}\n\n**Received:** {}\n\n**EAS Text Data:**\n```\n{}\n```\n\n**EAS Protocol Data:**\n```\n{}\n```\n\nPowered by [Wags' Software ENDEC](https://github.com/wagwan-piffting-blud/ASMARA_Rust)",
        station_name.as_str(),
        title,
        originator,
        received_timestamp,
        eas_text.trim_end(),
        raw_header.trim_end()
    )
}

fn build_html_body(
    title: &str,
    originator: &str,
    received_timestamp: &str,
    eas_text: &str,
    raw_header: &str,
) -> String {
    format!(
        "<p><strong>{} - Software ENDEC Logs</strong></p>\
         <p><strong>{}</strong> has just been received from: {}</p>\
         <p><strong>Received:</strong> {}</p>\
         <p><strong>EAS Text Data:</strong></p>\
         <pre>{}</pre>\
         <p><strong>EAS Protocol Data:</strong></p>\
         <pre>{}</pre>\
         <p>Powered by <a href=\"https://github.com/wagwan-piffting-blud/ASMARA_Rust\">Wags' Software ENDEC</a></p>",
        html_escape(&station_name.as_str()),
        html_escape(title),
        html_escape(originator),
        html_escape(received_timestamp),
        html_escape(eas_text.trim_end()),
        html_escape(raw_header.trim_end())
    )
}

fn build_plain_body(
    title: &str,
    originator: &str,
    received_timestamp: &str,
    eas_text: &str,
    raw_header: &str,
) -> String {
    format!(
        "{} - Software ENDEC Logs\n\n{} has just been received from: {}\nReceived: {}\n\nEAS Text Data:\n{}\n\nEAS Protocol Data:\n{}\n\nPowered by Wags' Software ENDEC (https://github.com/wagwan-piffting-blud/ASMARA_Rust)",
        station_name.as_str(),
        title,
        originator,
        received_timestamp,
        eas_text.trim_end(),
        raw_header.trim_end()
    )
}

fn html_escape(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
