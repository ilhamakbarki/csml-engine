
pub fn validate_api_key(req: &actix_web::HttpRequest) -> Option<String> {
    let api_keys = match std::env::var("ENGINE_SERVER_API_KEYS") {
      Ok(val) if !val.is_empty() => val,
      _ => return None
    };

    let vec = api_keys.split(',').collect::<Vec<&str>>();

    match req.headers().get("X-Api-Key") {
      Some(val) => {
        let val = val.to_str().unwrap_or("");
        if val.is_empty() || !vec.contains(&val) {
          return Some("Invalid X-Api-Key".to_owned())
        }
        None
      },
      None => {
        Some("Missing X-Api-Key in header".to_owned())
      }
    }
}

/// Clamp a label value to a byte length APM Server will accept (limit is 1024).
/// Every span field fed from a client-supplied string must go through this: nothing
/// in this codebase validates the length of bot_id / channel_id / request_id, and a
/// single oversized attribute causes apm-server to reject the whole OTLP export, which
/// the batch span processor only reports through its internal-logs channel.
pub fn trunc(s: &str) -> &str {
    if s.len() <= 256 { return s; }
    let mut end = 256;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    &s[..end]
}
