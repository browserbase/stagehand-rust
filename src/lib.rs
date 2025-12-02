pub mod proto {
    tonic::include_proto!("stagehand.v1");
}

use proto::stagehand_service_client::StagehandServiceClient;
use tonic::transport::Channel;
use futures::Stream;
use serde::Serialize;
use std::collections::HashMap;
use tokio_stream::StreamExt;

// --- Rest API specific imports ---
use reqwest::Client;
use async_stream::stream;
use serde::Deserialize;
use std::fmt;
use std::sync::Arc;
use std::pin::Pin;
use std::time::Duration;
use eventsource_client::{Client as SseClient, ClientBuilder, SSE};

// --- Request Payload Structs for REST API ---
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitRequestPayload {
    model_name: Option<String>,
    dom_settle_timeout_ms: Option<i32>,
    verbose: Option<i32>,
    system_prompt: Option<String>,
    self_heal: Option<bool>,
    browserbase_session_create_params: Option<serde_json::Value>,
    browserbase_session_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActRequestPayload<'a> {
    input: &'a str,
    frame_id: Option<String>,
    options: ActOptionsPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActOptionsPayload {
    model: Option<proto::ModelConfiguration>,
    variables: HashMap<String, String>,
    timeout_ms: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractRequestPayload<'a> {
    instruction: &'a str,
    schema: serde_json::Value,
    frame_id: Option<String>,
    options: ExtractOptionsPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractOptionsPayload {
    model: Option<proto::ModelConfiguration>,
    timeout_ms: Option<u32>,
    selector: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObserveRequestPayload {
    instruction: Option<String>,
    frame_id: Option<String>,
    options: ObserveOptionsPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObserveOptionsPayload {
    model: Option<proto::ModelConfiguration>,
    timeout_ms: Option<u32>,
    selector: Option<String>,
    only_selectors: Vec<String>,
}

// --- Agent specific structs (matching TypeScript types) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub cua: Option<bool>,
    pub stream: Option<bool>,
    pub model: Option<Model>,
    pub system_prompt: Option<String>,
    // Assuming Tool is a simple string identifier for now, can be expanded if needed
    pub tools: Option<HashMap<String, serde_json::Value>>, 
    // Assuming AgentIntegration is a simple string identifier for now
    pub integrations: Option<Vec<String>>,
    pub execution_model: Option<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecuteOptions {
    pub instruction: String,
    pub page: Option<String>, // Represents frame_id in our context
    pub timeout: Option<u32>,
    // Other fields from AgentExecuteOptions in TS can go here if needed
}

// --- Idiomatic Types ---

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

#[derive(Debug, Clone, Serialize, Deserialize)] // Added Serialize, Deserialize
pub enum Model {
    String(String),
    Config {
        model_name: String,
        api_key: Option<String>,
        base_url: Option<String>,
    },
}

impl From<Model> for proto::ModelConfiguration {
    fn from(m: Model) -> Self {
        match m {
            Model::String(s) => proto::ModelConfiguration {
                config: Some(proto::model_configuration::Config::ModelString(s)),
            },
            Model::Config { model_name, api_key, base_url } => proto::ModelConfiguration {
                config: Some(proto::model_configuration::Config::ModelObj(proto::ModelObj {
                    model_name,
                    api_key,
                    base_url,
                })),
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalBrowserLaunchOptions {
    pub headless: Option<bool>,
    pub executable_path: Option<String>,
    pub args: Vec<String>,
    pub user_data_dir: Option<String>,
    pub viewport: Option<(i32, i32)>, // width, height
    pub devtools: Option<bool>,
    pub ignore_https_errors: Option<bool>,
    pub cdp_url: Option<String>,
    // Proxy omitted for brevity, but follows same pattern
}

impl From<LocalBrowserLaunchOptions> for proto::LocalBrowserLaunchOptions {
    fn from(o: LocalBrowserLaunchOptions) -> Self {
        proto::LocalBrowserLaunchOptions {
            headless: o.headless,
            executable_path: o.executable_path,
            args: o.args,
            user_data_dir: o.user_data_dir,
            viewport: o.viewport.map(|(w, h)| proto::Viewport { width: w, height: h }),
            devtools: o.devtools,
            ignore_https_errors: o.ignore_https_errors,
            cdp_url: o.cdp_url,
            proxy: None, 
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct V3Options {
    pub env: Option<Env>,
    // Browserbase
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub browserbase_session_id: Option<String>,
    pub browserbase_session_create_params: Option<serde_json::Value>,
    // Local
    pub local_browser_launch_options: Option<LocalBrowserLaunchOptions>,
    // AI
    pub model: Option<Model>,
    pub system_prompt: Option<String>,
    // Behavior
    pub self_heal: Option<bool>,
    pub experimental: Option<bool>,
    pub dom_settle_timeout: Option<u32>,
    pub cache_dir: Option<String>,
    // Logging
    pub verbose: Option<i32>,
    pub log_inference_to_file: Option<bool>,
    pub disable_pino: Option<bool>,
    // Transport
    pub transport: Option<Transport>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    Grpc(String), // gRPC endpoint URL
    Rest(String), // REST API base URL
}

impl Default for Transport {
    fn default() -> Self {
        Transport::Grpc("http://127.0.0.1:50051".to_string())
    }
}

// --- Stagehand API Error Types ---
#[derive(Debug)]
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

impl fmt::Display for StagehandAPIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StagehandAPIError::Http(e) => write!(f, "HTTP error: {}", e),
            StagehandAPIError::Api(msg) => write!(f, "API error: {}", msg),
            StagehandAPIError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            StagehandAPIError::ConnectionRefused(msg) => write!(f, "Connection refused: {}", msg),
            StagehandAPIError::ResponseParse(msg) => write!(f, "Response parse error: {}", msg),
            StagehandAPIError::ResponseBody(msg) => write!(f, "Response body error: {}", msg),
            StagehandAPIError::ServerError(msg) => write!(f, "Server error: {}", msg),
            StagehandAPIError::MissingSessionId => write!(f, "Missing session ID"),
            StagehandAPIError::MissingApiKey(key_name) => write!(f, "Missing API key: {}", key_name),
            StagehandAPIError::TonicStatus(s) => write!(f, "Tonic Status error: {}", s),
            StagehandAPIError::EventSource(e) => write!(f, "EventSource error: {}", e),
            StagehandAPIError::Timeout => write!(f, "Stream timed out"),
        }
    }
}

impl std::error::Error for StagehandAPIError {}

impl From<reqwest::Error> for StagehandAPIError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() {
            StagehandAPIError::ConnectionRefused(err.to_string())
        } else {
            StagehandAPIError::Http(err)
        }
    }
}

impl From<eventsource_client::Error> for StagehandAPIError {
    fn from(err: eventsource_client::Error) -> Self {
        StagehandAPIError::EventSource(err)
    }
}

impl From<StagehandAPIError> for tonic::Status {
    fn from(err: StagehandAPIError) -> Self {
        tonic::Status::internal(err.to_string())
    }
}

// --- LogLine for REST API (matches proto::LogLine) ---
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogLine {
    pub category: String,
    pub message: String,
    pub auxiliary: Option<String>,
}

// --- Stagehand Rest API Client ---
#[derive(Clone)]
pub struct StagehandRestApiClient {
    base_url: String,
    api_key: String,
    project_id: String,
    session_id: Option<String>,
    model_api_key: Option<String>,
    client: Arc<Client>,
}

impl StagehandRestApiClient {
    pub fn new(base_url: String, api_key: String, project_id: String) -> Self {
        Self {
            base_url,
            api_key,
            project_id,
            session_id: None,
            model_api_key: None,
            client: Arc::new(Client::new()),
        }
    }

    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    async fn execute_stream<P: prost::Message + Default + Send + 'static + for<'de> Deserialize<'de>>(
        &mut self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<P, tonic::Status>> + Send>>, StagehandAPIError> {
        let url = format!("{}{}", self.base_url, path);
        let mut client_builder = ClientBuilder::for_url(&url)?
            .header("x-bb-api-key", &self.api_key)?
            .header("x-bb-project-id", &self.project_id)?
            .header("x-language", "rust")?;

        if let Some(session_id) = &self.session_id {
            client_builder = client_builder.header("x-bb-session-id", session_id)?;
        }
        if let Some(model_api_key) = &self.model_api_key {
            client_builder = client_builder.header("x-model-api-key", model_api_key)?;
        }

        client_builder = client_builder.header("x-stream-response", "true")?;
        client_builder = client_builder.header("Content-Type", "application/json")?;
        
        let sse_client = client_builder
            .method(reqwest::Method::POST.to_string())
            .body(body.to_string())
            .build();
        
        let mut stream = sse_client.stream();

        let s = stream! {
            loop {
                match tokio::time::timeout(Duration::from_secs(60), stream.next()).await {
                    Ok(Some(event)) => {
                        match event {
                            Ok(sse_event) => {
                                match sse_event {
                                    SSE::Event(event) => {
                                        match serde_json::from_str::<serde_json::Value>(&event.data) {
                                            Ok(event_data) => {
                                                if let Some(event_type) = event_data["type"].as_str() {
                                                    match event_type {
                                                        "system" => {
                                                            if let Some(status) = event_data["data"]["status"].as_str() {
                                                                match status {
                                                                    "error" => {
                                                                        let error_msg = event_data["data"]["error"].as_str().unwrap_or("Unknown server error").to_string();
                                                                        yield Err(tonic::Status::internal(error_msg));
                                                                        break; // Terminate loop
                                                                    },
                                                                    "finished" => {
                                                                        let result = serde_json::from_value::<P>(event_data["data"]["result"].clone());
                                                                        match result {
                                                                            Ok(res) => yield Ok(res),
                                                                            Err(e) => yield Err(tonic::Status::internal(format!("Failed to parse final result: {}", e))),
                                                                        }
                                                                        break; // Terminate loop
                                                                    },
                                                                    _ => {}
                                                                }
                                                            }
                                                        },
                                                        "log" => {
                                                            if let Ok(log_line) = serde_json::from_value::<LogLine>(event_data["data"].clone()) {
                                                                yield Err(tonic::Status::unimplemented(format!("Log message from REST: [{}]{}", log_line.category, log_line.message)));
                                                            }
                                                        },
                                                        _ => {}
                                                    }
                                                }
                                            },
                                            Err(e) => yield Err(tonic::Status::internal(format!("Failed to parse SSE event data: {}", e))),
                                        }
                                    },
                                    SSE::Comment(_) => {},
                                    SSE::Connected(_) => {},
                                }
                            },
                            Err(e) => {
                                yield Err(tonic::Status::internal(format!("EventSource error: {}", e)));
                                break;
                            }
                        }
                    },
                    Ok(None) => break, // Stream finished
                    Err(_) => { // Timeout
                        yield Err(tonic::Status::deadline_exceeded("Stream timed out after 60 seconds of inactivity."));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }
}

// --- The Stagehand Client ---

#[derive(Clone)]
pub enum StagehandClientType {
    Grpc(StagehandServiceClient<Channel>),
    Rest(StagehandRestApiClient),
}

#[derive(Clone)]
pub struct Stagehand {
    client: StagehandClientType,
}

impl Stagehand {
    pub async fn connect(_dst: String, transport_type: Transport) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = match transport_type {
            Transport::Grpc(url) => StagehandClientType::Grpc(StagehandServiceClient::connect(url).await?),
            Transport::Rest(base_url) => {
                let api_key = std::env::var("BROWSERBASE_API_KEY").map_err(|_| StagehandAPIError::MissingApiKey("BROWSERBASE_API_KEY".to_string()))?;
                let project_id = std::env::var("BROWSERBASE_PROJECT_ID").map_err(|_| StagehandAPIError::MissingApiKey("BROWSERBASE_PROJECT_ID".to_string()))?;
                StagehandClientType::Rest(StagehandRestApiClient::new(base_url, api_key, project_id))
            }
        };
        Ok(Self { client })
    }

    /// Stagehand.init()
    /// Returns a stream of log events. The stream ends when initialization is complete.
    pub async fn init(
        &mut self,
        opts: V3Options,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, tonic::Status>> + Send>>, tonic::Status> {
        match &mut self.client {
            StagehandClientType::Grpc(client) => {
                let req = proto::InitRequest {
                    env: opts.env.unwrap_or(Env::Local).to_string(),
                    api_key: opts.api_key,
                    project_id: opts.project_id,
                    browserbase_session_id: opts.browserbase_session_id,
                    browserbase_session_create_params_json: opts.browserbase_session_create_params.map(|v| v.to_string()),
                    local_browser_launch_options: opts.local_browser_launch_options.map(|o| o.into()),
                    model: opts.model.map(|m| m.into()),
                    system_prompt: opts.system_prompt,
                    self_heal: opts.self_heal,
                    experimental: opts.experimental,
                    dom_settle_timeout_ms: opts.dom_settle_timeout.map(|t| t as i32),
                    cache_dir: opts.cache_dir,
                    verbose: opts.verbose,
                    log_inference_to_file: opts.log_inference_to_file,
                    disable_pino: opts.disable_pino,
                };

                let response = client.init(req).await?;
                Ok(Box::pin(response.into_inner()))
            },
            StagehandClientType::Rest(client) => {
                let mut rest_client = client.clone();

                if let Some(model) = opts.model.as_ref() {
                    if let Model::Config { api_key, .. } = model {
                        rest_client.model_api_key = api_key.clone();
                    } else if let Model::String(s) = model {
                        if s.starts_with("openai/") {
                            rest_client.model_api_key = std::env::var("OPENAI_API_KEY").ok();
                        }
                    }
                }

                let path = "/sessions/start";
                let payload = InitRequestPayload {
                    model_name: opts.model.as_ref().map(|m| match m {
                        Model::String(s) => s.clone(),
                        Model::Config { model_name, .. } => model_name.clone(),
                    }),
                    dom_settle_timeout_ms: opts.dom_settle_timeout.map(|t| t as i32),
                    verbose: opts.verbose,
                    system_prompt: opts.system_prompt,
                    self_heal: opts.self_heal,
                    browserbase_session_create_params: opts.browserbase_session_create_params,
                    browserbase_session_id: opts.browserbase_session_id,
                };
                
                let body = serde_json::to_value(payload).unwrap();

                let sse_stream = rest_client.execute_stream::<proto::InitResponse>(path, body).await?;
                
                let collected_events: Vec<Result<proto::InitResponse, tonic::Status>> = sse_stream.collect().await;

                let mut session_id_from_stream: Option<String> = None;
                for event_result in &collected_events {
                    if let Ok(event) = event_result {
                        if let Some(proto::init_response::Event::Result(res)) = &event.event {
                            if !res.unused.is_empty() {
                                session_id_from_stream = Some(res.unused.clone());
                                break;
                            }
                        }
                    }
                }

                if let Some(session_id_val) = session_id_from_stream {
                    client.set_session_id(session_id_val);
                }
                
                Ok(Box::pin(tokio_stream::iter(collected_events)))
            }
        }
    }

    /// Stagehand.act(instruction)
    pub async fn act(
        &mut self,
        instruction: impl Into<String>,
        model: Option<Model>,
        variables: HashMap<String, String>,
        timeout_ms: Option<u32>,
        frame_id: Option<String>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, tonic::Status>> + Send>>, tonic::Status> {
        match &mut self.client {
            StagehandClientType::Grpc(client) => {
                let req = proto::ActRequest {
                    instruction: instruction.into(),
                    model: model.map(|m| m.into()),
                    variables,
                    timeout_ms: timeout_ms.map(|t| t as i32),
                    frame_id: frame_id.clone(),
                };

                let response = client.act(req).await?;
                Ok(Box::pin(response.into_inner()))
            },
            StagehandClientType::Rest(client) => {
                let mut rest_client = client.clone();
                let session_id = rest_client.session_id.as_ref().ok_or_else(|| tonic::Status::internal("Session ID is missing for REST API Act call"))?.clone();
                let path = format!("/sessions/{}/act", session_id);

                let instruction_str = instruction.into();
                let payload = ActRequestPayload {
                    input: &instruction_str,
                    frame_id,
                    options: ActOptionsPayload {
                        model: model.map(|m| m.into()),
                        variables,
                        timeout_ms,
                    },
                };

                let body = serde_json::to_value(payload).unwrap();
                let sse_stream = rest_client.execute_stream::<proto::ActResponse>(&path, body).await?;
                Ok(Box::pin(stream! {
                    for await item in sse_stream {
                        yield item;
                    }
                }))
            }
        }
    }

    /// Stagehand.extract(instruction, schema)
    pub async fn extract<S: Serialize>(
        &mut self,
        instruction: impl Into<String>,
        schema: &S,
        model: Option<Model>,
        timeout_ms: Option<u32>,
        selector: Option<String>,
        frame_id: Option<String>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, tonic::Status>> + Send>>, tonic::Status> {
        match &mut self.client {
            StagehandClientType::Grpc(client) => {
                let schema_json = serde_json::to_string(schema).unwrap_or_default();

                let req = proto::ExtractRequest {
                    instruction: instruction.into(),
                    schema_json,
                    model: model.map(|m| m.into()),
                    timeout_ms: timeout_ms.map(|t| t as i32),
                    selector,
                    frame_id,
                };

                let response = client.extract(req).await?;
                Ok(Box::pin(response.into_inner()))
            },
            StagehandClientType::Rest(client) => {
                let mut rest_client = client.clone();
                let session_id = rest_client.session_id.as_ref().ok_or_else(|| tonic::Status::internal("Session ID is missing for REST API Extract call"))?.clone();
                let path = format!("/sessions/{}/extract", session_id);
                
                let instruction_str = instruction.into();
                let payload = ExtractRequestPayload {
                    instruction: &instruction_str,
                    schema: serde_json::to_value(schema).unwrap(),
                    frame_id,
                    options: ExtractOptionsPayload {
                        model: model.map(|m| m.into()),
                        timeout_ms,
                        selector,
                    },
                };

                let body = serde_json::to_value(payload).unwrap();
                let sse_stream = rest_client.execute_stream::<proto::ExtractResponse>(&path, body).await?;
                Ok(Box::pin(stream! {
                    for await item in sse_stream {
                        yield item;
                    }
                }))
            }
        }
    }

    /// Stagehand.observe(instruction?)
    pub async fn observe(
        &mut self,
        instruction: Option<String>,
        model: Option<Model>,
        timeout_ms: Option<u32>,
        selector: Option<String>,
        only_selectors: Vec<String>,
        frame_id: Option<String>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, tonic::Status>> + Send>>, tonic::Status> {
        match &mut self.client {
            StagehandClientType::Grpc(client) => {
                let req = proto::ObserveRequest {
                    instruction,
                    model: model.map(|m| m.into()),
                    timeout_ms: timeout_ms.map(|t| t as i32),
                    selector,
                    only_selectors,
                    frame_id,
                };

                let response = client.observe(req).await?;
                Ok(Box::pin(response.into_inner()))
            },
            StagehandClientType::Rest(client) => {
                let mut rest_client = client.clone();
                let session_id = rest_client.session_id.as_ref().ok_or_else(|| tonic::Status::internal("Session ID is missing for REST API Observe call"))?.clone();
                let path = format!("/sessions/{}/observe", session_id);

                let payload = ObserveRequestPayload {
                    instruction,
                    frame_id,
                    options: ObserveOptionsPayload {
                        model: model.map(|m| m.into()),
                        timeout_ms,
                        selector,
                        only_selectors,
                    },
                };
                let body = serde_json::to_value(payload).unwrap();

                let sse_stream = rest_client.execute_stream::<proto::ObserveResponse>(&path, body).await?;
                Ok(Box::pin(stream! {
                    for await item in sse_stream {
                        yield item;
                    }
                }))
            }
        }
    }

    /// Stagehand.execute(...)
    /// Maps to TypeScript's `stagehand.agent().execute(instruction, options)` (or `agentExecute` in `api.ts`)
    pub async fn execute(
        &mut self,
        session_id: String,
        instruction: String,
        frame_id: Option<String>,
        agent_config: Option<AgentConfig>,
        execute_options: Option<AgentExecuteOptions>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, tonic::Status>> + Send>>, tonic::Status> {
        match &mut self.client {
            StagehandClientType::Grpc(client) => {
                let agent_config_json = agent_config.map(|c| serde_json::to_string(&c).unwrap_or_default());
                let execute_options_json = execute_options.map(|o| serde_json::to_string(&o).unwrap_or_default());

                let req = proto::ExecuteRequest {
                    session_id,
                    instruction,
                    frame_id,
                    agent_config_json,
                    execute_options_json,
                };
                let response = client.execute(req).await?;
                Ok(Box::pin(response.into_inner()))
            },
            StagehandClientType::Rest(client) => {
                let mut rest_client = client.clone();
                let path = format!("/sessions/{}/agentExecute", session_id);

                let body = serde_json::json!({
                    "agentConfig": agent_config,
                    "executeOptions": execute_options,
                    "frameId": frame_id,
                });

                let sse_stream = rest_client.execute_stream::<proto::ExecuteResponse>(&path, body).await?;
                Ok(Box::pin(stream! {
                    for await item in sse_stream {
                        yield item;
                    }
                }))
            }
        }
    }

    /// Stagehand.close()
    pub async fn close(&mut self, force: bool) -> Result<(), tonic::Status> {
        match &mut self.client {
            StagehandClientType::Grpc(client) => {
                let req = proto::CloseRequest { force };
                client.close(req).await?;
                Ok(())
            },
            StagehandClientType::Rest(client) => {
                if let Some(session_id) = client.session_id.as_ref() {
                    let path = format!("/sessions/{}/end", session_id);
                    let _ = client.client.post(&format!("{}{}", client.base_url, path)).send().await; // Fire and forget
                }
                client.session_id = None; // Clear session ID on close
                Ok(())
            }
        }
    }
}