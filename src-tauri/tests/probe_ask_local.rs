//! Asking a model that runs on this machine, end to end.
//!
//! The only provider that can be proved without somebody's API key and without
//! anything leaving the machine. It exercises the whole path: the address
//! rule, the request shape, the streaming response and the line splitting that
//! has to survive chunks arriving on no particular boundary.
//!
//! Ignored by default, and skipped rather than failed when nothing is
//! listening: a test that fails because Ollama is not running is a test that
//! gets switched off.
//!
//! ```text
//! ollama serve
//! SILL_OLLAMA_MODEL=qwen3:1.7b cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probe_ask_local -- --ignored --nocapture
//! ```

use sill_lib::ai::openai::{self, Message};
use sill_lib::ai::provider::Provider;

const BASE: &str = "http://localhost:11434/v1";

#[tokio::test]
#[ignore = "needs a model running on this machine"]
async fn a_local_model_answers_and_the_answer_arrives_in_pieces() {
    let model = std::env::var("SILL_OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:1.7b".to_string());

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("a client");

    // Nothing listening is a skip, not a failure.
    if client.get(format!("{BASE}/models")).send().await.is_err() {
        println!("nothing is listening on {BASE}, so there is nothing to ask");
        return;
    }

    let provider = Provider {
        id: "ollama".into(),
        name: "Ollama".into(),
        base_url: BASE.into(),
        api_key: String::new(),
        model: model.clone(),
        ..Provider::default()
    };

    println!("asking {model} on {BASE}");

    let mut pieces: Vec<String> = Vec::new();
    let started = std::time::Instant::now();
    let mut first_piece_after = None;

    openai::ask(
        &client,
        &provider,
        &[
            Message::system("Answer in one short sentence. No preamble."),
            Message::user("What is the capital of France?"),
        ],
        // No tools: this measures the streaming, and a model that decided to
        // look something up would measure something else.
        None,
        // Never stops. What is being measured is a whole answer arriving.
        &|| false,
        |text| {
            if first_piece_after.is_none() {
                first_piece_after = Some(started.elapsed());
            }
            pieces.push(text);
        },
    )
    .await
    .expect("the model answers");

    let answer: String = pieces.concat();

    println!(
        "first piece after {:?}",
        first_piece_after.unwrap_or_default()
    );
    println!("{} pieces in {:?}", pieces.len(), started.elapsed());
    println!("answer: {}", answer.trim());

    assert!(!answer.trim().is_empty(), "the answer was empty");

    // Streamed rather than delivered whole, which is the entire reason for
    // the line splitting above. One piece would mean the stream was buffered
    // somewhere and the launcher would show nothing until it finished.
    assert!(
        pieces.len() > 1,
        "the answer arrived in one piece, so nothing was streaming",
    );

    // Not an assertion about the model's knowledge, which is not what this
    // tests. A small model may reason aloud first, so the whole answer is
    // searched rather than its start.
    assert!(
        answer.to_lowercase().contains("paris"),
        "the answer does not look like an answer: {answer}",
    );
}

/// A model that is not there says so, rather than hanging or answering.
#[tokio::test]
#[ignore = "needs Ollama running on this machine"]
async fn a_model_that_is_not_installed_says_which_one() {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("a client");

    if client.get(format!("{BASE}/models")).send().await.is_err() {
        println!("nothing is listening on {BASE}");
        return;
    }

    let provider = Provider {
        base_url: BASE.into(),
        model: "a-model-nobody-has:0.1b".into(),
        ..Provider::default()
    };

    let refused = openai::ask(
        &client,
        &provider,
        &[Message::user("hello")],
        None,
        &|| false,
        |_| {},
    )
    .await
    .expect_err("a model that is not there cannot answer");

    println!("it said: {refused}");

    // The provider's own words, which is the whole reason the body is read
    // rather than the status alone.
    assert!(
        refused.contains("a-model-nobody-has"),
        "it did not say which model: {refused}",
    );
}
