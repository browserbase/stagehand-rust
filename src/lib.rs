pub mod proto {
    tonic::include_proto!("stagehand.v1");
}

use proto::stagehand_service_client::StagehandServiceClient;
use tonic::transport::Channel;
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

// --- Agent specific structs (matching TypeScript types) ---
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub cua: Option<bool>,
    pub stream: Option<bool>,
    pub model: Option<Model>,
    pub system_prompt: Option<String>,
    pub tools: Option<HashMap<String, serde_json::Value>>, 
    pub integrations: Option<Vec<String>>,
    pub execution_model: Option<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecuteOptions {
    pub instruction: String,
    pub page: Option<String>, // Represents frame_id in our context
    pub timeout: Option<u32>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub browserbase_session_id: Option<String>,
    pub browserbase_session_create_params: Option<serde_json::Value>,
    pub local_browser_launch_options: Option<LocalBrowserLaunchOptions>,
    pub model: Option<Model>,
    pub system_prompt: Option<String>,
    pub self_heal: Option<bool>,
    pub experimental: Option<bool>,
    pub dom_settle_timeout: Option<u32>,
    pub cache_dir: Option<String>,
    pub verbose: Option<i32>,
    pub log_inference_to_file: Option<bool>,
    pub disable_pino: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransportChoice {
    Grpc(String),
    Rest(String),
}

// --- Error Types ---
#[derive(Debug)]
pub enum StagehandError {
    Transport(String),
    Api(String),
    MissingApiKey(String),
    TonicStatus(tonic::Status),
}

impl fmt::Display for StagehandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StagehandError::Transport(msg) => write!(f, "Transport error: {}", msg),
            StagehandError::Api(msg) => write!(f, "API error: {}", msg),
            StagehandError::MissingApiKey(key) => write!(f, "Missing API key: {}", key),
            StagehandError::TonicStatus(s) => write!(f, "gRPC error: {}", s),
        }
    }
}

impl std::error::Error for StagehandError {}
impl From<tonic::transport::Error> for StagehandError {
    fn from(err: tonic::transport::Error) -> Self {
        StagehandError::Transport(err.to_string())
    }
}
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

// --- Transport Abstraction ---
#[async_trait]
pub trait Transport: Send + Sync {
    async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, StagehandError>> + Send>>, StagehandError>;
    async fn act(&mut self, session_id: &str, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError>;
    async fn extract(&mut self, session_id: &str, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError>;
    async fn observe(&mut self, session_id: &str, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError>;
    async fn execute(&mut self, session_id: &str, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError>;
    async fn close(&mut self, session_id: &str, force: bool) -> Result<(), StagehandError>;
}

// --- gRPC Transport Implementation ---
pub struct GrpcTransport {
    client: StagehandServiceClient<Channel>,
}

impl GrpcTransport {
    pub async fn new(url: String) -> Result<Self, StagehandError> {
        Ok(Self { client: StagehandServiceClient::connect(url).await? })
    }
}

#[async_trait]
impl Transport for GrpcTransport {
    async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, StagehandError>> + Send>>, StagehandError> {
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
        Ok(Box::pin(self.client.init(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn act(&mut self, _session_id: &str, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError> {
        let req = proto::ActRequest {
            instruction,
            model: model.map(|m| m.into()),
            variables,
            timeout_ms: timeout_ms.map(|t| t as i32),
            frame_id,
        };
        Ok(Box::pin(self.client.act(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn extract(&mut self, _session_id: &str, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        let req = proto::ExtractRequest {
            instruction,
            schema_json: schema.to_string(),
            model: model.map(|m| m.into()),
            timeout_ms: timeout_ms.map(|t| t as i32),
            selector,
            frame_id,
        };
        Ok(Box::pin(self.client.extract(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn observe(&mut self, _session_id: &str, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        let req = proto::ObserveRequest {
            instruction,
            model: model.map(|m| m.into()),
            timeout_ms: timeout_ms.map(|t| t as i32),
            selector,
            only_selectors,
            frame_id,
        };
        Ok(Box::pin(self.client.observe(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn execute(&mut self, session_id: &str, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        let agent_config_json = agent_config.map(|c| serde_json::to_string(&c).unwrap_or_default());
        let execute_options_json = execute_options.map(|o| serde_json::to_string(&o).unwrap_or_default());
        let req = proto::ExecuteRequest {
            session_id: session_id.to_string(),
            instruction,
            frame_id,
            agent_config_json,
            execute_options_json,
        };
        Ok(Box::pin(self.client.execute(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn close(&mut self, _session_id: &str, force: bool) -> Result<(), StagehandError> {
        self.client.close(proto::CloseRequest { force }).await.map_err(StagehandError::TonicStatus)?;
        Ok(())
    }
}

// --- REST Transport Implementation ---
pub struct RestTransport {
    base_url: String,
    api_key: String,
    project_id: String,
    model_api_key: Option<String>,
    client: Arc<Client>,
}

impl RestTransport {
    pub fn new(base_url: String) -> Result<Self, StagehandError> {
        Ok(Self {
            base_url,
            api_key: std::env::var("BROWSERBASE_API_KEY").map_err(|_| StagehandError::MissingApiKey("BROWSERBASE_API_KEY".to_string()))?,
            project_id: std::env::var("BROWSERBASE_PROJECT_ID").map_err(|_| StagehandError::MissingApiKey("BROWSERBASE_PROJECT_ID".to_string()))?,
            model_api_key: None, // Will be set in init if a model API key is provided
            client: Arc::new(Client::new()),
        })
    }
    
    async fn execute_stream(&self, session_id: &str, path: &str, body: serde_json::Value) -> Result<Pin<Box<dyn Stream<Item = Result<serde_json::Value, StagehandError>> + Send>>, StagehandError> {
        let url = format!("{}{}", self.base_url, path);
        let mut client_builder = ClientBuilder::for_url(&url)?
            .header("x-bb-api-key", &self.api_key)?
            .header("x-bb-project-id", &self.project_id)?;

        if !session_id.is_empty() {
            client_builder = client_builder.header("x-bb-session-id", session_id)?;
        }

        if let Some(model_api_key) = &self.model_api_key {
            client_builder = client_builder.header("x-model-api-key", model_api_key)?;
        }

        let sse_client = client_builder
            .method(reqwest::Method::POST.to_string())
            .body(body.to_string())
            .header("Content-Type", "application/json")?
            .header("x-stream-response", "true")?
            .build();

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
                                    let _ = tx.send(Err(StagehandError::Api(format!("Failed to parse SSE event data to JSON: {}", e.data)))).await;
                                }
                            },
                            _ => {}, // Ignore other SSE types (comments, connected, etc.)
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
}

#[async_trait]
impl Transport for RestTransport {
    async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, StagehandError>> + Send>>, StagehandError> {
        // Capture model API key if present in V3Options or env var
        if let Some(model) = opts.model.as_ref() {
            if let Model::Config { api_key, .. } = model {
                self.model_api_key = api_key.clone();
            } else if let Model::String(s) = model {
                if s.starts_with("openai/") {
                    self.model_api_key = std::env::var("OPENAI_API_KEY").ok();
                }
            }
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InitPayload<'a> {
            model_name: Option<String>,
            dom_settle_timeout_ms: Option<i32>,
            verbose: Option<i32>,
            system_prompt: Option<&'a String>,
            self_heal: Option<bool>,
            browserbase_session_create_params: Option<&'a serde_json::Value>,
            browserbase_session_id: Option<&'a String>,
        }
        let payload = InitPayload {
            model_name: opts.model.as_ref().map(|m| match m {
                Model::String(s) => s.clone(),
                Model::Config { model_name, .. } => model_name.clone(),
            }),
            dom_settle_timeout_ms: opts.dom_settle_timeout.map(|t| t as i32),
            verbose: opts.verbose,
            system_prompt: opts.system_prompt.as_ref(),
            self_heal: opts.self_heal,
            browserbase_session_create_params: opts.browserbase_session_create_params.as_ref(),
            browserbase_session_id: opts.browserbase_session_id.as_ref(),
        };
        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        // For Init, session_id is not yet known, so we pass a dummy empty string, which the API ignores for /sessions/start
        let json_stream = self.execute_stream("", "/sessions/start", body).await?;

        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| {
                // Explicitly parse based on event_type for InitResponse
                if let Some(event_type) = json_value["type"].as_str() {
                    match event_type {
                        "system" => {
                            if let Some(status) = json_value["data"]["status"].as_str() {
                                if status == "finished" {
                                    serde_json::from_value::<proto::InitResult>(json_value["data"]["result"].clone())
                                        .map(|res| proto::InitResponse { event: Some(proto::init_response::Event::Result(res)) })
                                        .map_err(|e| StagehandError::Api(format!("Failed to parse system finished result: {}", e)))
                                } else if status == "error" {
                                    Err(StagehandError::Api(json_value["data"]["error"].as_str().unwrap_or("Unknown server error").to_string()))
                                } else {
                                    // Other system events, ignore or map to a generic log
                                    Ok(proto::InitResponse { event: None })
                                }
                            } else {
                                Err(StagehandError::Api(format!("Missing status in system event: {}", json_value)))
                            }
                        },
                        "log" => {
                            serde_json::from_value::<proto::LogLine>(json_value["data"].clone())
                                .map(|log| proto::InitResponse { event: Some(proto::init_response::Event::Log(log)) })
                                .map_err(|e| StagehandError::Api(format!("Failed to parse log event: {}", e)))
                        },
                        _ => {
                            Err(StagehandError::Api(format!("Unrecognized event type in Init stream: {}", json_value)))
                        }
                    }
                } else {
                    // No 'type' field, try to parse directly as InitResult if it's the final event
                    serde_json::from_value::<proto::InitResult>(json_value.clone())
                        .map(|res| proto::InitResponse { event: Some(proto::init_response::Event::Result(res)) })
                        .map_err(|e| StagehandError::Api(format!("Failed to parse direct event as InitResult: {}", e)))
                }
            })
        })))
    }
    async fn act(&mut self, session_id: &str, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ActPayload<'a> {
            input: &'a String,
            frame_id: Option<&'a String>,
            options: ActOptionsPayload<'a>,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ActOptionsPayload<'a> {
            model: Option<proto::ModelConfiguration>,
            variables: &'a HashMap<String, String>,
            timeout_ms: Option<u32>,
        }

        let payload = ActPayload {
            input: &instruction,
            frame_id: frame_id.as_ref(),
            options: ActOptionsPayload {
                model: model.map(|m| m.into()),
                variables: &variables,
                timeout_ms,
            },
        };
        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/act", session_id), body).await?;
        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| serde_json::from_value::<proto::ActResponse>(json_value).map_err(|e| StagehandError::Api(e.to_string())))
        })))
    }
    async fn extract(&mut self, session_id: &str, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExtractPayload<'a> {
            instruction: &'a String,
            schema: &'a serde_json::Value,
            frame_id: Option<&'a String>,
            options: ExtractOptionsPayload,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExtractOptionsPayload {
            model: Option<proto::ModelConfiguration>,
            timeout_ms: Option<u32>,
            selector: Option<String>,
        }

        let payload = ExtractPayload {
            instruction: &instruction,
            schema: &schema,
            frame_id: frame_id.as_ref(),
            options: ExtractOptionsPayload {
                model: model.map(|m| m.into()),
                timeout_ms,
                selector,
            },
        };
        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/extract", session_id), body).await?;
        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| serde_json::from_value::<proto::ExtractResponse>(json_value).map_err(|e| StagehandError::Api(e.to_string())))
        })))
    }
    async fn observe(&mut self, session_id: &str, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ObservePayload<'a> {
            instruction: Option<&'a String>,
            frame_id: Option<&'a String>,
            options: ObserveOptionsPayload<'a>,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ObserveOptionsPayload<'a> {
            model: Option<proto::ModelConfiguration>,
            timeout_ms: Option<u32>,
            selector: Option<&'a String>,
            only_selectors: &'a Vec<String>,
        }

        let payload = ObservePayload {
            instruction: instruction.as_ref(),
            frame_id: frame_id.as_ref(),
            options: ObserveOptionsPayload {
                model: model.map(|m| m.into()),
                timeout_ms,
                selector: selector.as_ref(),
                only_selectors: &only_selectors,
            },
        };
        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/observe", session_id), body).await?;
        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| serde_json::from_value::<proto::ObserveResponse>(json_value).map_err(|e| StagehandError::Api(e.to_string())))
        })))
    }
    async fn execute(&mut self, session_id: &str, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExecutePayload<'a> {
            instruction: &'a String,
            frame_id: Option<&'a String>,
            agent_config: Option<&'a AgentConfig>,
            execute_options: Option<&'a AgentExecuteOptions>,
        }
        let payload = ExecutePayload {
            instruction: &instruction,
            frame_id: frame_id.as_ref(),
            agent_config: agent_config.as_ref(),
            execute_options: execute_options.as_ref(),
        };
        let body = serde_json::to_value(payload).map_err(|e| StagehandError::Api(e.to_string()))?;
        let json_stream = self.execute_stream(session_id, &format!("/sessions/{}/agentExecute", session_id), body).await?;
        Ok(Box::pin(json_stream.map(|item| {
            item.and_then(|json_value| serde_json::from_value::<proto::ExecuteResponse>(json_value).map_err(|e| StagehandError::Api(e.to_string())))
        })))
    }
    async fn close(&mut self, session_id: &str, _force: bool) -> Result<(), StagehandError> {
        let url = format!("{}/sessions/{}/end", self.base_url, session_id);
        self.client.post(&url).send().await?;
        Ok(())
    }
}


// --- The Stagehand Client ---
pub struct Stagehand {
    transport: Box<dyn Transport + Send + Sync>,
    session_id: Option<String>,
}

impl Stagehand {
    pub async fn connect(transport_choice: TransportChoice) -> Result<Self, StagehandError> {
        let transport: Box<dyn Transport + Send + Sync> = match transport_choice {
            TransportChoice::Grpc(url) => Box::new(GrpcTransport::new(url).await?),
            TransportChoice::Rest(base_url) => Box::new(RestTransport::new(base_url)?),
        };
        Ok(Self { transport, session_id: None })
    }

    pub async fn init(&mut self, opts: V3Options) -> Result<(), StagehandError> {
        let mut stream = self.transport.init(opts).await?;
        while let Some(item) = stream.next().await {
            match item {
                Ok(response) => {
                    if let Some(proto::init_response::Event::Result(res)) = response.event {
                        if !res.unused.is_empty() {
                            self.session_id = Some(res.unused);
                            return Ok(());
                        }
                    }
                },
                Err(e) => return Err(e),
            }
        }
        Err(StagehandError::Api("Init stream ended without a session ID.".to_string()))
    }

    pub async fn act(&mut self, instruction: impl Into<String>, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing for Act call".to_string()))?.clone();
        self.transport.act(&session_id, instruction.into(), model, variables, timeout_ms, frame_id).await
    }

    pub async fn extract<S: Serialize>(&mut self, instruction: impl Into<String>, schema: &S, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing for Extract call".to_string()))?.clone();
        let schema_value = serde_json::to_value(schema).map_err(|e| StagehandError::Api(e.to_string()))?;
        self.transport.extract(&session_id, instruction.into(), schema_value, model, timeout_ms, selector, frame_id).await
    }

    pub async fn observe(&mut self, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing for Observe call".to_string()))?.clone();
        self.transport.observe(&session_id, instruction, model, timeout_ms, selector, only_selectors, frame_id).await
    }

    pub async fn execute(&mut self, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing for Execute call".to_string()))?.clone();
        self.transport.execute(&session_id, instruction, frame_id, agent_config, execute_options).await
    }

    pub async fn close(&mut self, force: bool) -> Result<(), StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing for Close call".to_string()))?.clone();
        self.transport.close(&session_id, force).await
    }
}