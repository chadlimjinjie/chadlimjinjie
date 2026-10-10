use http_body_util::BodyExt;
use serde_json::{Value, json};
use vercel_runtime::{Error, Request, Response, run, service_fn};

const KEY: &str = "location:latest";

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(service_fn(handler)).await
}

async fn handler(req: Request) -> Result<Response<Value>, Error> {
    match req.method().as_str() {
        "GET" => {
            let stored = redis(json!(["GET", KEY])).await?;
            let point = match stored.as_str() {
                Some(s) => serde_json::from_str(s)?,
                None => Value::Null,
            };
            Ok(Response::builder()
                .header("cache-control", "public, s-maxage=30")
                .body(point)?)
        }
        "POST" => {
            let token = std::env::var("LOCATION_TOKEN")?;
            let header = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !authorized(header, &token) {
                return respond(401, json!({ "error": "unauthorized" }));
            }
            let bytes = req.into_body().collect().await?.to_bytes();
            let Ok(body) = serde_json::from_slice::<Value>(&bytes) else {
                return respond(400, json!({ "error": "invalid json" }));
            };
            if let Some(point) = latest_point(&body) {
                redis(json!(["SET", KEY, point.to_string()])).await?;
            }
            // Overland only drops the batch from its queue on exactly this body
            respond(200, json!({ "result": "ok" }))
        }
        _ => respond(405, json!({ "error": "method not allowed" })),
    }
}

fn respond(status: u16, body: Value) -> Result<Response<Value>, Error> {
    Ok(Response::builder().status(status).body(body)?)
}

fn authorized(header: &str, token: &str) -> bool {
    let Some(given) = header.strip_prefix("Bearer ") else {
        return false;
    };
    // constant-time compare so the token can't be guessed byte by byte from timing
    !token.is_empty()
        && given.len() == token.len()
        && given.bytes().zip(token.bytes()).fold(0, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Newest point of an Overland batch, reduced to what the site shows.
/// Overland sends its queue oldest-first, so the last feature is the newest.
fn latest_point(body: &Value) -> Option<Value> {
    let feature = body["locations"].as_array()?.last()?;
    // GeoJSON order is [longitude, latitude]
    let lon = feature["geometry"]["coordinates"][0].as_f64()?;
    let lat = feature["geometry"]["coordinates"][1].as_f64()?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let timestamp = feature["properties"]["timestamp"].as_str()?;
    Some(json!({ "lat": lat, "lon": lon, "timestamp": timestamp }))
}

/// Runs one command against the Upstash Redis REST API and returns its result.
async fn redis(command: Value) -> Result<Value, Error> {
    let res: Value = reqwest::Client::new()
        .post(std::env::var("KV_REST_API_URL")?)
        .bearer_auth(std::env::var("KV_REST_API_TOKEN")?)
        .json(&command)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(res["result"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature(lon: f64, lat: f64, timestamp: &str) -> Value {
        json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [lon, lat] },
            "properties": { "timestamp": timestamp, "wifi": "home-ssid", "battery_level": 0.8 }
        })
    }

    #[test]
    fn picks_last_point_and_keeps_only_position() {
        let body = json!({ "locations": [
            feature(-122.03, 37.33, "2026-10-10T08:00:00Z"),
            feature(103.8198, 1.3521, "2026-10-10T08:05:00Z"),
        ]});
        assert_eq!(
            latest_point(&body),
            Some(json!({ "lat": 1.3521, "lon": 103.8198, "timestamp": "2026-10-10T08:05:00Z" }))
        );
    }

    #[test]
    fn ignores_empty_and_invalid_batches() {
        assert_eq!(latest_point(&json!({ "locations": [] })), None);
        assert_eq!(latest_point(&json!({})), None);
        assert_eq!(latest_point(&json!({ "locations": [feature(10.0, 123.0, "t")] })), None);
    }

    #[test]
    fn checks_bearer_token() {
        assert!(authorized("Bearer secret", "secret"));
        assert!(!authorized("Bearer wrong!", "secret"));
        assert!(!authorized("secret", "secret"));
        assert!(!authorized("Bearer ", ""));
    }
}
