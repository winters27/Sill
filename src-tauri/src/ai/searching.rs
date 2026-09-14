//! Searching the web, through an instance somebody runs themselves.
//!
//! Sill has no search account and asks no search company for anything. The
//! address of a [SearXNG](https://docs.searxng.org) instance goes in settings,
//! the query goes to that address, and the results come back as JSON. A
//! machine on the LAN is the expected answer; a public instance works and is
//! nobody's business but the person who typed the address.
//!
//! **Blank is off, not broken.** With no address the tool answers with a
//! sentence saying where to set one, which the model reads out. That is the
//! same shape every other tool here uses for a thing it cannot do: a fact
//! about the machine, carried back into the turn, rather than a failure that
//! ends it.
//!
//! ## What comes back is not trustworthy
//!
//! Every other tool in this module reads something the person already has.
//! This one reads text written by strangers and hands it to a model that can
//! reach for `run_action`. Nothing here can make that text safe, and nothing
//! here tries to: what stands between a web page and this machine is the
//! approval card, and Windows Hello in front of running a command or writing a
//! file. Both are on the far side of the model and neither is weakened by
//! this. It is worth knowing the surface got wider.
//!
//! ## What it costs at rest: nothing
//!
//! One request per call, and calls only happen inside a turn.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

/// Long enough for several engines to answer, short enough that a box that is
/// off does not hold a turn open. SearXNG queries engines in parallel and
/// gives up on the slow ones itself.
const PATIENCE: Duration = Duration::from_secs(12);

/// How much of the answer is worth paying for on every later request.
///
/// A search is thirty results of a hundred words each, and all of it lands in
/// the conversation and is billed again on every step after it. Eight results
/// is what a question actually needs, and the summary is cut to a sentence or
/// two rather than the page's whole preview.
const MOST_RESULTS: usize = 8;
const MOST_SUMMARY: usize = 400;

/// What says a summary was cut, rather than being this short.
const ELLIPSIS: char = '\u{2026}';

/// One result, as SearXNG hands it over.
#[derive(Deserialize)]
struct Found {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default, rename = "publishedDate")]
    published: Option<String>,
}

/// The envelope. Every field is optional because the shape has moved between
/// releases and an instance somebody upgraded should not stop working.
#[derive(Deserialize)]
struct Answered {
    #[serde(default)]
    results: Vec<Found>,
    #[serde(default)]
    answers: Vec<Value>,
    #[serde(default)]
    unresponsive_engines: Vec<Value>,
}

/// The address to ask, with its trailing slash sorted out.
///
/// Held for one field read and let go, the same as the Hello gate: settings
/// are behind an async lock and the request below waits seconds.
async fn instance(app: &AppHandle) -> Option<String> {
    let prefs = app.try_state::<crate::state::PrefsState>()?;
    let typed = prefs.inner.lock().await.ai.search.clone();

    addressed(&typed)
}

/// What somebody typed, turned into something reqwest can use.
///
/// A bare host is what people type, and without a scheme the request
/// reaches nothing and the error blames the network. A trailing slash is
/// the other half: pasted from a browser it arrives with one, and
/// `{address}/search` would then ask for `//search`.
fn addressed(typed: &str) -> Option<String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }

    let with_scheme = if typed.contains("://") {
        typed.to_string()
    } else {
        format!("http://{typed}")
    };

    // A scheme and nothing else is somebody halfway through typing. Asked
    // before the slashes are trimmed, because trimming them turns
    // `http://` into `http:`, which no longer looks like a bare scheme and
    // sails past a check made afterwards.
    let host = with_scheme.split_once("://").map_or("", |(_, rest)| rest);
    if host.trim_matches('/').is_empty() {
        return None;
    }

    Some(with_scheme.trim_end_matches('/').to_string())
}

/// An answer SearXNG worked out itself, which is worth more than ten links.
///
/// The shape changed: it used to be a list of strings and is now a list of
/// objects with the sentence under `answer`. Both are read, because an
/// instance is somebody else's to upgrade.
fn plainly(answer: &Value) -> Option<String> {
    let text = match answer {
        Value::String(said) => said.clone(),
        Value::Object(_) => answer.get("answer")?.as_str()?.to_string(),
        _ => return None,
    };

    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Cuts a summary to length on a word, so it does not end mid-word.
fn shortened(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MOST_SUMMARY {
        return text;
    }

    let mut kept = String::new();
    for word in text.split(' ') {
        if kept.chars().count() + word.chars().count() + 1 > MOST_SUMMARY {
            break;
        }
        if !kept.is_empty() {
            kept.push(' ');
        }
        kept.push_str(word);
    }

    kept.push(ELLIPSIS);
    kept
}

/// Searches, and answers with what came back.
///
/// Never `Err`, like every tool in this module. A box that is off, an address
/// that is wrong and a query with nothing in it are all facts the turn should
/// continue with.
pub async fn run(app: &AppHandle, query: &str) -> Value {
    if query.is_empty() {
        return json!({ "error": "Give it something to search for." });
    }

    let Some(address) = instance(app).await else {
        return json!({
            "error": "No web search is set up. There is a field for the address of a \
                      SearXNG instance in Settings, AI, under The web."
        });
    };

    ask(&address, query).await
}

/// The search itself, with the address already worked out.
///
/// Split from `run` because everything here is reachable without Tauri, and
/// a request, a status code and somebody else's JSON are exactly the parts
/// worth pointing at a real instance. See the ignored test at the bottom of
/// this file for how.
pub async fn ask(address: &str, query: &str) -> Value {
    let client = match reqwest::Client::builder().timeout(PATIENCE).build() {
        Ok(client) => client,
        Err(err) => return json!({ "error": format!("Could not prepare the request: {err}") }),
    };

    // The query string is built rather than handed to reqwest's `query`,
    // which 0.13 puts behind a feature this build does not carry. The encoder
    // is the one the launcher's own web search uses, so a query with a space
    // or an ampersand in it travels the same way from both.
    let asked = client
        .get(format!(
            "{address}/search?q={}&format=json",
            crate::quicklinks::resolve::percent_encode(query)
        ))
        .send()
        .await;

    let answered = match asked {
        Ok(answered) => answered,
        Err(err) => {
            return json!({
                "error": format!(
                    "Could not reach the search instance at {address}: {err}. It may be off."
                )
            })
        }
    };

    let status = answered.status();
    if !status.is_success() {
        // 403 is the one worth naming. SearXNG ships without `json` in its
        // `search.formats`, and asking for it then answers 403 with no body
        // and no log line, which reads exactly like a firewall.
        let why = if status == reqwest::StatusCode::FORBIDDEN {
            " That is what it answers when json is missing from search.formats \
              in its settings.yml, or when its limiter is on."
        } else {
            ""
        };

        return json!({ "error": format!("The search instance answered {status}.{why}") });
    }

    let body = match answered.json::<Answered>().await {
        Ok(body) => body,
        Err(err) => {
            return json!({
                "error": format!("The search instance did not answer with JSON: {err}")
            })
        }
    };

    let results: Vec<Value> = body
        .results
        .iter()
        .filter(|found| !found.url.is_empty())
        .take(MOST_RESULTS)
        .map(|found| {
            let mut one = json!({
                "title": found.title,
                "url": found.url,
                "summary": shortened(&found.content),
            });

            if let Some(published) = found.published.as_deref().filter(|at| !at.is_empty()) {
                one["published"] = json!(published);
            }

            one
        })
        .collect();

    let answers: Vec<String> = body.answers.iter().filter_map(plainly).collect();

    if results.is_empty() && answers.is_empty() {
        return json!({
            "query": query,
            "results": [],
            "note": "Nothing came back for that. A different wording may find it."
        });
    }

    let mut answer = json!({ "query": query, "results": results });

    if !answers.is_empty() {
        answer["answers"] = json!(answers);
    }

    // Said out loud, because a thin answer with half the engines down is a
    // different thing from a thin answer, and only one of them is worth
    // rewording the query over.
    if !body.unresponsive_engines.is_empty() {
        answer["note"] = json!(format!(
            "{} of the search engines did not answer, so this may be less than the whole picture.",
            body.unresponsive_engines.len()
        ));
    }

    answer
}

#[cfg(test)]
mod tests {
    use super::*;

/// Two results, copied out of what a real instance actually sent.
    ///
    /// Hand-written fixtures agree with whatever the struct says, which is
    /// the one thing that cannot go wrong here. What can is a field spelt
    /// the way the docs spell it and not the way the server does, and only
    /// a real answer catches that. Captured 2026-09-12 from SearXNG
    /// answering /search?q=rust+tauri+2&format=json.
    ///
    /// Everything the struct does not name is left in on purpose: an
    /// instance adding a field must not stop the tool working.
    const REAL_ANSWER: &str = r#"{
      "query": "rust tauri 2",
      "results": [
        {
          "template": "default.html",
          "title": "Tauri 2.0 | Tauri",
          "content": "Tauri supports any frontend framework so you do not need to change your stack.",
          "img_src": "",
          "thumbnail": "https://encrypted-tbn0.gstatic.com/images?q=tbn",
          "publishedDate": null,
          "pubdate": "",
          "length": null,
          "engines": ["google cse", "duckduckgo", "brave"],
          "positions": [1, 2, 1],
          "score": 6.0,
          "category": "general",
          "url": "https://v2.tauri.app/",
          "parsed_url": ["https", "v2.tauri.app", "/", "", "", ""]
        },
        {
          "title": "Tauri 2.0 Stable Release",
          "content": "Tauri 2.0 is now stable.",
          "publishedDate": "2024-10-02T00:00:00",
          "engines": ["brave"],
          "url": "https://v2.tauri.app/blog/tauri-20/"
        }
      ],
      "answers": [],
      "corrections": [],
      "infoboxes": [],
      "suggestions": [],
      "unresponsive_engines": []
    }"#;

    #[test]
    fn a_real_answer_parses_into_the_fields_this_reads() {
        let body: Answered = serde_json::from_str(REAL_ANSWER).expect("a real answer parses");

        assert_eq!(body.results.len(), 2);
        assert_eq!(body.results[0].title, "Tauri 2.0 | Tauri");
        assert_eq!(body.results[0].url, "https://v2.tauri.app/");
        assert!(body.results[0].content.starts_with("Tauri supports"));

        // publishedDate is null on most results and a date on some. Both
        // arrive, and neither is an error.
        assert_eq!(body.results[0].published, None);
        assert_eq!(body.results[1].published.as_deref(), Some("2024-10-02T00:00:00"));

        assert!(body.answers.is_empty());
        assert!(body.unresponsive_engines.is_empty());
    }

    /// The whole request path, against a real instance.
    ///
    /// Ignored, because the suite has to pass on a machine with no SearXNG
    /// on the network, and a test that needs one cannot be part of it. It is
    /// here rather than nowhere because the fixture above can only prove the
    /// struct agrees with a string committed beside it: whether a live
    /// instance is reachable, answers 200 to `format=json`, and hands back
    /// the fields this reads, is a different question and this is the only
    /// thing that asks it.
    ///
    /// ```text
    /// SILL_SEARXNG=http://searxng.lan:8888 \
    ///   cargo test --lib a_live_instance -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs a SearXNG on the network; set SILL_SEARXNG to its address"]
    async fn a_live_instance_answers_with_results_this_can_read() {
        let address = std::env::var("SILL_SEARXNG")
            .expect("set SILL_SEARXNG to the address of an instance");

        let answer = ask(&address, "tauri rust").await;

        assert!(answer.get("error").is_none(), "{answer}");

        let results = answer["results"].as_array().expect("results is a list");
        assert!(!results.is_empty(), "nothing came back: {answer}");

        for one in results {
            assert!(
                one["url"].as_str().is_some_and(|url| url.starts_with("http")),
                "no usable url: {one}"
            );
            assert!(one["title"].as_str().is_some(), "no title: {one}");
            assert!(
                one["summary"]
                    .as_str()
                    .is_some_and(|summary| summary.chars().count() <= MOST_SUMMARY + 1),
                "summary missing or over the cap: {one}"
            );
        }

        println!("{} results from {address}", results.len());
    }

    #[test]
    fn a_bare_host_is_given_a_scheme() {
        assert_eq!(addressed("searxng.lan:8888").as_deref(), Some("http://searxng.lan:8888"));
        assert_eq!(addressed("192.0.2.10:8888").as_deref(), Some("http://192.0.2.10:8888"));
    }

    #[test]
    fn a_scheme_somebody_typed_is_kept() {
        assert_eq!(addressed("https://searx.example").as_deref(), Some("https://searx.example"));
    }

    #[test]
    fn a_trailing_slash_does_not_become_a_double_one() {
        assert_eq!(addressed("http://searxng.lan:8888/").as_deref(), Some("http://searxng.lan:8888"));
        assert_eq!(addressed("searxng.lan:8888///").as_deref(), Some("http://searxng.lan:8888"));
    }

    #[test]
    fn nothing_useful_is_none_rather_than_a_request_to_nowhere() {
        assert_eq!(addressed(""), None);
        assert_eq!(addressed("   "), None);
        assert_eq!(addressed("http://"), None);
    }

    #[test]
    fn a_short_summary_is_left_alone() {
        assert_eq!(shortened("two words"), "two words");
    }

    /// Short words up to the limit, then one word far too long to fit.
    ///
    /// The input is the whole test. An earlier version repeated one
    /// five-character word, and the limit is a multiple of five, so a cut
    /// that ignored words entirely still landed on a space: the test passed
    /// through the sabotage it exists for. Here the long word cannot fit
    /// whole, so keeping any part of it is proof the cut was blind.
    #[test]
    fn a_long_summary_is_cut_on_a_word_and_marked() {
        let long = format!("{}{}", "x ".repeat(MOST_SUMMARY / 2 - 3), "y".repeat(50));
        let cut = shortened(&long);

        assert!(cut.chars().count() <= MOST_SUMMARY + 1, "{}", cut.chars().count());
        assert!(cut.ends_with(ELLIPSIS), "{cut}");
        assert!(!cut.contains('y'), "kept part of a word that does not fit: {cut}");
        assert!(cut.contains("x x"), "kept nothing at all: {cut}");
    }

    #[test]
    fn a_summary_loses_the_whitespace_a_page_padded_it_with() {
        assert_eq!(shortened("one\n\n  two\tthree "), "one two three");
    }

    #[test]
    fn an_answer_is_read_in_both_shapes_the_instance_has_used() {
        assert_eq!(plainly(&json!("forty two")).as_deref(), Some("forty two"));
        assert_eq!(
            plainly(&json!({ "answer": "forty two", "url": "x" })).as_deref(),
            Some("forty two")
        );
    }

    #[test]
    fn an_empty_answer_is_not_one() {
        assert_eq!(plainly(&json!("   ")), None);
        assert_eq!(plainly(&json!({ "url": "x" })), None);
        assert_eq!(plainly(&json!(7)), None);
    }
}
