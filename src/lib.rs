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
    async fn act(&mut self, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError>;
    async fn extract(&mut self, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError>;
    async fn observe(&mut self, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError>;
    async fn execute(&mut self, session_id: String, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError>;
    async fn close(&mut self, force: bool) -> Result<(), StagehandError>;
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

    async fn act(&mut self, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError> {
        let req = proto::ActRequest {
            instruction,
            model: model.map(|m| m.into()),
            variables,
            timeout_ms: timeout_ms.map(|t| t as i32),
            frame_id,
        };
        Ok(Box::pin(self.client.act(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn extract(&mut self, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError> {
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

    async fn observe(&mut self, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError> {
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

    async fn execute(&mut self, session_id: String, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        let agent_config_json = agent_config.map(|c| serde_json::to_string(&c).unwrap_or_default());
        let execute_options_json = execute_options.map(|o| serde_json::to_string(&o).unwrap_or_default());
        let req = proto::ExecuteRequest {
            session_id,
            instruction,
            frame_id,
            agent_config_json,
            execute_options_json,
        };
        Ok(Box::pin(self.client.execute(req).await.map_err(StagehandError::TonicStatus)?.into_inner().map(|item| item.map_err(StagehandError::TonicStatus))))
    }

    async fn close(&mut self, force: bool) -> Result<(), StagehandError> {
        self.client.close(proto::CloseRequest { force }).await.map_err(StagehandError::TonicStatus)?;
        Ok(())
    }
}

// --- REST Transport Implementation ---
pub struct RestTransport {
    base_url: String,
    api_key: String,
    project_id: String,
    session_id: Option<String>,
    client: Arc<Client>,
}

impl RestTransport {
    pub fn new(base_url: String) -> Result<Self, StagehandError> {
        Ok(Self {
            base_url,
            api_key: std::env::var("BROWSERBASE_API_KEY").map_err(|_| StagehandError::MissingApiKey("BROWSERBASE_API_KEY".to_string()))?,
            project_id: std::env::var("BROWSERBASE_PROJECT_ID").map_err(|_| StagehandError::MissingApiKey("BROWSERBASE_PROJECT_ID".to_string()))?,
            session_id: None,
            client: Arc::new(Client::new()),
        })
    }
    
    async fn execute_stream<T: for<'de> Deserialize<'de> + Send + 'static>(&self, path: &str, body: serde_json::Value) -> Result<Pin<Box<dyn Stream<Item = Result<T, StagehandError>> + Send>>, StagehandError> {
        let url = format!("{}{}", self.base_url, path);
        let mut client_builder = ClientBuilder::for_url(&url)?
            .header("x-bb-api-key", &self.api_key)?
            .header("x-bb-project-id", &self.project_id)?;

        if let Some(session_id) = &self.session_id {
            client_builder = client_builder.header("x-bb-session-id", session_id)?;
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
                    Ok(SSE::Event(e)) => {
                        if let Ok(data) = serde_json::from_str::<T>(&e.data) {
                            if tx.send(Ok(data)).await.is_err() {
                                break;
                            }
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(Err(StagehandError::Transport(e.to_string()))).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

#[async_trait]
impl Transport for RestTransport {
    async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, StagehandError>> + Send>>, StagehandError> {
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
        self.execute_stream("/sessions/start", body).await
    }
    async fn act(&mut self, instruction: String, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing".to_string()))?;
        let body = serde_json::json!({
            "input": instruction,
            "frameId": frame_id,
            "options": {
                "model": model.map(|m| -> proto::ModelConfiguration { m.into() }),
                "variables": variables,
                "timeoutMs": timeout_ms
            }
        });
        self.execute_stream(&format!("/sessions/{}/act", session_id), body).await
    }
    async fn extract(&mut self, instruction: String, schema: serde_json::Value, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing".to_string()))?;
        let body = serde_json::json!({
            "instruction": instruction,
            "schema": schema,
            "frameId": frame_id,
            "options": {
                "model": model.map(|m| -> proto::ModelConfiguration { m.into() }),
                "timeoutMs": timeout_ms,
                "selector": selector,
            }
        });
        self.execute_stream(&format!("/sessions/{}/extract", session_id), body).await
    }
    async fn observe(&mut self, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| StagehandError::Api("Session ID is missing".to_string()))?;
        let body = serde_json::json!({
            "instruction": instruction,
            "frameId": frame_id,
            "options": {
                "model": model.map(|m| -> proto::ModelConfiguration { m.into() }),
                "timeoutMs": timeout_ms,
                "selector": selector,
                "onlySelectors": only_selectors,
            }
        });
        self.execute_stream(&format!("/sessions/{}/observe", session_id), body).await
    }
    async fn execute(&mut self, session_id: String, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        let body = serde_json::json!({
            "instruction": instruction,
            "frameId": frame_id,
            "agentConfig": agent_config,
            "executeOptions": execute_options,
        });
        self.execute_stream(&format!("/sessions/{}/agentExecute", session_id), body).await
    }
    async fn close(&mut self, _force: bool) -> Result<(), StagehandError> {
        if let Some(session_id) = self.session_id.take() {
            let url = format!("{}/sessions/{}/end", self.base_url, session_id);
            self.client.post(&url).send().await?;
        }
        Ok(())
    }
}


// --- The Stagehand Client ---
pub struct Stagehand {
    transport: Box<dyn Transport + Send + Sync>,
}

impl Stagehand {
    pub async fn connect(transport_choice: TransportChoice) -> Result<Self, StagehandError> {
        let transport: Box<dyn Transport + Send + Sync> = match transport_choice {
            TransportChoice::Grpc(url) => Box::new(GrpcTransport::new(url).await?),
            TransportChoice::Rest(base_url) => Box::new(RestTransport::new(base_url)?),
        };
        Ok(Self { transport })
    }

    pub async fn init(&mut self, opts: V3Options) -> Result<Pin<Box<dyn Stream<Item = Result<proto::InitResponse, StagehandError>> + Send>>, StagehandError> {
        self.transport.init(opts).await
    }

    pub async fn act(&mut self, instruction: impl Into<String>, model: Option<Model>, variables: HashMap<String, String>, timeout_ms: Option<u32>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ActResponse, StagehandError>> + Send>>, StagehandError> {
        self.transport.act(instruction.into(), model, variables, timeout_ms, frame_id).await
    }

    pub async fn extract<S: Serialize>(&mut self, instruction: impl Into<String>, schema: &S, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExtractResponse, StagehandError>> + Send>>, StagehandError> {
        let schema_value = serde_json::to_value(schema).map_err(|e| StagehandError::Api(e.to_string()))?;
        self.transport.extract(instruction.into(), schema_value, model, timeout_ms, selector, frame_id).await
    }

    pub async fn observe(&mut self, instruction: Option<String>, model: Option<Model>, timeout_ms: Option<u32>, selector: Option<String>, only_selectors: Vec<String>, frame_id: Option<String>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ObserveResponse, StagehandError>> + Send>>, StagehandError> {
        self.transport.observe(instruction, model, timeout_ms, selector, only_selectors, frame_id).await
    }

    pub async fn execute(&mut self, session_id: String, instruction: String, frame_id: Option<String>, agent_config: Option<AgentConfig>, execute_options: Option<AgentExecuteOptions>) -> Result<Pin<Box<dyn Stream<Item = Result<proto::ExecuteResponse, StagehandError>> + Send>>, StagehandError> {
        self.transport.execute(session_id, instruction, frame_id, agent_config, execute_options).await
    }

    pub async fn close(&mut self, force: bool) -> Result<(), StagehandError> {
        self.transport.close(force).await
    }
}
