use feed_rs::parser;
use std::io::Cursor;
use std::io::Read;
use std::time::Duration;

pub fn fetch_ticker_text(
    feeds: &[String],
    max_items_per_feed: usize,
    max_total_items: usize,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut items = Vec::new();

    for url in feeds {
        let url = url.trim();
        if url.is_empty() {
            continue;
        }

        let response = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(10))
            .timeout_write(Duration::from_secs(10))
            .build()
            .get(url)
            .set("User-Agent", "vesper/1.0 (RSS Ticker)")
            .call();

        let response = match response {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut body = String::new();
        if response.into_reader().read_to_string(&mut body).is_err() {
            continue;
        }

        let feed = match parser::parse(Cursor::new(body.as_bytes())) {
            Ok(feed) => feed,
            Err(_) => continue,
        };

        let feed_name = feed
            .title
            .as_ref()
            .map(|t| t.content.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(url);

        let mut count = 0usize;
        for entry in feed.entries {
            if count >= max_items_per_feed || items.len() >= max_total_items {
                break;
            }
            let title = entry
                .title
                .as_ref()
                .map(|t| t.content.trim())
                .filter(|s| !s.is_empty());
            let Some(title) = title else {
                continue;
            };
            items.push(format!("{feed_name}: {title}"));
            count += 1;
        }

        if items.len() >= max_total_items {
            break;
        }
    }

    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(items.join("  •  ")))
}
