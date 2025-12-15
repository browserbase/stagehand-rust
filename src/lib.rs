use futures::{Stream, StreamExt};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use async_trait::async_trait;
use std::fmt;

// --- Rest API specific imports ---
use reqwest::Client;
use eventsource_client::{Client as SseClient, ClientBuilder, SSE};
use tokio_stream::wrappers::ReceiverStream;

// =============================================================================
// Native Response Types
// =============================================================================

/// Log line from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub status: Option<String>,
}

/// Result from init operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitResult {
    #[serde(default)]
    pub session_id: String,
}

/// Events that can occur during init
#[derive(Debug, Clone)]
pub enum InitResponseEvent {
    Log(LogLine),
    Result(InitResult),
}

/// Response from init operation
#[derive(Debug, Clone)]
pub struct InitResponse {
    pub event: Option<InitResponseEvent>,
}

/// Events that can occur during act
#[derive(Debug, Clone)]
pub enum ActResponseEvent {
    Log(LogLine),
    Success(bool),
}

/// Response from act operation
#[derive(Debug, Clone)]
pub struct ActResponse {
    pub event: Option<ActResponseEvent>,
}

/// Events that can occur during extract
#[derive(Debug, Clone)]
pub enum ExtractResponseEvent {
    Log(LogLine),
    DataJson(String),
}

/// Response from extract operation
#[derive(Debug, Clone)]
pub struct ExtractResponse {
    pub event: Option<ExtractResponseEvent>,
}

/// Events that can occur during observe
#[derive(Debug, Clone)]
pub enum ObserveResponseEvent {
    Log(LogLine),
    ElementsJson(String),
}

/// Response from observe operation
#[derive(Debug, Clone)]
pub struct ObserveResponse {
    pub event: Option<ObserveResponseEvent>,
}

/// Events that can occur during execute
#[derive(Debug, Clone)]
pub enum ExecuteResponseEvent {
    Log(LogLine),
    ResultJson(String),
}

/// Response from execute operation
#[derive(Debug, Clone)]
pub struct ExecuteResponse {
    pub event: Option<ExecuteResponseEvent>,
}

// =============================================================================
// Model Configuration Types (matches API exactly)
// =============================================================================

/// Model configuration object for API - uses camelCase field names
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelObj {
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "baseURL")]
    pub base_url: Option<String>,
}

/// Model configuration - always serializes as an object for proper API key inheritance
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ModelConfiguration {
    String(String),
    Object(ModelObj),
}

impl Serialize for ModelConfiguration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Always serialize as an object so the Stagehand agent inherits the API key
        match self {
            ModelConfiguration::String(s) => {
                let obj = ModelObj {
                    model_name: s.clone(),
                    api_key: None,
                    base_url: None,
                };
                obj.serialize(serializer)
            }
            ModelConfiguration::Object(obj) => obj.serialize(serializer),
        }
    }
}

// =============================================================================
// Agent Specific Structs (matching V3 API schema)
// =============================================================================

/// Agent config for agentExecute endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cua: Option<bool>,
}

/// Execute options for agentExecute endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecuteOptions {
    pub instruction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight_cursor: Option<bool>,
}

// =============================================================================
// Idiomatic Configuration Types
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Env {
    Local,
    Browserbase,
}

impl ToString for Env {
    fn to_string(&self) -> String {
        match self {
            Env::Local => "LOCAL".to_string(),
            Env::Browserbase => "BROWSERBASE".to_string(),
        }
    }
}

/// User-facing model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Model {
    String(String),
    Config {
        model_name: String,
        api_key: Option<String>,
        base_url: Option<String>,
    },
}

impl From<Model> for ModelConfiguration {
    fn from(m: Model) -> Self {
        match m {
            Model::String(s) => ModelConfiguration::String(s),
            Model::Config { model_name, api_key, base_url } => ModelConfiguration::Object(ModelObj {
                model_name,
                api_key,
                base_url,
            }),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalBrowserLaunchOptions {
    pub headless: Option<bool>,
    pub executable_path: Option<String>,
    pub args: Vec<String>,
    pub user_data_dir: Option<String>,
    pub viewport: Option<(i32, i32)>,
    pub devtools: Option<bool>,
    pub ignore_https_errors: Option<bool>,
    pub cdp_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct V3Options {
    pub env: Option<Env>,
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub browserbase_session_id: Option<String>,
    pub browserbase_session_create_params: Option<serde_json::Value>,
    pub local_browser_launch_options: Option<LocalBrowserLaunchOptions>,
    pub model: Option<Model>,
    pub system_prompt: Option<String>,
    pub self_heal: Option<bool>,
    pub wait_for_captcha_solves: Option<bool>,
    pub experimental: Option<bool>,
    pub dom_settle_timeout_ms: Option<u32>,
    pub act_timeout_ms: Option<u32>,
    pub verbose: Option<i32>,
}

// =============================================================================
// Transport Choice (abstraction layer for future transports)
// =============================================================================

/// Transport selection for connecting to Stagehand API
#[derive(Debug, Clone, PartialEq)]
pub enum TransportChoice {
    /// REST + SSE transport (the primary supported transport)
    Rest(String),
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug)]
pub enum StagehandError {
    Transport(String),
    Api(String),
    MissingApiKey(String),
}

impl fmt::Display for StagehandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StagehandError::Transport(msg) => write!(f, "Transport error: {}", msg),
            StagehandError::Api(msg) => write!(f, "API error: {}", msg),
            StagehandError::MissingApiKey(key) => write!(f, "Missing API key: {}", key),
        }
    }
}

impl std::error::Error for StagehandError {}

impl From<reqwest::Error> for StagehandError {
    fn from(err: reqwest::Error) -> Self {
        StagehandError::Transport(err.to_string())
    }
}

impl From<eventsource_client::Error> for StagehandError {
    fn from(err: eventsource_client::Error) -> Self {
        StagehandError::Transport(err.to_string())
    }
}

// =============================================================================
// Transport Abstraction Layer
// =============================================================================

/// Transport trait for Stagehand API communication
#[async_trait]
pub trait Transport: Send + Sync {
    async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<InitResponse, StagehandError>> + Send>>, StagehandError>;
    async fn act(&mut self, session_id: &str, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ActResponse, StagehandError>> + Send>>, StagehandError>;
    async fn extract(&mut self, session_id: &str, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ExtractResponse, StagehandError>> + Send>>, StagehandError>;
    async fn observe(&mut self, session_id: &str, instruction: Option<String>, model: Option<Model>, timeout: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ObserveResponse, StagehandError>> + Send>>, StagehandError>;
    async fn execute(&mut self, session_id: &str, agent_config: AgentConfig, execute_options: AgentExecuteOptions, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ExecuteResponse, StagehandError>> + Send>>, StagehandError>;
    async fn close(&mut self, session_id: &str) -> Result<(), StagehandError>;
}

// =============================================================================
// REST Transport Implementation
// =============================================================================

pub struct RestTransport {
    base_url: String,
    api_key: String,
    project_id: String,
    model_api_key: String,
    client: Arc<Client>,
}

impl RestTransport {
    pub fn new(base_url: String) -> Result<Self, StagehandError> {
        let model_api_key = std::env::var("OPENAI_API_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .map_err(|_| StagehandError::MissingApiKey("OPENAI_API_KEY or ANTHROPIC_API_KEY".to_string()))?;

        Ok(Self {
            base_url,
            api_key: std::env::var("BROWSERBASE_API_KEY").map_err(|_| StagehandError::MissingApiKey("BROWSERBASE_API_KEY".to_string()))?,
            project_id: std::env::var("BROWSERBASE_PROJECT_ID").map_err(|_| StagehandError::MissingApiKey("BROWSERBASE_PROJECT_ID".to_string()))?,
            model_api_key,
            client: Arc::new(Client::new()),
        })
    }

    async fn execute_stream(&self, _session_id: &str, path: &str, body: serde_json::Value) -> Result<Pin<Box<dyn Stream<Item = Result<serde_json::Value, StagehandError>> + Send>>, StagehandError> {
        let url = format!("{}{}", self.base_url, path);

        let client_builder = ClientBuilder::for_url(&url)?
            .header("x-bb-api-key", &self.api_key)?
            .header("x-bb-project-id", &self.project_id)?
            .header("x-model-api-key", &self.model_api_key)?
            .header("x-stream-response", "true")?
            .header("x-language", "typescript")?
            .header("x-sdk-version", "3.0.0")?
            .header("Content-Type", "application/json")?
            .method(reqwest::Method::POST.to_string())
            .body(body.to_string());

        let sse_client = client_builder.build();
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            let mut stream = sse_client.stream();
            while let Some(event) = stream.next().await {
                match event {
                    Ok(sse_event) => {
                        match sse_event {
                            SSE::Event(e) => {
                                if let Ok(event_data) = serde_json::from_str::<serde_json::Value>(&e.data) {
                                    if tx.send(Ok(event_data)).await.is_err() {
                                        break;
                                    }
                                } else {
                                    let _ = tx.send(Err(StagehandError::Api(format!("Failed to parse SSE event: {}", e.data)))).await;
                                }
                            },
                            _ => {},
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(Err(StagehandError::Transport(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn parse_log_event(json_value: &serde_json::Value) -> Option<LogLine> {
        let data = &json_value["data"];
        Some(LogLine {
            message: data["message"].as_str().unwrap_or("").to_string(),
            status: data["status"].as_str().map(|s| s.to_string()),
        })
    }
}

#[async_trait]
impl Transport for RestTransport {
    async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<InitResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InitPayload<'a> {
            model_name: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            dom_settle_timeout_ms: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            verbose: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            system_prompt: Option<&'a String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            self_heal: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            wait_for_captcha_solves: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            browserbase_session_create_params: Option<&'a serde_json::Value>,
            #[serde(rename = "browserbaseSessionID")]
            #[serde(skip_serializing_if = "Option::is_none")]
            browserbase_session_id: Option<&'a String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            experimental: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            act_timeout_ms: Option<u32>,
        }

        let model_name = opts.model.as_ref().map(|m| match m {
            Model::String(s) => s.clone(),
            Model::Config { model_name, .. } => model_name.clone(),
        }).unwrap_or_else(|| "openai/gpt-5-nano".to_string());

        let payload = InitPayload {
            model_name,
            dom_settle_timeout_ms: opts.dom_settle_timeout_ms,
            verbose: opts.verbose.map(|v| v.to_string()),
            system_prompt: opts.system_prompt.as_ref(),
            self_heal: opts.self_heal,
            wait_for_captcha_solves: opts.wait_for_captcha_solves,
            browserbase_session_create_params: opts.browserbase_session_create_params.as_ref(),
            browserbase_session_id: opts.browserbase_session_id.as_ref(),
            experimental: opts.experimental,
            act_timeout_ms: opts.act_timeout_ms,
        };

        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;

        // Init uses regular HTTP POST, not SSE streaming
        let url = format!("{}/sessions/start", self.base_url);
        let response = self.client
            .post(&url)
            .header("x-bb-api-key", &self.api_key)
            .header("x-bb-project-id", &self.project_id)
            .header("x-model-api-key", &self.model_api_key)
            .header("x-language", "typescript")
            .header("x-sdk-version", "3.0.0")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let json_value: serde_json::Value = response.json().await?;

        // Check for error response
        if !json_value["success"].as_bool().unwrap_or(false) {
            return Err(StagehandError::Api(json_value["error"].as_str().unwrap_or("Unknown error").to_string()));
        }

        // Check if available
        if !json_value["data"]["available"].as_bool().unwrap_or(false) {
            return Err(StagehandError::Api("Stagehand API not available for this account".to_string()));
        }

        let session_id = json_value["data"]["sessionId"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Return a single-item stream with the result
        let result = InitResponse {
            event: Some(InitResponseEvent::Result(InitResult { session_id }))
        };

        Ok(Box::pin(futures::stream::once(async move { Ok(result) })))
    }

    async fn act(&mut self, session_id: &str, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ActResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ActPayload {
            input: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            options: Option<ActOptions>,
            #[serde(skip_serializing_if = "Option::is_none")]
            frame_id: Option<String>,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ActOptions {
            #[serde(skip_serializing_if = "Option::is_none")]
            model: Option<ModelObj>,
            #[serde(skip_serializing_if = "Option::is_none")]
            variables: Option<HashMap<String, String>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            timeout: Option<u32>,
        }

        let model_obj = model.map(|m| match m {
            Model::String(s) => ModelObj { model_name: s, api_key: None, base_url: None },
            Model::Config { model_name, api_key, base_url } => ModelObj { model_name, api_key, base_url },
        });

        let options = if model_obj.is_some() || !variables.is_empty() || timeout.is_some() {
            Some(ActOptions {
                model: model_obj,
                variables: if variables.is_empty() { None } else { Some(variables) },
                timeout,
            })
        } else {
            None
        };

        let payload = ActPayload {
            input: instruction,
            options,
            frame_id,
        };

        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/act", session_id), body).await?;

        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| {
                if let Some(event_type) = json_value["type"].as_str() {
                    match event_type {
                        "system" => {
                            if let Some(status) = json_value["data"]["status"].as_str() {
                                match status {
                                    "finished" => {
                                        let success = json_value["data"]["result"]["success"].as_bool().unwrap_or(true);
                                        Ok(ActResponse { event: Some(ActResponseEvent::Success(success)) })
                                    },
                                    "error" => {
                                        Err(StagehandError::Api(json_value["data"]["error"].as_str().unwrap_or("Unknown error").to_string()))
                                    },
                                    _ => Ok(ActResponse { event: None })
                                }
                            } else {
                                Ok(ActResponse { event: None })
                            }
                        },
                        "log" => {
                            if let Some(log) = RestTransport::parse_log_event(&json_value) {
                                Ok(ActResponse { event: Some(ActResponseEvent::Log(log)) })
                            } else {
                                Ok(ActResponse { event: None })
                            }
                        },
                        _ => Ok(ActResponse { event: None })
                    }
                } else {
                    let success = json_value["success"].as_bool().unwrap_or(true);
                    Ok(ActResponse { event: Some(ActResponseEvent::Success(success)) })
                }
            })
        })))
    }

    async fn extract(&mut self, session_id: &str, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExtractPayload {
            #[serde(skip_serializing_if = "Option::is_none")]
            instruction: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            schema: Option<serde_json::Value>,
            #[serde(skip_serializing_if = "Option::is_none")]
            options: Option<ExtractOptions>,
            #[serde(skip_serializing_if = "Option::is_none")]
            frame_id: Option<String>,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExtractOptions {
            #[serde(skip_serializing_if = "Option::is_none")]
            model: Option<ModelObj>,
            #[serde(skip_serializing_if = "Option::is_none")]
            timeout: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            selector: Option<String>,
        }

        let model_obj = model.map(|m| match m {
            Model::String(s) => ModelObj { model_name: s, api_key: None, base_url: None },
            Model::Config { model_name, api_key, base_url } => ModelObj { model_name, api_key, base_url },
        });

        let options = if model_obj.is_some() || timeout.is_some() || selector.is_some() {
            Some(ExtractOptions {
                model: model_obj,
                timeout,
                selector,
            })
        } else {
            None
        };

        let payload = ExtractPayload {
            instruction: if instruction.is_empty() { None } else { Some(instruction) },
            schema: if schema.is_null() { None } else { Some(schema) },
            options,
            frame_id,
        };

        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/extract", session_id), body).await?;

        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| {
                if let Some(event_type) = json_value["type"].as_str() {
                    match event_type {
                        "system" => {
                            if let Some(status) = json_value["data"]["status"].as_str() {
                                match status {
                                    "finished" => {
                                        let result = &json_value["data"]["result"];
                                        Ok(ExtractResponse { event: Some(ExtractResponseEvent::DataJson(result.to_string())) })
                                    },
                                    "error" => {
                                        Err(StagehandError::Api(json_value["data"]["error"].as_str().unwrap_or("Unknown error").to_string()))
                                    },
                                    _ => Ok(ExtractResponse { event: None })
                                }
                            } else {
                                Ok(ExtractResponse { event: None })
                            }
                        },
                        "log" => {
                            if let Some(log) = RestTransport::parse_log_event(&json_value) {
                                Ok(ExtractResponse { event: Some(ExtractResponseEvent::Log(log)) })
                            } else {
                                Ok(ExtractResponse { event: None })
                            }
                        },
                        _ => Ok(ExtractResponse { event: None })
                    }
                } else {
                    Ok(ExtractResponse { event: Some(ExtractResponseEvent::DataJson(json_value.to_string())) })
                }
            })
        })))
    }

    async fn observe(&mut self, session_id: &str, instruction: Option<String>, model: Option<Model>, timeout: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ObservePayload {
            #[serde(skip_serializing_if = "Option::is_none")]
            instruction: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            options: Option<ObserveOptions>,
            #[serde(skip_serializing_if = "Option::is_none")]
            frame_id: Option<String>,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ObserveOptions {
            #[serde(skip_serializing_if = "Option::is_none")]
            model: Option<ModelObj>,
            #[serde(skip_serializing_if = "Option::is_none")]
            timeout: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            selector: Option<String>,
        }

        let model_obj = model.map(|m| match m {
            Model::String(s) => ModelObj { model_name: s, api_key: None, base_url: None },
            Model::Config { model_name, api_key, base_url } => ModelObj { model_name, api_key, base_url },
        });

        let options = if model_obj.is_some() || timeout.is_some() || selector.is_some() {
            Some(ObserveOptions {
                model: model_obj,
                timeout,
                selector,
            })
        } else {
            None
        };

        let payload = ObservePayload {
            instruction,
            options,
            frame_id,
        };

        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/observe", session_id), body).await?;

        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| {
                if let Some(event_type) = json_value["type"].as_str() {
                    match event_type {
                        "system" => {
                            if let Some(status) = json_value["data"]["status"].as_str() {
                                match status {
                                    "finished" => {
                                        let result = &json_value["data"]["result"];
                                        Ok(ObserveResponse { event: Some(ObserveResponseEvent::ElementsJson(result.to_string())) })
                                    },
                                    "error" => {
                                        Err(StagehandError::Api(json_value["data"]["error"].as_str().unwrap_or("Unknown error").to_string()))
                                    },
                                    _ => Ok(ObserveResponse { event: None })
                                }
                            } else {
                                Ok(ObserveResponse { event: None })
                            }
                        },
                        "log" => {
                            if let Some(log) = RestTransport::parse_log_event(&json_value) {
                                Ok(ObserveResponse { event: Some(ObserveResponseEvent::Log(log)) })
                            } else {
                                Ok(ObserveResponse { event: None })
                            }
                        },
                        _ => Ok(ObserveResponse { event: None })
                    }
                } else {
                    Ok(ObserveResponse { event: Some(ObserveResponseEvent::ElementsJson(json_value.to_string())) })
                }
            })
        })))
    }

    async fn execute(&mut self, session_id: &str, agent_config: AgentConfig, execute_options: AgentExecuteOptions, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExecutePayload {
            agent_config: AgentConfig,
            execute_options: AgentExecuteOptions,
            #[serde(skip_serializing_if = "Option::is_none")]
            frame_id: Option<String>,
        }

        let payload = ExecutePayload {
            agent_config,
            execute_options,
            frame_id,
        };

        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/agentExecute", session_id), body).await?;

        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| {
                if let Some(event_type) = json_value["type"].as_str() {
                    match event_type {
                        "system" => {
                            if let Some(status) = json_value["data"]["status"].as_str() {
                                match status {
                                    "finished" => {
                                        let result = &json_value["data"]["result"];
                                        Ok(ExecuteResponse { event: Some(ExecuteResponseEvent::ResultJson(result.to_string())) })
                                    },
                                    "error" => {
                                        Err(StagehandError::Api(json_value["data"]["error"].as_str().unwrap_or("Unknown error").to_string()))
                                    },
                                    _ => Ok(ExecuteResponse { event: None })
                                }
                            } else {
                                Ok(ExecuteResponse { event: None })
                            }
                        },
                        "log" => {
                            if let Some(log) = RestTransport::parse_log_event(&json_value) {
                                Ok(ExecuteResponse { event: Some(ExecuteResponseEvent::Log(log)) })
                            } else {
                                Ok(ExecuteResponse { event: None })
                            }
                        },
                        _ => Ok(ExecuteResponse { event: None })
                    }
                } else {
                    Ok(ExecuteResponse { event: Some(ExecuteResponseEvent::ResultJson(json_value.to_string())) })
                }
            })
        })))
    }

    async fn close(&mut self, session_id: &str) -> Result<(), StagehandError> {
        let url = format!("{}/sessions/{}/end", self.base_url, session_id);
        self.client.post(&url)
            .header("x-bb-api-key", &self.api_key)
            .header("x-bb-project-id", &self.project_id)
            .header("x-model-api-key", &self.model_api_key)
            .header("x-stream-response", "false")
            .send()
            .await?;
        Ok(())
    }
}

// =============================================================================
// The Stagehand Client
// =============================================================================

pub struct Stagehand {
    transport: Box<dyn Transport + Send + Sync>,
    session_id: Option<String>,
}

impl Stagehand {
    pub async fn connect(transport_choice: TransportChoice) -> Result<Self, StagehandError> {
        let transport: Box<dyn Transport + Send + Sync> = match transport_choice {
            TransportChoice::Rest(base_url) => Box::new(RestTransport::new(base_url)?),
        };
        Ok(Self { transport, session_id: None })
    }

    pub async fn init(&mut self, opts: V3Options) -> Result<(), StagehandError> {
        let mut stream = self.transport.init(opts).await?;
        while let Some(item) = stream.next().await {
            match item {
                Ok(response) => {
                    if let Some(InitResponseEvent::Result(res)) = response.event {
                        if !res.session_id.is_empty() {
                            self.session_id = Some(res.session_id);
                            return Ok(());
                        }
                    }
                },
                Err(e) => return Err(e),
            }
        }
        Err(StagehandError::Api("Init stream ended without a session ID.".to_string()))
    }

    pub async fn act(&mut self, instruction: impl Into<String>, model: Option<Model>, variables: HashMap<String, String>, timeout: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ActResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session not initialized".to_string()))?.clone();
        self.transport.act(&session_id, instruction.into(), model, variables, timeout, frame_id).await
    }

    pub async fn extract<S: Serialize>(&mut self, instruction: impl Into<String>, schema: &S, model: Option<Model>, timeout: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session not initialized".to_string()))?.clone();
        let schema_value = serde_json::to_value(schema).map_err(|e| StagehandError::Api(e.to_string()))?;
        self.transport.extract(&session_id, instruction.into(), schema_value, model, timeout, selector, frame_id).await
    }

    pub async fn observe(&mut self, instruction: Option<String>, model: Option<Model>, timeout: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session not initialized".to_string()))?.clone();
        self.transport.observe(&session_id, instruction, model, timeout, selector, frame_id).await
    }

    pub async fn execute(&mut self, agent_config: AgentConfig, execute_options: AgentExecuteOptions, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session not initialized".to_string()))?.clone();
        self.transport.execute(&session_id, agent_config, execute_options, frame_id).await
    }

    pub async fn close(&mut self) -> Result<(), StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session not initialized".to_string()))?.clone();
        self.transport.close(&session_id).await
    }

    /// Returns the Browserbase session ID if initialized
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Returns the Browserbase CDP WebSocket URL for connecting external tools like chromiumoxide.
    ///
    /// The URL format is: `wss://connect.browserbase.com?sessionId={sessionId}&apiKey={apiKey}`
    ///
    /// This allows you to connect directly to the browser session using CDP.
    pub fn browserbase_cdp_url(&self) -> Option<String> {
        let session_id = self.session_id.as_ref()?;
        let api_key = std::env::var("BROWSERBASE_API_KEY").ok()?;
        Some(format!(
            "wss://connect.browserbase.com?sessionId={}&apiKey={}",
            session_id, api_key
        ))
    }
}
