# Stagehand Rust SDK [ALPHA]

A Rust client library for [Stagehand](https://stagehand.dev), the AI-powered browser automation framework. This SDK provides an async-first, type-safe interface for controlling browsers and performing AI-driven web interactions.

> [!CAUTION]
> This is an ALPHA release and may not be production-ready.  
> Please provide feedback and let us know if you have feature requests / bug reports!

## Features

- **Dual Transport Support**: Connect via gRPC (local/self-hosted) or REST API with Server-Sent Events (Browserbase cloud)
- **AI-Driven Actions**: Use natural language instructions to interact with web pages
- **Structured Data Extraction**: Extract typed data from pages using Serde schemas
- **Element Observation**: Identify and analyze interactive elements on pages
- **Streaming Responses**: Real-time progress updates via async streams
- **Type Safety**: Full Rust type safety with generated protobuf types

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [API Reference](#api-reference)
  - [Stagehand::connect](#stagehandconnect)
  - [init](#init)
  - [act](#act)
  - [extract](#extract)
  - [observe](#observe)
  - [execute](#execute)
  - [close](#close)
- [Transport Modes](#transport-modes)
- [Examples](#examples)
- [Error Handling](#error-handling)
- [Proto Specification](#proto-specification)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
stagehand-sdk = { git = "https://github.com/browserbase/stagehand-rust-sdk" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Quick Start

### Using Browserbase Cloud (REST API)

```rust
use stagehand_sdk::{Stagehand, V3Options, Env, Model, Transport};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Quote {
    text: String,
    author: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Environment variables required:
    // - BROWSERBASE_API_KEY
    // - BROWSERBASE_PROJECT_ID
    // - OPENAI_API_KEY

    // 1. Connect to Stagehand cloud API
    let mut stagehand = Stagehand::connect(
        "https://api.stagehand.browserbase.com/v1".to_string(),
        Transport::Rest("https://api.stagehand.browserbase.com/v1".to_string()),
    ).await?;

    // 2. Initialize session
    let opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-4o".into())),
        verbose: Some(2),
        ..Default::default()
    };

    let mut init_stream = stagehand.init(opts).await?;
    while let Some(res) = init_stream.next().await {
        // Handle initialization events...
    }

    // 3. Navigate to a page
    let mut act_stream = stagehand.act(
        "Go to https://quotes.toscrape.com/",
        None,
        Default::default(),
        Some(60_000),
        None,
    ).await?;
    while let Some(_) = act_stream.next().await {}

    // 4. Extract structured data
    let schema = Quote { text: String::new(), author: String::new() };
    let mut extract_stream = stagehand.extract(
        "Extract the first quote on the page",
        &schema,
        None,
        Some(60_000),
        None,
        None,
    ).await?;

    while let Some(res) = extract_stream.next().await {
        if let Ok(response) = res {
            if let Some(stagehand_sdk::proto::extract_response::Event::DataJson(json)) = response.event {
                let quote: Quote = serde_json::from_str(&json)?;
                println!("Quote: {} - {}", quote.text, quote.author);
            }
        }
    }

    // 5. Clean up
    stagehand.close(true).await?;
    Ok(())
}
```

## Configuration

### Environment Variables

Create a `.env` file in your project root:

```env
# Browserbase API credentials (required for REST transport)
BROWSERBASE_API_KEY=your_browserbase_api_key_here
BROWSERBASE_PROJECT_ID=your_browserbase_project_id_here

# OpenAI API key (required for LLM operations)
OPENAI_API_KEY=your_openai_api_key_here
```

### V3Options

The main configuration struct for initializing Stagehand:

```rust
pub struct V3Options {
    // Environment: Local or Browserbase
    pub env: Option<Env>,

    // Browserbase credentials (auto-loaded from env vars for REST)
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub browserbase_session_id: Option<String>,
    pub browserbase_session_create_params: Option<serde_json::Value>,

    // Local browser options
    pub local_browser_launch_options: Option<LocalBrowserLaunchOptions>,

    // AI model configuration
    pub model: Option<Model>,
    pub system_prompt: Option<String>,

    // Behavior settings
    pub self_heal: Option<bool>,
    pub experimental: Option<bool>,
    pub dom_settle_timeout: Option<u32>,
    pub cache_dir: Option<String>,

    // Logging
    pub verbose: Option<i32>,  // 0, 1, or 2
    pub log_inference_to_file: Option<bool>,
    pub disable_pino: Option<bool>,
}
```

### Model Configuration

Specify AI models in two ways:

```rust
// Simple string format
let model = Model::String("openai/gpt-4o".into());

// Detailed configuration with custom API key/base URL
let model = Model::Config {
    model_name: "gpt-4o".to_string(),
    api_key: Some("sk-...".to_string()),
    base_url: Some("https://api.openai.com/v1".to_string()),
};
```

### Local Browser Options

For local browser automation (gRPC transport):

```rust
pub struct LocalBrowserLaunchOptions {
    pub headless: Option<bool>,
    pub executable_path: Option<String>,
    pub args: Vec<String>,
    pub user_data_dir: Option<String>,
    pub viewport: Option<(i32, i32)>,  // (width, height)
    pub devtools: Option<bool>,
    pub ignore_https_errors: Option<bool>,
    pub cdp_url: Option<String>,  // Connect to existing browser via CDP
}
```

## API Reference

### `Stagehand::connect`

Establishes a connection to the Stagehand service.

```rust
pub async fn connect(
    dst: String,
    transport_type: Transport,
) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
```

**Parameters:**
- `dst` - The destination URL (used for reference)
- `transport_type` - Either `Transport::Grpc(url)` or `Transport::Rest(base_url)`

**Example:**
```rust
// REST API (Browserbase cloud)
let stagehand = Stagehand::connect(
    "https://api.stagehand.browserbase.com/v1".to_string(),
    Transport::Rest("https://api.stagehand.browserbase.com/v1".to_string()),
).await?;

// gRPC (local/self-hosted)
let stagehand = Stagehand::connect(
    "http://127.0.0.1:50051".to_string(),
    Transport::Grpc("http://127.0.0.1:50051".to_string()),
).await?;
```

---

### `init`

Initializes a browser session. Returns a stream of log events and the final result.

```rust
pub async fn init(
    &mut self,
    opts: V3Options,
) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, tonic::Status>> + Send>>, tonic::Status>
```

**Response Events:**
- `InitResponse::Log(LogLine)` - Progress log messages
- `InitResponse::Result(InitResult)` - Session initialization complete (contains session ID for REST)

**Example:**
```rust
let mut stream = stagehand.init(opts).await?;
while let Some(res) = stream.next().await {
    match res {
        Ok(response) => match response.event {
            Some(proto::init_response::Event::Log(log)) => {
                println!("[{}] {}", log.category, log.message);
            }
            Some(proto::init_response::Event::Result(result)) => {
                println!("Session initialized: {}", result.unused);
            }
            _ => {}
        },
        Err(e) => eprintln!("Error: {:?}", e),
    }
}
```

---

### `act`

Performs browser actions based on natural language instructions.

```rust
pub async fn act(
    &mut self,
    instruction: impl Into<String>,
    model: Option<Model>,
    variables: HashMap<String, String>,
    timeout_ms: Option<u32>,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, tonic::Status>> + Send>>, tonic::Status>
```

**Parameters:**
- `instruction` - Natural language instruction (e.g., "Click the login button")
- `model` - Override the default AI model
- `variables` - Variable substitution map for the instruction
- `timeout_ms` - Operation timeout in milliseconds
- `frame_id` - Target a specific iframe

**Response Events:**
- `ActResponse::Log(LogLine)` - Progress logs
- `ActResponse::Success(bool)` - Action completion status

**Example:**
```rust
let mut stream = stagehand.act(
    "Navigate to https://example.com and click 'More information...'",
    None,
    HashMap::new(),
    Some(60_000),
    None,
).await?;

while let Some(res) = stream.next().await {
    if let Ok(response) = res {
        if let Some(proto::act_response::Event::Success(success)) = response.event {
            println!("Action succeeded: {}", success);
        }
    }
}
```

---

### `extract`

Extracts structured data from web pages using a schema.

```rust
pub async fn extract<S: Serialize>(
    &mut self,
    instruction: impl Into<String>,
    schema: &S,
    model: Option<Model>,
    timeout_ms: Option<u32>,
    selector: Option<String>,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, tonic::Status>> + Send>>, tonic::Status>
```

**Parameters:**
- `instruction` - What data to extract
- `schema` - A Serde-serializable struct defining the expected shape
- `model` - Override the default AI model
- `timeout_ms` - Operation timeout
- `selector` - CSS selector to narrow extraction scope
- `frame_id` - Target a specific iframe

**Response Events:**
- `ExtractResponse::Log(LogLine)` - Progress logs
- `ExtractResponse::DataJson(String)` - JSON string matching the schema

**Example:**
```rust
#[derive(Serialize, Deserialize, Debug)]
struct ProductInfo {
    name: String,
    price: String,
    description: String,
}

let schema = ProductInfo {
    name: String::new(),
    price: String::new(),
    description: String::new(),
};

let mut stream = stagehand.extract(
    "Extract the product information from this page",
    &schema,
    None,
    Some(30_000),
    None,
    None,
).await?;

while let Some(res) = stream.next().await {
    if let Ok(response) = res {
        if let Some(proto::extract_response::Event::DataJson(json)) = response.event {
            let product: ProductInfo = serde_json::from_str(&json)?;
            println!("Product: {:?}", product);
        }
    }
}
```

---

### `observe`

Identifies interactive elements on a page.

```rust
pub async fn observe(
    &mut self,
    instruction: Option<String>,
    model: Option<Model>,
    timeout_ms: Option<u32>,
    selector: Option<String>,
    only_selectors: Vec<String>,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, tonic::Status>> + Send>>, tonic::Status>
```

**Parameters:**
- `instruction` - Optional AI instruction for analysis
- `model` - Override the default AI model
- `timeout_ms` - Operation timeout
- `selector` - CSS selector to narrow observation scope
- `only_selectors` - Limit to specific selectors
- `frame_id` - Target a specific iframe

**Response Events:**
- `ObserveResponse::Log(LogLine)` - Progress logs
- `ObserveResponse::ElementsJson(String)` - JSON array of observed elements

**Example:**
```rust
#[derive(Deserialize, Debug)]
struct ObservedElement {
    selector: String,
    description: String,
}

let mut stream = stagehand.observe(
    Some("Find all clickable buttons".to_string()),
    None,
    Some(30_000),
    None,
    vec![],
    None,
).await?;

while let Some(res) = stream.next().await {
    if let Ok(response) = res {
        if let Some(proto::observe_response::Event::ElementsJson(json)) = response.event {
            let elements: Vec<ObservedElement> = serde_json::from_str(&json)?;
            for el in elements {
                println!("Found: {} ({})", el.description, el.selector);
            }
        }
    }
}
```

---

### `execute`

Executes JavaScript or agent instructions.

```rust
pub async fn execute(
    &mut self,
    session_id: String,
    instruction: String,
    frame_id: Option<String>,
    agent_config: Option<AgentConfig>,
    execute_options: Option<AgentExecuteOptions>,
) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, tonic::Status>> + Send>>, tonic::Status>
```

**Parameters:**
- `session_id` - The active session ID
- `instruction` - JavaScript code or agent instruction
- `frame_id` - Target a specific iframe
- `agent_config` - Advanced agent configuration
- `execute_options` - Execution parameters

**Response Events:**
- `ExecuteResponse::Progress(String)` - Execution progress
- `ExecuteResponse::Result(String)` - Final result

**Example:**
```rust
let mut stream = stagehand.execute(
    session_id,
    "Return the current page's URL".to_string(),
    Some("main".to_string()),
    None,
    Some(AgentExecuteOptions {
        instruction: "Return the current page's URL".to_string(),
        page: Some("main".to_string()),
        timeout: Some(10_000),
    }),
).await?;

while let Some(res) = stream.next().await {
    if let Ok(response) = res {
        if let Some(proto::execute_response::Event::Result(result)) = response.event {
            println!("Result: {}", result);
        }
    }
}
```

---

### `close`

Closes the browser session.

```rust
pub async fn close(&mut self, force: bool) -> Result<(), tonic::Status>
```

**Parameters:**
- `force` - Force close the session immediately

**Example:**
```rust
stagehand.close(true).await?;
```

## Transport Modes

### REST API (Browserbase Cloud)

The REST transport connects to Browserbase's managed browser infrastructure using Server-Sent Events (SSE) for streaming.

```rust
let stagehand = Stagehand::connect(
    "https://api.stagehand.browserbase.com/v1".to_string(),
    Transport::Rest("https://api.stagehand.browserbase.com/v1".to_string()),
).await?;
```

**Required environment variables:**
- `BROWSERBASE_API_KEY`
- `BROWSERBASE_PROJECT_ID`
- `OPENAI_API_KEY` (for OpenAI models)

### gRPC (Local/Self-Hosted)

The gRPC transport connects to a local or self-hosted Stagehand server.

```rust
let stagehand = Stagehand::connect(
    "http://127.0.0.1:50051".to_string(),
    Transport::Grpc("http://127.0.0.1:50051".to_string()),
).await?;
```

## Examples

### Full Integration Example

See [`tests/browserbase_live.rs`](tests/browserbase_live.rs) for a complete working example:

```rust
use stagehand_sdk::{Stagehand, V3Options, Env, Model, Transport};
use tokio_stream::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Quote {
    text: String,
    author: String,
}

#[tokio::test]
async fn test_browserbase_live_extract() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();

    let mut stagehand = Stagehand::connect(
        "https://api.stagehand.browserbase.com/v1".to_string(),
        Transport::Rest("https://api.stagehand.browserbase.com/v1".to_string()),
    ).await?;

    let init_opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-4o".into())),
        ..Default::default()
    };

    let mut init_stream = stagehand.init(init_opts).await?;
    while let Some(_) = init_stream.next().await {}

    // Navigate
    let mut act_stream = stagehand.act(
        "Go to https://quotes.toscrape.com/",
        None,
        Default::default(),
        Some(60_000),
        None,
    ).await?;
    while let Some(_) = act_stream.next().await {}

    // Extract
    let schema = Quote { text: String::new(), author: String::new() };
    let mut extract_stream = stagehand.extract(
        "Extract the first quote on the page",
        &schema,
        None,
        Some(60_000),
        None,
        None,
    ).await?;

    let mut extracted: Option<Quote> = None;
    while let Some(res) = extract_stream.next().await {
        if let Ok(response) = res {
            if let Some(stagehand_sdk::proto::extract_response::Event::DataJson(json)) = response.event {
                extracted = Some(serde_json::from_str(&json)?);
            }
        }
    }

    stagehand.close(true).await?;

    assert!(extracted.is_some());
    assert_eq!(extracted.unwrap().author, "Albert Einstein");
    Ok(())
}
```

### Local Browser with Chromiumoxide

See [`tests/chromiumoxide_integration.rs`](tests/chromiumoxide_integration.rs) for using a local browser:

```rust
use chromiumoxide::browser::{Browser, BrowserConfig};
use stagehand_sdk::{Stagehand, V3Options, LocalBrowserLaunchOptions, Transport};

async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Launch local browser
    let (browser, handler) = Browser::launch(
        BrowserConfig::builder()
            .with_head()
            .window_size(1280, 720)
            .build()?,
    ).await?;

    // Get WebSocket URL for CDP connection
    let ws_url = browser.websocket_address();

    // Connect Stagehand to the running browser
    let mut stagehand = Stagehand::connect(
        "http://127.0.0.1:50051".to_string(),
        Transport::Grpc("http://127.0.0.1:50051".to_string()),
    ).await?;

    let opts = V3Options {
        local_browser_launch_options: Some(LocalBrowserLaunchOptions {
            cdp_url: Some(ws_url.to_string()),
            headless: Some(false),
            viewport: Some((1280, 720)),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Use stagehand...
    Ok(())
}
```

## Error Handling

The SDK provides a comprehensive error type:

```rust
pub enum StagehandAPIError {
    Http(reqwest::Error),
    Api(String),
    Unauthorized(String),
    ConnectionRefused(String),
    ResponseParse(String),
    ResponseBody(String),
    ServerError(String),
    MissingSessionId,
    MissingApiKey(String),
    TonicStatus(tonic::Status),
    EventSource(eventsource_client::Error),
    Timeout,
}
```

All errors implement `std::error::Error` and can be converted to `tonic::Status` for consistency.

## Proto Specification

The SDK is built on the following gRPC service definition ([`proto/stagehand.v1.proto`](proto/stagehand.v1.proto)):

```protobuf
service StagehandService {
  rpc Init (InitRequest) returns (stream InitResponse);
  rpc Act (ActRequest) returns (stream ActResponse);
  rpc Extract (ExtractRequest) returns (stream ExtractResponse);
  rpc Observe (ObserveRequest) returns (stream ObserveResponse);
  rpc Execute (ExecuteRequest) returns (stream ExecuteResponse);
  rpc Close (CloseRequest) returns (CloseResponse);
}
```

### Key Message Types

| Type | Description |
|------|-------------|
| `InitRequest` | Session configuration (env, credentials, browser options, AI model) |
| `ActRequest` | Natural language instruction with optional model/variables/timeout |
| `ExtractRequest` | Extraction instruction with JSON schema |
| `ObserveRequest` | Element observation with optional instruction/selectors |
| `ExecuteRequest` | JavaScript/agent code execution |
| `LogLine` | Structured log message (category, message, auxiliary JSON) |

## Running Tests

```bash
# Set up environment variables
cp .env.example .env
# Edit .env with your credentials

# Run all tests
cargo test

# Run specific integration test
cargo test test_browserbase_live_extract -- --nocapture

# Run local browser test
cargo test test_chromiumoxide_basic -- --nocapture
```

## License

MIT

## Links

- [Stagehand Documentation](https://docs.stagehand.dev)
- [Browserbase](https://browserbase.com)
- [GitHub Repository](https://github.com/browserbase/stagehand-rust-sdk)
