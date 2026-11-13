/// Search URL for a track title, or `None` when there is nothing to search for.
pub fn youtube_url(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    let encoded: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("search_query", trimmed)
        .finish();

    Some(format!("https://www.youtube.com/results?{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_query() {
        let url = youtube_url("Röyksopp + Robyn").unwrap();
        assert!(url.starts_with("https://www.youtube.com/results?search_query="));
        assert!(url.contains("%2B"));
    }

    #[test]
    fn ignores_blank_text() {
        assert_eq!(youtube_url("   "), None);
    }
}
