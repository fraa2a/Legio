use crate::download_resolver::{self, Provider};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct ProbeResult {
    pub uri: String,
    pub provider: Provider,
    pub available: bool,
    pub speed_bps: Option<u64>,
}

pub(crate) fn rank_candidates(results: &[ProbeResult]) -> Vec<String> {
    let mut available: Vec<_> = results.iter().filter(|p| p.available).collect();
    available.sort_by(|a, b| {
        (b.provider == Provider::Gofile)
            .cmp(&(a.provider == Provider::Gofile))
            .then_with(|| b.speed_bps.cmp(&a.speed_bps))
            .then_with(|| a.uri.cmp(&b.uri))
    });
    available.into_iter().map(|p| p.uri.clone()).collect()
}

pub(crate) async fn probe_candidates(
    client: &reqwest::Client,
    uris: &[String],
) -> Vec<ProbeResult> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut pending = uris.iter();
    let mut tasks = tokio::task::JoinSet::new();
    let mut results = Vec::new();
    loop {
        while tasks.len() < 4 {
            let Some(uri) = pending.next() else {
                break;
            };
            let uri = uri.clone();
            let client = client.clone();
            tasks.spawn(async move {
                let provider = download_resolver::classify(&uri).unwrap_or(Provider::Unsupported);
                let result =
                    tokio::time::timeout(Duration::from_secs(8), probe(&client, &uri)).await;
                ProbeResult {
                    uri,
                    provider,
                    available: matches!(result, Ok(Ok(_))),
                    speed_bps: result.ok().and_then(Result::ok).map(|(_, speed)| speed),
                }
            });
        }
        if tasks.is_empty() {
            break;
        }
        match tokio::time::timeout_at(deadline, tasks.join_next()).await {
            Ok(Some(Ok(result))) => results.push(result),
            Ok(Some(Err(_))) => {}
            _ => break,
        }
    }
    tasks.abort_all();
    results
}

pub(crate) async fn probe(client: &reqwest::Client, uri: &str) -> Result<(u64, u64), String> {
    let resolved = download_resolver::resolve(client, uri).await?;
    let _provider = resolved.provider;
    if resolved.files.len() != 1 {
        return Err(
            "This download contains multiple files and needs grouped transfer support".into(),
        );
    }
    let file = resolved
        .files
        .into_iter()
        .next()
        .ok_or("Download contains no files")?;
    let _name = file.name;
    let mut headers = file.headers;
    headers.insert(
        "range",
        reqwest::header::HeaderValue::from_static("bytes=0-262143"),
    );
    let mut response = download_resolver::get(client, &file.url, headers).await?;
    if download_resolver::is_web_page(&response) {
        return Err("The download link returned a web page".into());
    }
    let size = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok())
    } else {
        response.content_length()
    }
    .filter(|size| *size > 0)
    .ok_or("The download host did not provide an exact file size")?;
    let started = Instant::now();
    let mut bytes = 0u64;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1500);
    while bytes < 256 * 1024 {
        match tokio::time::timeout_at(deadline, response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                bytes += (chunk.len() as u64).min(256 * 1024 - bytes);
            }
            Ok(Ok(None)) => break,
            Ok(Err(error)) => return Err(error.without_url().to_string()),
            Err(_) => break,
        }
    }
    if bytes == 0 {
        return Err("The download host did not return file data".into());
    }
    Ok((
        size,
        (bytes as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn available_gofile_has_priority_and_fallback_uses_measured_throughput() {
        let probes = vec![
            ProbeResult {
                uri: "fast".into(),
                provider: Provider::Direct,
                available: true,
                speed_bps: Some(1000),
            },
            ProbeResult {
                uri: "gofile".into(),
                provider: Provider::Gofile,
                available: true,
                speed_bps: Some(10),
            },
            ProbeResult {
                uri: "slow".into(),
                provider: Provider::Direct,
                available: true,
                speed_bps: Some(100),
            },
            ProbeResult {
                uri: "offline".into(),
                provider: Provider::Gofile,
                available: false,
                speed_bps: None,
            },
        ];
        assert_eq!(rank_candidates(&probes), vec!["gofile", "fast", "slow"]);
    }
}
