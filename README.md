# Stagehand Rust SDK [ALPHA]

[![Crates.io](https://img.shields.io/crates/v/stagehand_sdk.svg)](https://crates.io/crates/stagehand_sdk)
[![Documentation](https://img.shields.io/badge/docs-API%20Reference-blue)](https://github.com/browserbase/stagehand-rust#api-reference)
[![License](https://img.shields.io/crates/l/stagehand_sdk.svg)](https://github.com/browserbase/stagehand-rust/blob/main/LICENSE)
[![Downloads](https://img.shields.io/crates/d/stagehand_sdk.svg)](https://crates.io/crates/stagehand_sdk)

A Rust client library for [Stagehand](https://stagehand.dev), the AI-powered browser automation framework. This SDK provides an async-first, type-safe interface for controlling browsers and performing AI-driven web interactions.

> [!CAUTION]
> This is an ALPHA release and is not production-ready.
> Please provide feedback and let us know if you have feature requests / bug reports!

## Features

- **Browserbase Cloud Support**: Drive Browserbase cloud browser sessions
- **AI-Driven Actions**: Use natural language instructions to interact with web pages
- **Structured Data Extraction**: Extract typed data from pages using Serde schemas
- **Element Observation**: Identify and analyze interactive elements on pages
- **Agent Execution**: Run multi-step AI agents with the `execute` method
- **Streaming Responses**: Real-time progress updates via Server-Sent Events (SSE)
- **CDP Access**: Get the CDP WebSocket URL to connect external tools like chromiumoxide

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [API Reference](#api-reference)
  - [Stagehand::connect](#stagehandconnect)
  - [start](#start)
  - [act](#act)
  - [extract](#extract)
  - [observe](#observe)
  - [execute](#execute)
  - [end](#end)
  - [browserbase_cdp_url](#browserbase_cdp_url)
- [Examples](#examples)
- [Error Handling](#error-handling)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
stagehand_sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dotenvy = "0.15"
```

### Runtime Support

The SDK supports both **tokio** and **async-std** runtimes. Tokio is enabled by default.

**Using tokio (default):**
```toml
[dependencies]
stagehand_sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**Using async-std:**
```toml
[dependencies]
stagehand_sdk = { version = "0.3", default-features = false, features = ["async-std-runtime"] }
async-std = { version = "1", features = ["attributes"] }
```

## Quick Start

```rust
use stagehand_sdk::{Stagehand, V3Options, Env, Model, TransportChoice};
use stagehand_sdk::{ActResponseEvent, ExtractResponseEvent};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
struct Quote {
    text: String,
    author: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Environment variables required:
    // - BROWSERBASE_API_KEY
    // - BROWSERBASE_PROJECT_ID
    // - A model API key (OPENAI_API_KEY, ANTHROPIC_API_KEY, GOOGLE_GENERATIVE_AI_API_KEY, etc.)

    // 1. Connect to Stagehand cloud API (uses STAGEHAND_API_URL env var or default)
    let mut stagehand = Stagehand::connect(TransportChoice::default_rest()).await?;

    // 2. Start session
    let opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        verbose: Some(2),
        ..Default::default()
    };

    stagehand.start(opts).await?;
    println!("Session ID: {:?}", stagehand.session_id());

    // 3. Navigate to a page
    let mut act_stream = stagehand.act(
        "Go to https://quotes.toscrape.com/",
        None,
        HashMap::new(),
        Some(60_000),
        None,
    ).await?;

    while let Some(res) = act_stream.next().await {
        if let Ok(response) = res {
            if let Some(ActResponseEvent::Success(success)) = response.event {
                println!("Navigation success: {}", success);
            }
        }
    }

    // 4. Extract structured data
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string" },
            "author": { "type": "string" }
        }
    });
    let mut extract_stream = stagehand.extract(
        "Extract the first quote on the page",
        schema,
        None,
        Some(60_000),
        None,
        None,
    ).await?;

    while let Some(res) = extract_stream.next().await {
        if let Ok(response) = res {
            if let Some(ExtractResponseEvent::DataJson(json)) = response.event {
                println!("Quote: {}", json);
            }
        }
    }

    // 5. End session
    stagehand.end().await?;
    Ok(())
}
```

## Configuration

### Environment Variables

Create a `.env` file in your project root:

```env
# Browserbase API credentials (required)
BROWSERBASE_API_KEY=your_browserbase_api_key_here
BROWSERBASE_PROJECT_ID=your_browserbase_project_id_here

# Model API key (at least one required, checked in this order)
MODEL_API_KEY=your_api_key                          # Generic override (highest priority)
OPENAI_API_KEY=your_openai_key                      # OpenAI
ANTHROPIC_API_KEY=your_anthropic_key                # Anthropic (Claude)
GOOGLE_GENERATIVE_AI_API_KEY=your_gemini_key        # Google Gemini
AZURE_API_KEY=your_azure_key                        # Azure OpenAI
MISTRAL_API_KEY=your_mistral_key                    # Mistral
GROQ_API_KEY=your_groq_key                          # Groq
CEREBRAS_API_KEY=your_cerebras_key                  # Cerebras
DEEPSEEK_API_KEY=your_deepseek_key                  # DeepSeek

# Optional: Custom API URLs
STAGEHAND_API_URL=https://api.stagehand.browserbase.com/v1  # Stagehand API (default)
BROWSERBASE_API_URL=https://api.browserbase.com/v1          # Browserbase API (default)
```

The SDK checks for model API keys in the order listed above and uses the first one found.

### V3Options

The main configuration struct for initializing Stagehand:

```rust
pub struct V3Options {
    // Environment: Local or Browserbase
    pub env: Option<Env>,

    // Browserbase credentials (auto-loaded from env vars)
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub browserbase_session_id: Option<String>,
    pub browserbase_session_create_params: Option<serde_json::Value>,

    // Local browser options (coming soon)
    // pub local_browser_launch_options: Option<LocalBrowserLaunchOptions>,

    // AI model configuration
    pub model: Option<Model>,
    pub system_prompt: Option<String>,

    // Behavior settings
    pub self_heal: Option<bool>,
    pub wait_for_captcha_solves: Option<bool>,
    pub experimental: Option<bool>,
    pub dom_settle_timeout_ms: Option<u32>,
    pub act_timeout_ms: Option<u32>,

    // Logging verbosity (0, 1, or 2)
    pub verbose: Option<i32>,
}
```

### Model Configuration

Specify AI models in two ways:

```rust
// Simple string format (recommended)
let model = Model::String("openai/gpt-5-nano".into());

// Detailed configuration with custom API key/base URL
let model = Model::Config {
    model_name: "gpt-5-nano".to_string(),
    api_key: Some("sk-...".to_string()),
    base_url: Some("https://api.openai.com/v1".to_string()),
};
```

## API Reference

### `Stagehand::connect`

Establishes a connection to the Stagehand service.

```rust
pub async fn connect(
    transport_choice: TransportChoice,
) -> Result<Self, StagehandError>
```

**Parameters:**

- `transport_choice` - `TransportChoice::Rest(base_url)` for REST API with explicit URL, or use `TransportChoice::default_rest()` to use the `STAGEHAND_API_URL` env var (falls back to default)

**Example:**

```rust
// Using default (recommended) - checks STAGEHAND_API_URL env var, falls back to default
let stagehand = Stagehand::connect(TransportChoice::default_rest()).await?;

// Or with explicit URL
let stagehand = Stagehand::connect(
    TransportChoice::Rest("https://api.stagehand.browserbase.com/v1".to_string()),
).await?;
```

---

### `start`

Starts a browser session.

```rust
pub async fn start(&mut self, opts: V3Options) -> Result<(), StagehandError>
```

**Example:**

```rust
let opts = V3Options {
    env: Some(Env::Browserbase),
    model: Some(Model::String("openai/gpt-5-nano".into())),
    verbose: Some(1),
    ..Default::default()
};

stagehand.start(opts).await?;
println!("Session: {}", stagehand.session_id().unwrap());
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
    timeout: Option<u32>,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<ActResponse, StagehandError>> + Send>>, StagehandError>
```

**Parameters:**

- `instruction` - Natural language instruction (e.g., "Click the login button")
- `model` - Override the default AI model
- `variables` - Variable substitution map for the instruction
- `timeout` - Operation timeout in milliseconds
- `frame_id` - Target a specific iframe

**Response Events:**

- `ActResponseEvent::Log(LogLine)` - Progress logs
- `ActResponseEvent::Success(bool)` - Action completion status

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
        if let Some(ActResponseEvent::Success(success)) = response.event {
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
    timeout: Option<u32>,
    selector: Option<String>,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<ExtractResponse, StagehandError>> + Send>>, StagehandError>
```

**Parameters:**

- `instruction` - What data to extract
- `schema` - A Serde-serializable struct defining the expected shape
- `model` - Override the default AI model
- `timeout` - Operation timeout
- `selector` - CSS selector to narrow extraction scope
- `frame_id` - Target a specific iframe

**Response Events:**

- `ExtractResponseEvent::Log(LogLine)` - Progress logs
- `ExtractResponseEvent::DataJson(String)` - JSON string matching the schema

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
        if let Some(ExtractResponseEvent::DataJson(json)) = response.event {
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
    timeout: Option<u32>,
    selector: Option<String>,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<ObserveResponse, StagehandError>> + Send>>, StagehandError>
```

**Parameters:**

- `instruction` - Optional AI instruction for analysis
- `model` - Override the default AI model
- `timeout` - Operation timeout
- `selector` - CSS selector to narrow observation scope
- `frame_id` - Target a specific iframe

**Response Events:**

- `ObserveResponseEvent::Log(LogLine)` - Progress logs
- `ObserveResponseEvent::ElementsJson(String)` - JSON array of observed elements

**Example:**

```rust
let mut stream = stagehand.observe(
    Some("Find all clickable buttons".to_string()),
    None,
    Some(30_000),
    None,
    None,
).await?;

while let Some(res) = stream.next().await {
    if let Ok(response) = res {
        if let Some(ObserveResponseEvent::ElementsJson(json)) = response.event {
            println!("Elements: {}", json);
        }
    }
}
```

---

### `execute`

Executes an AI agent with multi-step capabilities.

```rust
pub async fn execute(
    &mut self,
    agent_config: AgentConfig,
    execute_options: AgentExecuteOptions,
    frame_id: Option<String>,
) -> Result<Pin<Box<dyn Stream<Item = Result<ExecuteResponse, StagehandError>> + Send>>, StagehandError>
```

**Parameters:**

- `agent_config` - Agent configuration (provider, model, system prompt, CUA mode)
- `execute_options` - Execution options (instruction, max steps, highlight cursor)
- `frame_id` - Target a specific iframe

**Response Events:**

- `ExecuteResponseEvent::Log(LogLine)` - Execution progress
- `ExecuteResponseEvent::ResultJson(String)` - Final result

**Example:**

```rust
use stagehand_sdk::{AgentConfig, AgentExecuteOptions, ModelConfiguration};

let agent_config = AgentConfig {
    provider: None,
    model: Some(ModelConfiguration::String("openai/gpt-5-nano".into())),
    system_prompt: None,
    cua: None,
};

let execute_options = AgentExecuteOptions {
    instruction: "What is the URL of the current page?".to_string(),
    max_steps: Some(10),
    highlight_cursor: None,
};

let mut stream = stagehand.execute(
    agent_config,
    execute_options,
    None,
).await?;

while let Some(res) = stream.next().await {
    if let Ok(response) = res {
        if let Some(ExecuteResponseEvent::ResultJson(result)) = response.event {
            println!("Result: {}", result);
        }
    }
}
```

---

### `end`

Ends the browser session.

```rust
pub async fn end(&mut self) -> Result<(), StagehandError>
```

**Example:**

```rust
stagehand.end().await?;
```

---

### `browserbase_cdp_url`

Returns the CDP WebSocket URL for connecting external tools like chromiumoxide.

```rust
pub fn browserbase_cdp_url(&self) -> Option<String>
```

The URL format is: `wss://connect.browserbase.com?sessionId={sessionId}&apiKey={apiKey}`

**Example:**

```rust
// After init(), get the CDP URL to connect chromiumoxide
let cdp_url = stagehand.browserbase_cdp_url()
    .expect("CDP URL available after init");

// Connect chromiumoxide to the remote browser
let (browser, handler) = Browser::connect(&cdp_url).await?;
```

See [`tests/chromiumoxide_integration.rs`](tests/chromiumoxide_integration.rs) for a complete example.

## Examples

### Full Integration Example

See [`tests/browserbase_live.rs`](tests/browserbase_live.rs) for a complete working example that demonstrates act, extract, and execute.

### Chromiumoxide Integration

See [`tests/chromiumoxide_integration.rs`](tests/chromiumoxide_integration.rs) for connecting chromiumoxide to a Browserbase session:

```rust
use chromiumoxide::browser::Browser;
use stagehand_sdk::{Stagehand, V3Options, Env, Model, TransportChoice};

async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Create Stagehand session
    let mut stagehand = Stagehand::connect(TransportChoice::default_rest()).await?;

    stagehand.start(V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        ..Default::default()
    }).await?;

    // 2. Get CDP URL and connect chromiumoxide
    let cdp_url = stagehand.browserbase_cdp_url().unwrap();
    let (browser, mut handler) = Browser::connect(&cdp_url).await?;

    // Spawn handler
    tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() { break; }
        }
    });

    // 3. Use chromiumoxide for direct CDP control
    let page = browser.pages().await?.into_iter().next().unwrap();
    let screenshot = page.screenshot(Default::default()).await?;

    // 4. Or use Stagehand's AI methods
    let mut stream = stagehand.act("Click the login button", None, Default::default(), None, None).await?;
    // ...

    stagehand.end().await?;
    Ok(())
}
```

## Error Handling

The SDK uses `StagehandError` for all error cases:

```rust
pub enum StagehandError {
    Transport(String),      // Network/connection errors
    Api(String),            // API response errors
    MissingApiKey(String),  // Missing required environment variable
}
```

All errors implement `std::error::Error` and `Display`.

## Running Tests

```bash
# Set up environment variables
cp .env.example .env
# Edit .env with your credentials

# Run all tests
cargo test

# Run specific integration test with output
cargo test test_browserbase_live_extract -- --nocapture

# Run chromiumoxide integration test
cargo test test_chromiumoxide_browserbase_connection -- --nocapture
```

## License

Apache-2.0

## Links

- [GitHub Repository: `stagehand-rust`](https://github.com/browserbase/stagehand-rust)
- [Crates.io: `stagehand_sdk`](https://crates.io/crates/stagehand_sdk)
- [Stagehand REST API Documentation](https://stagehand.stldocs.app/api)
- [Stagehand TS Library Documentation](https://docs.stagehand.dev)
- [Browserbase Cloud](https://browserbase.com)
