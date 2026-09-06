//! The shape nearly everything speaks.
//!
//! OpenAI's chat completions, which xAI, Ollama, OpenRouter, LM Studio and
//! Google's compatibility route all accept. One adapter, six services and
//! anything else somebody points it at.
//!
//! The two halves that can go wrong quietly are both pure functions here: what
//! is sent, and how a line of the response is read. The part that cannot be a
//! pure function, the request itself, does as little as possible.

use serde::{Deserialize, Serialize};

use super::provider::Provider;

/// One call the model wants made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// What the result must be labelled with when it is sent back.
    pub id: String,
    /// Always `function`. Sent because the services require the field.
    #[serde(rename = "type")]
    pub kind: String,
    pub function: Called,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Called {
    pub name: String,
    /// JSON, as a string. That is how the services send it, and it arrives a
    /// few characters at a time.
    pub arguments: String,
}

/// Something handed to the model along with a question.
///
/// Two kinds, because the services take exactly two. A picture goes as its own
/// content part and only a model that can see gets anything from it; a text
/// file has no content type of its own anywhere, so it is folded into the
/// words with its name above it, which is what every chat window does and what
/// a model reads correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attached {
    pub name: String,
    /// `image` or `text`.
    pub kind: String,
    /// A data URI for a picture; the text itself for a text file.
    pub body: String,
    /// How big the original was, for the chip that names it.
    pub bytes: usize,
}

impl Attached {
    pub fn is_image(&self) -> bool {
        self.kind == "image"
    }
}

/// Who said what.
///
/// Four roles rather than three now. `assistant` carries the calls it wants
/// made, and `tool` carries one result labelled with the call it answers. Both
/// extra fields are skipped when empty, because a service given
/// `"tool_calls": null` on every ordinary message rejects the request.
///
/// This is the shape Sill stores, not the shape a service receives. The two
/// differ once a picture is involved, and keeping them apart is what lets a
/// saved conversation stay readable while the wire format follows whatever the
/// services decided to require. `wire` below is the one place they meet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// `system`, `user`, `assistant` or `tool`.
    pub role: String,
    pub content: String,
    #[serde(rename = "tool_calls", default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Which call this answers. Only ever set on a `tool` message.
    #[serde(
        rename = "tool_call_id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_call_id: Option<String>,
    /// Anything handed over with the question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attached>,
    /// How an answer came about, in the order it happened.
    ///
    /// Only ever set on an `assistant` message, and never sent anywhere:
    /// `wire` does not read it. It is what the window draws above and between
    /// the words, and a conversation reopened tomorrow shows the working as
    /// well as the answer. A file from before this field reads as a message
    /// with no parts, which draws as it always did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
}

/// One thing that happened on the way to an answer.
///
/// Text, thinking and steps interleave, because that is the order a model
/// produces them: it says what it is about to do, does it, and says what it
/// found. Flattening that into "steps, then words" loses why a step happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Part {
    /// Words of the answer.
    Text { text: String },
    /// What the model thought before it said anything.
    Thinking {
        text: String,
        /// How long it thought for, once something else followed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u64>,
    },
    /// One tool reached for.
    Step {
        /// The call's own id, so its result can find it.
        id: String,
        tool: String,
        /// What it was used on. Empty for tools that take no arguments.
        subject: String,
        /// Whether it worked. `None` while it runs, and after a turn that
        /// stopped before it finished.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<bool>,
    },
}

/// What one request cost, in tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

/// One message, in the shape a service receives.
///
/// Ordinary messages keep a plain string, because that is what every service
/// has always taken and several still insist on. Only a message carrying a
/// picture becomes the array form, and then the text goes in as its own part
/// beside it.
pub fn wire(message: &Message) -> serde_json::Value {
    let mut out = serde_json::json!({ "role": message.role });

    if !message.tool_calls.is_empty() {
        out["tool_calls"] = serde_json::to_value(&message.tool_calls).unwrap_or_default();
    }

    if let Some(id) = &message.tool_call_id {
        out["tool_call_id"] = serde_json::Value::String(id.clone());
    }

    // A text file has no content type anywhere, so it becomes part of what was
    // said, named so the model knows where it came from.
    let mut said = message.content.clone();

    for file in message.attachments.iter().filter(|one| !one.is_image()) {
        said.push_str(&format!(
            "\n\n--- {} ---\n{}\n--- end of {} ---",
            file.name, file.body, file.name
        ));
    }

    let pictures: Vec<&Attached> = message
        .attachments
        .iter()
        .filter(|one| one.is_image())
        .collect();

    if pictures.is_empty() {
        out["content"] = serde_json::Value::String(said);
        return out;
    }

    let mut parts = vec![serde_json::json!({ "type": "text", "text": said })];

    for picture in pictures {
        parts.push(serde_json::json!({
            "type": "image_url",
            "image_url": { "url": picture.body },
        }));
    }

    out["content"] = serde_json::Value::Array(parts);
    out
}

impl Message {
    fn plain(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            attachments: Vec::new(),
            parts: Vec::new(),
        }
    }

    /// The same message, with how it came about.
    pub fn with_parts(mut self, parts: Vec<Part>) -> Self {
        self.parts = parts;
        self
    }

    /// A question with something handed over alongside it.
    pub fn with(content: impl Into<String>, attachments: Vec<Attached>) -> Self {
        Self {
            attachments,
            ..Self::plain("user", content)
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::plain("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::plain("assistant", content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::plain("system", content)
    }

    /// What the model said it wants to do, kept so the next request has it.
    ///
    /// The services require the whole turn back: the calls it asked for and
    /// then the results, in that order. Sending only the results earns a
    /// complaint about a tool message with no call before it.
    pub fn calling(content: impl Into<String>, calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: calls,
            tool_call_id: None,
            attachments: Vec::new(),
            parts: Vec::new(),
        }
    }

    /// One result, labelled with the call it answers.
    pub fn answered(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(id.into()),
            attachments: Vec::new(),
            parts: Vec::new(),
        }
    }
}

/// A piece of one call, as it arrives.
///
/// Fragmented on purpose by the services: the name comes whole in the first
/// piece and the arguments arrive a few characters at a time after it, all
/// keyed by a position in the list rather than by the id, because the id is in
/// the first piece too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPiece {
    pub at: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: String,
}

/// What one line of the response said.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// More of the answer.
    Text(String),
    /// More of a call the model wants made.
    Calling(CallPiece),
    /// More of what the model is thinking before it answers.
    Thinking(String),
    /// What the request cost, which arrives once at the very end.
    ///
    /// In tokens always, and in dollars when the service does the sum
    /// itself, which today is OpenRouter once it has been asked to.
    Usage { usage: Usage, cost: Option<f64> },
    /// Which model actually answered, which can differ from the one asked
    /// for when the name was an alias or a gateway chose.
    Model(String),
    /// The answer is finished.
    Done,
    /// Nothing this needs to act on.
    Ignored,
}

/// Where to send it.
///
/// A base URL is given as the service's own documentation gives it, which for
/// some ends in `/v1` and for others does not. Joining without checking
/// produces `/v1/v1/chat/completions`, which answers 404 with nothing in the
/// message explaining why. The dictation side learned this already; this is
/// the same trap in the same shape.
pub fn endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    format!("{base}/chat/completions")
}

/// What gets posted.
///
/// The tool list rides along when there is one. Sending an empty array is not
/// the same as sending none: several services take it as "tools are in play,
/// here are zero" and answer differently, so the field is left out entirely.
pub fn body(
    provider: &Provider,
    messages: &[Message],
    tools: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": provider.model,
        "messages": messages.iter().map(wire).collect::<Vec<_>>(),
        // Tokens as they are produced. A launcher that shows nothing for four
        // seconds and then a paragraph feels broken even when it is not.
        "stream": true,
        // What it cost, on one more line at the end. Services that do not
        // know the option ignore it; none has been seen to refuse it.
        "stream_options": { "include_usage": true },
    });

    if let Some(tools) = tools.filter(|list| !list.as_array().is_some_and(Vec::is_empty)) {
        body["tools"] = tools.clone();
    }

    // OpenRouter does the sum itself, in dollars, when asked with a field of
    // its own. Only sent to it: OpenAI refuses a request carrying a field it
    // does not know, and nothing else has a cost to report.
    if sums_the_cost(&provider.base_url) {
        body["usage"] = serde_json::json!({ "include": true });
    }

    body
}

/// Whether this address names the dollars on its usage chunk when asked.
///
/// A gateway that bills for many models knows the rate for each, and the
/// number it names is the one that appears on the bill, which beats any table
/// Sill could keep. The host is matched rather than the provider id because
/// somebody can add OpenRouter as a custom entry under any name.
fn sums_the_cost(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("openrouter.ai")
}

/// Reads one line of the event stream.
///
/// Server-sent events: lines beginning `data: `, one JSON object each, and a
/// literal `[DONE]` at the end. Comments, blank lines and any field other than
/// `data` are ignored, which is what the format says to do and also what keeps
/// a future field from breaking the answer arriving.
pub fn parse_line(line: &str) -> Event {
    let line = line.trim();

    let Some(payload) = line.strip_prefix("data:") else {
        return Event::Ignored;
    };

    let payload = payload.trim();

    if payload == "[DONE]" {
        return Event::Done;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Event::Ignored;
    };

    // A call being asked for, which is read before the text: the same delta
    // can carry an empty content alongside a piece of a call, and reading
    // content first would drop it.
    if let Some(piece) = value
        .pointer("/choices/0/delta/tool_calls/0")
        .and_then(read_piece)
    {
        return Event::Calling(piece);
    }

    // The delta of the first choice, which is the only one asked for.
    if let Some(text) = value
        .pointer("/choices/0/delta/content")
        .and_then(|c| c.as_str())
        .filter(|text| !text.is_empty())
    {
        return Event::Text(text.to_string());
    }

    // Thinking, under either of the two names it goes by. `reasoning_content`
    // is the older spelling most gateways copied; `reasoning` is what the
    // newer services send. A service that streams neither is simply a service
    // that does not think out loud.
    for key in ["/choices/0/delta/reasoning_content", "/choices/0/delta/reasoning"] {
        if let Some(thought) = value
            .pointer(key)
            .and_then(|t| t.as_str())
            .filter(|thought| !thought.is_empty())
        {
            return Event::Thinking(thought.to_string());
        }
    }

    // The cost, which comes on its own line at the end once it was asked for.
    if let Some(usage) = value.get("usage").and_then(read_usage) {
        let cost = value
            .pointer("/usage/cost")
            .and_then(serde_json::Value::as_f64)
            .filter(|cost| cost.is_finite() && *cost >= 0.0);
        return Event::Usage { usage, cost };
    }

    // A chunk with nothing else to say still names the model. Read last, so a
    // chunk carrying words is words rather than a name repeated every line.
    if let Some(model) = value
        .get("model")
        .and_then(|m| m.as_str())
        .filter(|model| !model.is_empty())
    {
        return Event::Model(model.to_string());
    }

    Event::Ignored
}

/// The token counts out of a `usage` object.
///
/// Both names, because the field is `prompt_tokens` in the shape everybody
/// copied and `input_tokens` in the shape Anthropic's gateway route sends.
fn read_usage(value: &serde_json::Value) -> Option<Usage> {
    let count = |keys: [&str; 2]| {
        keys.iter()
            .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
    };

    let input = count(["prompt_tokens", "input_tokens"]);
    let output = count(["completion_tokens", "output_tokens"]);

    match (input, output) {
        (None, None) => None,
        (input, output) => Some(Usage {
            input: input.unwrap_or(0),
            output: output.unwrap_or(0),
        }),
    }
}

/// One fragment of a call, out of the delta that carried it.
fn read_piece(value: &serde_json::Value) -> Option<CallPiece> {
    Some(CallPiece {
        // Missing means the first and only one. Several services leave the
        // index off when a turn asks for exactly one call.
        at: value
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize,
        id: value
            .get("id")
            .and_then(|id| id.as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        name: value
            .pointer("/function/name")
            .and_then(|name| name.as_str())
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        arguments: value
            .pointer("/function/arguments")
            .and_then(|args| args.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// Every call a turn asked for, assembled from the pieces it arrived in.
///
/// Its own type because the assembling is the part that goes wrong: pieces
/// are keyed by position, a name arrives once and arguments arrive many
/// times, and appending to the wrong slot produces a call whose arguments are
/// two calls spliced together.
#[derive(Debug, Default)]
pub struct Calls {
    building: Vec<ToolCall>,
}

impl Calls {
    pub fn take(&mut self, piece: CallPiece) {
        while self.building.len() <= piece.at {
            self.building.push(ToolCall {
                id: String::new(),
                kind: "function".to_string(),
                function: Called {
                    name: String::new(),
                    arguments: String::new(),
                },
            });
        }

        let call = &mut self.building[piece.at];

        if let Some(id) = piece.id {
            call.id = id;
        }
        if let Some(name) = piece.name {
            call.function.name = name;
        }
        call.function.arguments.push_str(&piece.arguments);
    }

    /// What was asked for, dropping anything that never got a name.
    ///
    /// A slot with no name is a gap left by a service numbering its calls from
    /// one, or a stream that stopped part way. Either way there is nothing to
    /// run and nothing to answer with.
    pub fn finish(self) -> Vec<ToolCall> {
        self.building
            .into_iter()
            .filter(|call| !call.function.name.is_empty())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.building
            .iter()
            .all(|call| call.function.name.is_empty())
    }
}

/// The headers a request needs, beyond the content type.
///
/// A service that needs no key is sent no header rather than an empty one: a
/// local model does not want an `Authorization: Bearer ` with nothing after
/// it, and some gateways reject that rather than ignoring it.
pub fn headers(provider: &Provider) -> Vec<(String, String)> {
    let key = provider.api_key.trim();

    if key.is_empty() {
        return Vec::new();
    }

    vec![("Authorization".to_string(), format!("Bearer {key}"))]
}

/// One piece of an answer as it arrives, handed to whoever asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Words of the answer.
    Text(String),
    /// Thinking, before the words.
    Thinking(String),
}

/// Asks, and hands each piece of the answer to `on_piece` as it arrives.
///
/// Streaming rather than collecting, because the point of a launcher is that
/// it answers while you are still reading the first line. The caller decides
/// what to do with each piece; this knows nothing about windows or events.
///
/// The body is read as bytes and split on newlines here rather than with a
/// line-oriented reader, because a chunk from the network does not arrive on
/// a line boundary and the tail of one chunk is the head of the next.
pub async fn ask(
    client: &reqwest::Client,
    provider: &Provider,
    messages: &[Message],
    tools: Option<&serde_json::Value>,
    /*
     * Asked between chunks rather than awaited on, because the answer is
     * already arriving and what stopping means is stopping reading it.
     *
     * `Sync` on purpose. Tauri needs a command's future to be `Send`, and a
     * plain `&dyn Fn` is neither, so leaving it off makes the whole command
     * fail to compile with an error that names the await rather than this.
     */
    give_up: &(dyn Fn() -> bool + Sync),
    mut on_piece: impl FnMut(Piece),
) -> Result<Said, String> {
    use futures_util::StreamExt;

    super::provider::check(&provider.base_url).map_err(|why| why.message().to_string())?;

    let mut request = client
        .post(endpoint(&provider.base_url))
        .json(&body(provider, messages, tools));

    for (name, value) in headers(provider) {
        request = request.header(name, value);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach {}: {err}", provider.name))?;

    if !response.status().is_success() {
        let status = response.status();
        // The body, because a provider's own message says far more than the
        // number does: a wrong model name and an unpaid account are both 400.
        let said = response.text().await.unwrap_or_default();
        return Err(complaint(status.as_u16(), &said));
    }

    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    // Kept as well as handed out, because a turn that asks for a tool has to
    // send its own words back with the calls, and the caller only sees the
    // pieces as they go past.
    let mut said = Said::default();
    let mut calls = Calls::default();
    // When the first piece arrived, so the answer can be timed from there.
    // Measured from the first piece rather than from the request, because
    // what comes before it is the service reading the prompt, and a rate that
    // included that would fall as the conversation grew.
    let mut first: Option<std::time::Instant> = None;

    while let Some(chunk) = stream.next().await {
        // Checked before the chunk is read rather than after, so a stop takes
        // effect on the next thing to arrive rather than one thing later.
        if give_up() {
            said.stopped = true;
            said.generating_ms = since(first);
            return Ok(said);
        }

        let chunk = chunk.map_err(|err| format!("the answer stopped part way: {err}"))?;
        pending.push_str(&String::from_utf8_lossy(&chunk));

        // Everything up to the last newline is whole lines; what follows it is
        // the start of the next one.
        while let Some(at) = pending.find('\n') {
            let line: String = pending.drain(..=at).collect();

            match parse_line(&line) {
                Event::Text(text) => {
                    first.get_or_insert_with(std::time::Instant::now);
                    said.text.push_str(&text);
                    on_piece(Piece::Text(text));
                }
                Event::Thinking(thought) => {
                    first.get_or_insert_with(std::time::Instant::now);
                    said.thinking.push_str(&thought);
                    on_piece(Piece::Thinking(thought));
                }
                Event::Calling(piece) => {
                    first.get_or_insert_with(std::time::Instant::now);
                    calls.take(piece);
                }
                Event::Usage { usage, cost } => {
                    said.usage = Some(usage);
                    said.cost = cost;
                }
                Event::Model(model) => said.model = model,
                Event::Done => {
                    said.calls = calls.finish();
                    said.generating_ms = since(first);
                    return Ok(said);
                }
                Event::Ignored => {}
            }
        }
    }

    // The stream ended without a `[DONE]`, which several services do. The
    // answer still arrived.
    said.calls = calls.finish();
    said.generating_ms = since(first);
    Ok(said)
}

/// Milliseconds since the first piece, or nothing if none came.
fn since(first: Option<std::time::Instant>) -> u64 {
    first.map_or(0, |at| at.elapsed().as_millis() as u64)
}

/// What one exchange produced.
///
/// Either words, or calls to make and then ask again, or both: a model
/// explaining what it is about to do and then doing it is one turn, not two.
#[derive(Debug, Default)]
pub struct Said {
    pub text: String,
    /// What it thought before answering, for services that say.
    pub thinking: String,
    pub calls: Vec<ToolCall>,
    /// Which model answered, when the service said. Empty otherwise.
    pub model: String,
    /// What the request cost, when the service said.
    pub usage: Option<Usage>,
    /// In dollars, for the one service that says. See `Event::Usage`.
    pub cost: Option<f64>,
    /// From the first piece to the last, in milliseconds. Zero when nothing
    /// arrived, and what a local model's speed is read from.
    pub generating_ms: u64,
    /// Whether it was told to stop rather than finishing.
    ///
    /// Not an error. What arrived is still an answer and is still worth
    /// keeping: somebody who stops a reply has usually read enough of it.
    pub stopped: bool,
}

/// Which models this service has.
///
/// Asked rather than typed. A model id is a string like `gemini-3-flash` or
/// `anthropic/claude-sonnet-5`, and one character wrong is a request that
/// fails with a message about a model nobody meant to ask for. Every service
/// that speaks this shape publishes the list, so the choice can be a list.
///
/// Sorted, because a service returns them in whatever order its database felt
/// like and a picker that reorders itself between openings is one nobody can
/// learn.
pub async fn models(client: &reqwest::Client, provider: &Provider) -> Result<Vec<String>, String> {
    super::provider::check(&provider.base_url).map_err(|why| why.message().to_string())?;

    let base = provider.base_url.trim().trim_end_matches('/');
    let mut request = client.get(format!("{base}/models"));

    for (name, value) in headers(provider) {
        request = request.header(name, value);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach {}: {err}", provider.name))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let said = response.text().await.unwrap_or_default();
        return Err(complaint(status, &said));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("that list could not be read: {err}"))?;

    Ok(model_ids(&body))
}

/// The ids out of a models response.
///
/// Its own function so the shape can be tested without a service. Both the
/// documented `{"data": [...]}` and the bare array some gateways return, in
/// case the second is what arrives.
pub fn model_ids(body: &serde_json::Value) -> Vec<String> {
    let rows = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.as_array());

    let Some(rows) = rows else {
        return Vec::new();
    };

    let mut ids: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            row.get("id")
                .and_then(|id| id.as_str())
                // A bare list of names, which is what a couple of gateways send.
                .or_else(|| row.as_str())
        })
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();

    ids.sort();
    ids.dedup();
    ids
}

/// What to say when a provider refuses.
///
/// Its own words where there are any, because "400" tells somebody nothing
/// and "model not found" tells them exactly what to fix. Trimmed, because a
/// gateway will return a page of HTML and a status line is not the place.
fn complaint(status: u16, body: &str) -> String {
    /*
     * The three refusals that mean something specific, said in Sill's words.
     *
     * A provider's own JSON is written for whoever is calling the API, and it
     * is the wrong register for a settings window: somebody who pasted the
     * wrong key needs to be told that, not handed a nested error object to
     * read. Everything else falls through to the body, because a message
     * nobody anticipated is more use verbatim than summarised wrongly.
     */
    /*
     * The two refusals worth saying in Sill's words.
     *
     * Rejected credentials are said here rather than passed through, because
     * some services quote the key back in the message and a settings window is
     * not the place for that. Everything else keeps the provider's own words,
     * including a 404: it looks like a wrong address and often is not, since a
     * model that does not exist is a 404 on several services, and telling
     * somebody to check an address that is correct sends them the wrong way.
     */
    match status {
        401 | 403 => return "That key was not accepted.".to_string(),
        429 => return "That provider is rate limiting the request. Try again shortly.".to_string(),
        _ => {}
    }

    // The provider's own sentence, dug out of the object it arrived in.
    //
    // Every service speaking this shape answers a refusal with an error field,
    // and what is in it is usually the most useful thing available: xAI's says
    // where to get a key, and a wrong model name is quoted back exactly as it
    // was sent. What is not useful is the JSON around it.
    if let Some(said) = said_by(body) {
        // The number as an aside rather than the subject. It says nothing on
        // its own, but it is the searchable half when somebody reports one.
        return format!("{said} ({status})");
    }

    let said = body.trim();

    if said.is_empty() {
        return format!("that provider refused the request ({status})");
    }

    let short: String = said.chars().take(300).collect();
    format!("that provider refused the request ({status}): {short}")
}

/// The sentence a provider put in its error body, if it put one there.
///
/// Two shapes, because the services disagree: `{"error": "..."}` and
/// `{"error": {"message": "..."}}`. Anything else answers nothing and the
/// caller falls back to the body as it arrived, which is the right default for
/// a message nobody anticipated.
fn said_by(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body.trim()).ok()?;

    let said = value
        .pointer("/error/message")
        .or_else(|| value.pointer("/error"))
        .or_else(|| value.pointer("/message"))
        .and_then(|found| found.as_str())?
        .trim();

    if said.is_empty() {
        return None;
    }

    Some(said.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base: &str, key: &str, model: &str) -> Provider {
        Provider {
            base_url: base.into(),
            api_key: key.into(),
            model: model.into(),
            ..Provider::default()
        }
    }

    mod what_a_refusal_says {
        use super::*;

        /// The one that happens most, and the one whose real body is least
        /// use: a nested JSON error object about invalid authentication is
        /// not what somebody who pasted the wrong key needs to read.
        #[test]
        fn a_rejected_key_says_that_and_nothing_else() {
            let said = complaint(
                401,
                r#"{"error":{"message":"Incorrect API key provided: sk-abc..."}}"#,
            );
            assert_eq!(said, "That key was not accepted.");
            assert!(!said.contains("sk-abc"), "it repeated the key back");
        }

        #[test]
        fn a_forbidden_request_reads_the_same_way() {
            assert_eq!(complaint(403, "{}"), "That key was not accepted.");
        }

        /// A 404 is not proof the address is wrong. Several services answer
        /// one for a model that does not exist, and sending somebody to check
        /// an address that is correct sends them the wrong way.
        #[test]
        fn a_404_is_not_read_as_a_wrong_address() {
            let said = complaint(404, r#"{"error":{"message":"model \"qwen9\" not found"}}"#);
            assert!(said.contains("qwen9"), "it said {said:?}");
            assert!(
                !said.to_lowercase().contains("address"),
                "it guessed: {said:?}"
            );
        }

        #[test]
        fn being_rate_limited_says_to_come_back() {
            assert!(complaint(429, "slow down").contains("rate limiting"));
        }

        /// What xAI actually answers a bad key with, taken from a real
        /// request. The status is 400, not 401, which is why mapping by status
        /// alone was not enough: the services disagree about which number a
        /// rejected key is, and only the body says what happened.
        #[test]
        fn a_provider_that_calls_a_bad_key_a_bad_request_still_reads_plainly() {
            let said = complaint(
                400,
                r#"{"code":"invalid-argument","error":"Incorrect API key provided. You can obtain an API key from https://console.x.ai."}"#,
            );

            assert_eq!(
                said,
                "Incorrect API key provided. You can obtain an API key from \
                 https://console.x.ai. (400)",
            );
        }

        /// The other shape, which is what OpenAI and most of the rest send.
        #[test]
        fn a_nested_message_is_dug_out_of_its_object() {
            let said = complaint(
                400,
                r#"{"error":{"message":"model `grok-9` does not exist","type":"invalid_request_error"}}"#,
            );
            assert_eq!(said, "model `grok-9` does not exist (400)");
            assert!(
                !said.contains("invalid_request_error"),
                "the object came too"
            );
        }

        /// A body that is not JSON at all is still the most useful thing there
        /// is, so it survives.
        #[test]
        fn something_that_is_not_json_is_passed_through() {
            let said = complaint(502, "upstream connect error");
            assert!(said.contains("upstream connect error"));
        }

        /// An error field that is an empty string is not a message.
        #[test]
        fn an_empty_message_falls_back_rather_than_saying_nothing() {
            let said = complaint(400, r#"{"error":{"message":"  "}}"#);
            assert!(said.contains("400"), "it said {said:?}");
        }

        /// Anything unanticipated is passed through rather than summarised
        /// wrongly. A wrong model name and an unpaid account are both 400,
        /// and only the provider knows which.
        #[test]
        fn anything_else_keeps_the_providers_own_words() {
            let said = complaint(400, r#"{"error":"model `grok-9` does not exist"}"#);
            assert!(said.contains("grok-9"), "the useful half was thrown away");
        }

        #[test]
        fn an_empty_body_still_names_the_status() {
            assert!(complaint(500, "   ").contains("500"));
        }

        /// A provider that answers an error with a page of HTML must not put
        /// a page of HTML in a settings window.
        #[test]
        fn an_enormous_body_is_cut_down() {
            let said = complaint(400, &"x".repeat(5000));
            assert!(
                said.chars().count() < 400,
                "it was {} long",
                said.chars().count()
            );
        }
    }

    mod where_it_goes {
        use super::*;

        #[test]
        fn a_base_url_gets_the_path_added() {
            assert_eq!(
                endpoint("https://api.openai.com/v1"),
                "https://api.openai.com/v1/chat/completions",
            );
        }

        /// People paste the URL as their provider's documentation gives it,
        /// and some of them end in a slash.
        #[test]
        fn a_trailing_slash_does_not_double_up() {
            assert_eq!(
                endpoint("http://localhost:11434/v1/"),
                "http://localhost:11434/v1/chat/completions",
            );
            assert_eq!(
                endpoint("  https://api.x.ai/v1  "),
                "https://api.x.ai/v1/chat/completions",
            );
        }
    }

    mod what_is_sent {
        use super::*;

        #[test]
        fn the_model_and_the_conversation_go_in_it() {
            let sent = body(
                &provider("http://x/v1", "", "qwen3:1.7b"),
                &[Message::system("be brief"), Message::user("hello")],
                None,
            );

            assert_eq!(sent["model"], "qwen3:1.7b");
            assert_eq!(sent["messages"][0]["role"], "system");
            assert_eq!(sent["messages"][1]["content"], "hello");
        }

        /// A launcher that shows nothing for four seconds and then a paragraph
        /// feels broken even when it is not.
        #[test]
        fn it_always_asks_for_a_stream() {
            let sent = body(&provider("http://x/v1", "", "m"), &[], None);
            assert_eq!(sent["stream"], true);
        }

        /// The cost arrives on one more line at the end, and only when it
        /// was asked for.
        #[test]
        fn usage_is_asked_for_alongside_the_stream() {
            let sent = body(&provider("http://x/v1", "", "m"), &[], None);
            assert_eq!(sent["stream_options"]["include_usage"], true);
        }

        /// The working stays with the answer and goes nowhere else. A service
        /// sent a field it does not know rejects the whole request.
        #[test]
        fn parts_are_kept_but_never_sent() {
            let message = Message::assistant("eleven").with_parts(vec![Part::Step {
                id: "call_1".into(),
                tool: "list_windows".into(),
                subject: String::new(),
                ok: Some(true),
            }]);

            let sent = wire(&message);
            assert!(sent.get("parts").is_none(), "{sent}");
            assert_eq!(sent["content"], "eleven");

            let kept: Message =
                serde_json::from_str(&serde_json::to_string(&message).expect("written"))
                    .expect("read back");
            assert_eq!(kept.parts, message.parts);
        }

        /// A message written before there were parts.
        #[test]
        fn a_message_without_parts_still_reads() {
            let read: Message =
                serde_json::from_str(r#"{"role":"assistant","content":"eleven"}"#)
                    .expect("read");
            assert!(read.parts.is_empty());
            assert_eq!(read, Message::assistant("eleven"));
        }

        /// A local model does not want an empty bearer token, and some
        /// gateways reject one rather than ignoring it.
        #[test]
        fn nothing_that_needs_no_key_is_sent_an_empty_one() {
            assert!(headers(&provider("http://localhost:11434/v1", "", "m")).is_empty());
            assert!(headers(&provider("http://x/v1", "   ", "m")).is_empty());
        }

        #[test]
        fn a_key_is_sent_as_a_bearer_token() {
            let sent = headers(&provider("https://x/v1", "sk-abc", "m"));
            assert_eq!(sent, vec![("Authorization".into(), "Bearer sk-abc".into())]);
        }
    }

    mod listing_the_models {
        use super::*;

        #[test]
        fn the_documented_shape_reads() {
            let body = serde_json::json!({
                "object": "list",
                "data": [
                    {"id": "gpt-5.2", "object": "model"},
                    {"id": "gpt-5.2-mini", "object": "model"},
                ],
            });

            assert_eq!(model_ids(&body), vec!["gpt-5.2", "gpt-5.2-mini"]);
        }

        /// A picker that reorders itself between openings is one nobody can
        /// learn, and services return these in whatever order they like.
        #[test]
        fn they_come_back_sorted() {
            let body = serde_json::json!({"data": [{"id": "zeta"}, {"id": "alpha"}]});
            assert_eq!(model_ids(&body), vec!["alpha", "zeta"]);
        }

        #[test]
        fn a_bare_array_reads_too() {
            let body = serde_json::json!([{"id": "a"}, "b"]);
            assert_eq!(model_ids(&body), vec!["a", "b"]);
        }

        /// Nothing usable is an empty list, not a failure: the panel then
        /// offers a text field instead of a picker, which still works.
        #[test]
        fn something_unrecognisable_is_no_models_rather_than_an_error() {
            for body in [
                serde_json::json!({}),
                serde_json::json!({"data": "not a list"}),
                serde_json::json!({"data": [{"name": "no id here"}]}),
                serde_json::json!(null),
            ] {
                assert!(model_ids(&body).is_empty(), "{body}");
            }
        }

        #[test]
        fn the_same_model_listed_twice_appears_once() {
            let body = serde_json::json!({"data": [{"id": "a"}, {"id": "a"}]});
            assert_eq!(model_ids(&body), vec!["a"]);
        }
    }

    mod when_it_refuses {
        use super::*;

        /// "400" tells somebody nothing. "model not found" tells them what to
        /// fix, and a provider's own words are the only place that comes from.
        #[test]
        fn the_providers_own_words_are_kept() {
            let said = complaint(404, r#"{"error":{"message":"model \"qwen9\" not found"}}"#);
            assert!(said.contains("qwen9"), "{said}");
            assert!(said.contains("404"), "{said}");
        }

        #[test]
        fn a_silent_refusal_still_says_something() {
            assert_eq!(
                complaint(500, "   "),
                "that provider refused the request (500)",
            );
        }

        /// A gateway will answer with a page of HTML, and a status line is not
        /// the place for it.
        #[test]
        fn a_wall_of_text_is_cut_rather_than_shown_whole() {
            let said = complaint(502, &"x".repeat(5000));
            assert!(said.len() < 400, "{} characters", said.len());
        }
    }

    mod handing_it_something {
        use super::*;

        fn a_picture(name: &str) -> Attached {
            Attached {
                name: name.into(),
                kind: "image".into(),
                body: "data:image/png;base64,AAAA".into(),
                bytes: 3,
            }
        }

        fn a_file(name: &str, body: &str) -> Attached {
            Attached {
                name: name.into(),
                kind: "text".into(),
                body: body.into(),
                bytes: body.len(),
            }
        }

        /// The shape every service has always taken, and several still insist
        /// on. A plain question must not become an array just because the
        /// field could hold one.
        #[test]
        fn an_ordinary_message_keeps_a_plain_string() {
            let sent = wire(&Message::user("what is this"));
            assert_eq!(sent["content"], "what is this");
            assert!(sent["content"].is_string());
        }

        #[test]
        fn a_picture_becomes_its_own_content_part() {
            let sent = wire(&Message::with("what is this", vec![a_picture("shot.png")]));

            let parts = sent["content"].as_array().expect("an array");
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0]["type"], "text");
            assert_eq!(parts[0]["text"], "what is this");
            assert_eq!(parts[1]["type"], "image_url");
            assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,AAAA");
        }

        #[test]
        fn several_pictures_all_go() {
            let sent = wire(&Message::with(
                "compare these",
                vec![a_picture("one.png"), a_picture("two.png")],
            ));

            assert_eq!(sent["content"].as_array().expect("an array").len(), 3);
        }

        /*
         * A text file has no content type anywhere, so it is folded into the
         * words with its name above it. That is what every chat window does
         * and what a model reads correctly; sending it as an unknown part type
         * earns a complaint about the request.
         */
        #[test]
        fn a_text_file_is_folded_into_what_was_said() {
            let sent = wire(&Message::with(
                "summarise",
                vec![a_file("notes.md", "the body")],
            ));

            let said = sent["content"].as_str().expect("a string");
            assert!(said.starts_with("summarise"), "{said}");
            assert!(said.contains("notes.md"), "the name is missing: {said}");
            assert!(said.contains("the body"), "the text is missing: {said}");
        }

        /// One of each. The file goes into the text part, not beside it.
        #[test]
        fn a_file_and_a_picture_together() {
            let sent = wire(&Message::with(
                "look",
                vec![a_file("notes.md", "the body"), a_picture("shot.png")],
            ));

            let parts = sent["content"].as_array().expect("an array");
            assert_eq!(parts.len(), 2, "the file became its own part");
            assert!(parts[0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("the body"));
            assert_eq!(parts[1]["type"], "image_url");
        }

        /// The whole turn still has to go back when tools are in play, and
        /// attaching something must not disturb that.
        #[test]
        fn a_turn_asking_for_a_tool_is_unchanged() {
            let call = ToolCall {
                id: "call_a".into(),
                kind: "function".into(),
                function: Called {
                    name: "system_state".into(),
                    arguments: "{}".into(),
                },
            };

            let sent = wire(&Message::calling("Let me look.", vec![call]));
            assert_eq!(sent["role"], "assistant");
            assert_eq!(sent["content"], "Let me look.");
            assert_eq!(sent["tool_calls"][0]["id"], "call_a");
        }

        #[test]
        fn a_result_still_names_the_call_it_answers() {
            let sent = wire(&Message::answered("call_a", "{}"));
            assert_eq!(sent["role"], "tool");
            assert_eq!(sent["tool_call_id"], "call_a");
            assert_eq!(sent["content"], "{}");
        }

        /// Neither extra field may appear on an ordinary message: a service
        /// given `"tool_calls": null` on every one rejects the request.
        #[test]
        fn nothing_extra_rides_along_on_a_plain_message() {
            let sent = wire(&Message::user("hello"));
            assert!(sent.get("tool_calls").is_none(), "{sent}");
            assert!(sent.get("tool_call_id").is_none(), "{sent}");
            assert!(sent.get("attachments").is_none(), "{sent}");
        }

        /// What is stored and what is sent are different shapes on purpose.
        /// A saved conversation keeps the attachment as an attachment so it
        /// can be drawn again; only the wire folds it into the content.
        #[test]
        fn what_is_stored_keeps_the_attachment_whole() {
            let held = Message::with("look", vec![a_picture("shot.png")]);
            let saved = serde_json::to_value(&held).expect("stored");

            assert_eq!(saved["content"], "look");
            assert_eq!(saved["attachments"][0]["name"], "shot.png");

            let read: Message = serde_json::from_value(saved).expect("read back");
            assert_eq!(read.attachments.len(), 1);
            assert_eq!(read.content, "look");
        }
    }

    mod asking_for_a_tool {
        use super::*;

        /// Real frames, in the order and the pieces a service sends them.
        /// The name arrives once with the id, and the arguments arrive a few
        /// characters at a time after it.
        fn pieces(lines: &[&str]) -> Vec<ToolCall> {
            let mut calls = Calls::default();

            for line in lines {
                match parse_line(line) {
                    Event::Calling(piece) => calls.take(piece),
                    _ => {}
                }
            }

            calls.finish()
        }

        #[test]
        fn one_call_is_assembled_from_its_fragments() {
            let calls = pieces(&[
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"find_files","arguments":""}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"qu"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ery\":\"inv"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"oice\"}"}}]}}]}"#,
            ]);

            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].id, "call_a");
            assert_eq!(calls[0].function.name, "find_files");
            assert_eq!(calls[0].function.arguments, r#"{"query":"invoice"}"#);
        }

        /// Two calls in one turn, interleaved. Appending to the wrong slot
        /// produces one call whose arguments are two calls spliced together,
        /// which parses as nothing and fails with a message about JSON.
        #[test]
        fn two_calls_do_not_run_into_each_other() {
            let calls = pieces(&[
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"find_files","arguments":"{\"query\":"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"list_windows","arguments":"{"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a\"}"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"}"}}]}}]}"#,
            ]);

            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].function.name, "find_files");
            assert_eq!(calls[0].function.arguments, r#"{"query":"a"}"#);
            assert_eq!(calls[1].function.name, "list_windows");
            assert_eq!(calls[1].function.arguments, "{}");
        }

        /// Several services leave the index off when a turn asks for exactly
        /// one call. Treating a missing index as anything but the first slot
        /// loses the call entirely.
        #[test]
        fn a_missing_index_is_the_first_one() {
            let calls = pieces(&[
                r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"a","function":{"name":"system_state","arguments":"{}"}}]}}]}"#,
            ]);

            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].function.name, "system_state");
        }

        /// A stream that stopped part way leaves a slot with no name in it.
        /// There is nothing to run and nothing to answer with, so it goes.
        #[test]
        fn a_slot_that_never_got_a_name_is_dropped() {
            let mut calls = Calls::default();
            calls.take(CallPiece {
                at: 1,
                id: None,
                name: None,
                arguments: "{}".into(),
            });
            assert!(calls.is_empty());
            assert!(calls.finish().is_empty());
        }

        /// The same delta can carry an empty content beside a piece of a call.
        /// Reading content first drops the call.
        #[test]
        fn a_call_beside_an_empty_content_is_still_read() {
            let line = r#"data: {"choices":[{"delta":{"content":"","tool_calls":[{"index":0,"id":"a","function":{"name":"system_state","arguments":"{}"}}]}}]}"#;
            assert!(matches!(parse_line(line), Event::Calling(_)));
        }

        #[test]
        fn ordinary_text_is_still_text() {
            let line = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
            assert_eq!(parse_line(line), Event::Text("Hello".into()));
        }
    }

    mod what_a_turn_sends_back {
        use super::*;

        /// The services want the whole turn: what the model said, then the
        /// calls it asked for, then one result per call. Sending only the
        /// results earns a complaint about a tool message with no call before
        /// it.
        #[test]
        fn a_turn_that_asked_for_something_carries_the_calls() {
            let call = ToolCall {
                id: "call_a".into(),
                kind: "function".into(),
                function: Called {
                    name: "system_state".into(),
                    arguments: "{}".into(),
                },
            };

            let sent = serde_json::to_value(Message::calling("Let me look.", vec![call])).unwrap();

            assert_eq!(sent["role"], "assistant");
            assert_eq!(sent["tool_calls"][0]["id"], "call_a");
            assert_eq!(sent["tool_calls"][0]["type"], "function");
            assert_eq!(sent["tool_calls"][0]["function"]["name"], "system_state");
        }

        #[test]
        fn a_result_names_the_call_it_answers() {
            let sent = serde_json::to_value(Message::answered("call_a", "{}")).unwrap();
            assert_eq!(sent["role"], "tool");
            assert_eq!(sent["tool_call_id"], "call_a");
        }

        /// A service given `"tool_calls": null` on every ordinary message
        /// rejects the request, so the field is absent rather than empty.
        #[test]
        fn an_ordinary_message_carries_neither_field() {
            let sent = serde_json::to_value(Message::user("hello")).unwrap();
            assert!(sent.get("tool_calls").is_none(), "{sent}");
            assert!(sent.get("tool_call_id").is_none(), "{sent}");
        }

        /// "Tools are in play, here are zero" is a different request from
        /// "no tools", and several services answer it differently.
        #[test]
        fn no_tools_means_the_field_is_absent() {
            let sent = body(&provider("http://x/v1", "", "m"), &[], None);
            assert!(sent.get("tools").is_none());

            let empty = serde_json::json!([]);
            let sent = body(&provider("http://x/v1", "", "m"), &[], Some(&empty));
            assert!(sent.get("tools").is_none(), "an empty list was sent as one");
        }

        #[test]
        fn the_tool_list_rides_along_when_there_is_one() {
            let tools = crate::ai::tools::as_request();
            let sent = body(&provider("http://x/v1", "", "m"), &[], Some(&tools));
            assert!(sent["tools"]
                .as_array()
                .is_some_and(|list| !list.is_empty()));
        }
    }

    mod reading_the_stream {
        use super::*;

        #[test]
        fn a_chunk_of_the_answer_comes_out_as_text() {
            let line = r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#;
            assert_eq!(parse_line(line), Event::Text("Hel".into()));
        }

        #[test]
        fn the_end_is_marked_plainly() {
            assert_eq!(parse_line("data: [DONE]"), Event::Done);
            assert_eq!(parse_line("data:[DONE]"), Event::Done);
        }

        /// The first chunk carries the role and no content, and the last
        /// carries a finish reason and no content. Neither is text.
        #[test]
        fn a_chunk_with_no_content_is_not_text() {
            for line in [
                r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#,
                r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
                r#"data: {"choices":[{"delta":{"content":""}}]}"#,
            ] {
                assert_eq!(parse_line(line), Event::Ignored, "{line}");
            }
        }

        /// Comments, blank lines and other fields are part of the format.
        #[test]
        fn everything_that_is_not_data_is_ignored() {
            for line in [
                "",
                "   ",
                ": a comment keeping the connection open",
                "event: message",
                "id: 42",
                "data: not json",
                "{\"choices\":[]}",
            ] {
                assert_eq!(parse_line(line), Event::Ignored, "{line}");
            }
        }

        /// Two spellings for the same thing, one older and widely copied,
        /// one newer. Either is thinking rather than words.
        #[test]
        fn reasoning_comes_out_as_thinking() {
            let older = r#"data: {"choices":[{"delta":{"reasoning_content":"hmm"}}]}"#;
            let newer = r#"data: {"choices":[{"delta":{"reasoning":"hmm"}}]}"#;

            assert_eq!(parse_line(older), Event::Thinking("hmm".into()));
            assert_eq!(parse_line(newer), Event::Thinking("hmm".into()));

            // An empty one is nothing, the same as empty content.
            let empty = r#"data: {"choices":[{"delta":{"reasoning_content":""}}]}"#;
            assert_eq!(parse_line(empty), Event::Ignored);
        }

        /// The last line once usage was asked for: no choices, just numbers.
        #[test]
        fn a_chunk_carrying_only_usage_says_what_it_cost() {
            let line = r#"data: {"choices":[],"model":"m","usage":{"prompt_tokens":12,"completion_tokens":34,"total_tokens":46}}"#;
            assert_eq!(
                parse_line(line),
                Event::Usage {
                    usage: Usage {
                        input: 12,
                        output: 34
                    },
                    cost: None,
                }
            );

            // The other spelling, from a gateway in front of Anthropic.
            let line = r#"data: {"choices":[],"usage":{"input_tokens":1,"output_tokens":2}}"#;
            assert_eq!(
                parse_line(line),
                Event::Usage {
                    usage: Usage {
                        input: 1,
                        output: 2
                    },
                    cost: None,
                }
            );
        }

        /// OpenRouter's usage chunk, once asked for, carries the dollars.
        #[test]
        fn a_usage_chunk_naming_the_dollars_is_read_with_them() {
            let line = r#"data: {"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":20,"cost":0.00042}}"#;
            assert_eq!(
                parse_line(line),
                Event::Usage {
                    usage: Usage {
                        input: 10,
                        output: 20
                    },
                    cost: Some(0.00042),
                }
            );

            // A cost that cannot be a cost is no cost at all.
            let line = r#"data: {"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":20,"cost":-1}}"#;
            assert!(matches!(parse_line(line), Event::Usage { cost: None, .. }));
        }

        /// The sum is asked of the one service that does it, and of nobody
        /// else: OpenAI refuses a request carrying a field it does not know.
        #[test]
        fn the_dollars_are_asked_for_only_where_they_are_answered() {
            let mut provider = Provider::default();
            provider.model = "m".to_string();

            provider.base_url = "https://openrouter.ai/api/v1".to_string();
            let sent = body(&provider, &[], None);
            assert_eq!(sent["usage"]["include"], true);

            provider.base_url = "https://api.openai.com/v1".to_string();
            let sent = body(&provider, &[], None);
            assert!(sent.get("usage").is_none(), "{sent}");
        }

        /// The first chunk carries the role and the model and nothing else.
        /// The model is worth knowing: a gateway asked for an alias answers
        /// with whichever real one it chose.
        #[test]
        fn the_first_chunk_names_the_model() {
            let line = r#"data: {"model":"qwen3:1.7b","choices":[{"delta":{"role":"assistant"}}]}"#;
            assert_eq!(parse_line(line), Event::Model("qwen3:1.7b".into()));

            // Words are words, however many times the name rides along.
            let line = r#"data: {"model":"qwen3:1.7b","choices":[{"delta":{"content":"x"}}]}"#;
            assert_eq!(parse_line(line), Event::Text("x".into()));
        }

        /// Taken from a real Ollama stream: the whole call in one chunk, with
        /// its id and index, an empty content beside it and the role repeated.
        #[test]
        fn a_whole_call_in_one_chunk_the_way_ollama_sends_it() {
            let line = r#"data: {"id":"chatcmpl-125","object":"chat.completion.chunk","created":1788631012,"model":"huihui_ai/qwen3.5-abliterated:9b","system_fingerprint":"fp_ollama","choices":[{"index":0,"delta":{"role":"assistant","content":"","tool_calls":[{"id":"call_z6ed1hvv","index":0,"type":"function","function":{"name":"list_windows","arguments":"{}"}}]},"finish_reason":null}]}"#;

            let Event::Calling(piece) = parse_line(line) else {
                panic!("not a call: {:?}", parse_line(line));
            };

            let mut calls = Calls::default();
            calls.take(piece);
            let calls = calls.finish();

            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].id, "call_z6ed1hvv");
            assert_eq!(calls[0].function.name, "list_windows");
            assert_eq!(calls[0].function.arguments, "{}");
        }

        /// Whitespace around the payload varies between services.
        #[test]
        fn the_space_after_data_is_optional() {
            let with = r#"data: {"choices":[{"delta":{"content":"x"}}]}"#;
            let without = r#"data:{"choices":[{"delta":{"content":"x"}}]}"#;

            assert_eq!(parse_line(with), Event::Text("x".into()));
            assert_eq!(parse_line(without), Event::Text("x".into()));
        }
    }

    /// Against the Ollama running on this machine.
    ///
    /// Ignored, because it needs a service and a model and takes as long as
    /// the model takes. It is the only proof that the parser reads what a real
    /// service sends rather than what a fixture says it sends.
    ///
    ///   cargo test --lib ai::openai::tests::live -- --ignored --nocapture
    mod live {
        use super::*;

        const MODEL: &str = "huihui_ai/qwen3.5-abliterated:9b";

        fn ollama() -> Provider {
            provider("http://localhost:11434/v1", "", MODEL)
        }

        fn never() -> bool {
            false
        }

        #[tokio::test]
        #[ignore]
        async fn a_thinking_model_thinks_then_answers_and_says_what_it_cost() {
            let client = reqwest::Client::new();
            let mut thinking = String::new();
            let mut text = String::new();
            let mut order: Vec<&str> = Vec::new();

            let said = ask(
                &client,
                &ollama(),
                &[Message::user(
                    "What is 17 times 23? Answer in one short sentence.",
                )],
                None,
                &never,
                |piece| match piece {
                    Piece::Thinking(t) => {
                        if order.last() != Some(&"thinking") {
                            order.push("thinking");
                        }
                        thinking.push_str(&t);
                    }
                    Piece::Text(t) => {
                        if order.last() != Some(&"text") {
                            order.push("text");
                        }
                        text.push_str(&t);
                    }
                },
            )
            .await
            .expect("answered");

            eprintln!(
                "thinking {} chars, text {text:?}, model {:?}, usage {:?}",
                thinking.len(),
                said.model,
                said.usage
            );

            assert!(!thinking.is_empty(), "no thinking arrived");
            assert!(text.contains("391"), "{text}");
            assert_eq!(order, vec!["thinking", "text"]);
            assert_eq!(said.model, MODEL);
            assert!(said.usage.is_some_and(|u| u.output > 0), "{:?}", said.usage);
            assert!(said.calls.is_empty());
        }

        /// Writes the exact request the tool test sends, so it can be replayed
        /// with curl when the model does something the parser did not expect.
        #[test]
        #[ignore]
        fn dump_the_tool_request() {
            let tools = crate::ai::tools::as_request();
            let sent = body(
                &ollama(),
                &[Message::user(
                    "Which windows are open on this machine right now? Use a tool to find out.",
                )],
                Some(&tools),
            );
            std::fs::write(
                std::env::temp_dir().join("sill-tool-request.json"),
                serde_json::to_string_pretty(&sent).expect("json"),
            )
            .expect("written");
        }

        #[tokio::test]
        #[ignore]
        async fn a_question_about_this_machine_reaches_for_a_tool() {
            let client = reqwest::Client::new();
            let tools = crate::ai::tools::as_request();

            let said = ask(
                &client,
                &ollama(),
                &[Message::user(
                    "Which windows are open on this machine right now? Use a tool to find out.",
                )],
                Some(&tools),
                &never,
                |_| {},
            )
            .await
            .expect("answered");

            eprintln!(
                "calls {:?}, text {:?}, thought {} chars: {:?}",
                said.calls,
                said.text,
                said.thinking.len(),
                said.thinking.chars().rev().take(300).collect::<String>().chars().rev().collect::<String>()
            );

            assert!(!said.calls.is_empty(), "no tool was asked for: {}", said.text);
            assert_eq!(said.calls[0].function.name, "list_windows");
            assert!(!said.calls[0].id.is_empty(), "the call has no id to answer");
        }
    }
}
