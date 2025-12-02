//! Integration test using the Stagehand cloud REST API with Browserbase.
//!
//! This test:
//! 1. Connects to Stagehand cloud API (REST + SSE)
//! 2. Creates a Browserbase session (cloud-managed browser)
//! 3. Navigates to https://example.com
//! 4. Calls stagehand extract, observe, and act
//! 5. Verifies the actions complete successfully

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use stagehand_sdk::{Env, Model, Stagehand, Transport, V3Options};
use std::collections::HashMap;

/// Schema for extracting links from the page
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinksSchema {
    links: Vec<LinkInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkInfo {
    text: String,
    href: String,
}

/// Schema for observed actions
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObservedAction {
    selector: String,
    description: String,
}

#[tokio::test]
async fn test_stagehand_cloud_integration(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // 1. Connect to Stagehand cloud API using REST transport
    let api_base = std::env::var("STAGEHAND_API_URL")
        .unwrap_or_else(|_| "https://api.stagehand.browserbase.com/v1".to_string());

    println!("Connecting to Stagehand API at: {}", api_base);

    let mut stagehand = Stagehand::connect(
        api_base.clone(),
        Transport::Rest(api_base),
    )
    .await?;

    // 2. Initialize Stagehand - this creates a Browserbase session
    let init_opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-4o".into())),
        verbose: Some(2),
        ..Default::default()
    };

    println!("Initializing Stagehand session...");
    let mut init_stream = stagehand.init(init_opts).await?;
    while let Some(res) = init_stream.next().await {
        match res {
            Ok(init_response) => {
                if let Some(stagehand_sdk::proto::init_response::Event::Result(result)) =
                    init_response.event
                {
                    println!("Stagehand initialization complete. Session: {}", result.unused);
                } else if let Some(stagehand_sdk::proto::init_response::Event::Log(log)) =
                    init_response.event
                {
                    println!("[INIT LOG] [{}] {}", log.category, log.message);
                }
            }
            Err(e) => {
                eprintln!("Initialization stream error: {:?}", e);
            }
        }
    }

    // 3. Navigate to example.com using act
    println!("\n--- Navigating to example.com ---");
    let mut nav_stream = stagehand
        .act(
            "Navigate to https://example.com",
            None,
            HashMap::new(),
            Some(60_000),
            None,
        )
        .await?;

    while let Some(res) = nav_stream.next().await {
        match res {
            Ok(act_response) => {
                if let Some(stagehand_sdk::proto::act_response::Event::Success(success)) =
                    act_response.event
                {
                    println!("[NAVIGATE] Success: {}", success);
                }
            }
            Err(e) => {
                eprintln!("Navigate stream error: {:?}", e);
            }
        }
    }

    // Wait for page to load
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 4. Call extract to get links on the page
    println!("\n--- Extracting links from the page ---");
    let schema = LinksSchema { links: vec![] };

    let mut extract_stream = stagehand
        .extract(
            "Extract all the links on this page, including their text and href attributes",
            &schema,
            None,
            Some(60_000),
            None,
            None,
        )
        .await?;

    let mut extracted_links: Option<LinksSchema> = None;
    while let Some(res) = extract_stream.next().await {
        match res {
            Ok(extract_response) => {
                if let Some(stagehand_sdk::proto::extract_response::Event::DataJson(json_str)) =
                    extract_response.event
                {
                    println!("[EXTRACT RESULT] {}", json_str);
                    if let Ok(links) = serde_json::from_str::<LinksSchema>(&json_str) {
                        extracted_links = Some(links);
                    }
                }
            }
            Err(e) => {
                eprintln!("Extract stream error: {:?}", e);
            }
        }
    }

    if let Some(links) = &extracted_links {
        println!("Found {} links on the page", links.links.len());
        for link in &links.links {
            println!("  - {} -> {}", link.text, link.href);
        }
    }

    // 5. Call observe to find clickable links
    println!("\n--- Observing clickable elements ---");
    let mut observe_stream = stagehand
        .observe(
            Some("Find the links that can be clicked".to_string()),
            None,
            Some(60_000),
            None,
            vec![],
            None,
        )
        .await?;

    let mut observed_actions: Vec<ObservedAction> = vec![];
    while let Some(res) = observe_stream.next().await {
        match res {
            Ok(observe_response) => {
                if let Some(stagehand_sdk::proto::observe_response::Event::ElementsJson(json_str)) =
                    observe_response.event
                {
                    println!("[OBSERVE RESULT] {}", json_str);
                    if let Ok(actions) = serde_json::from_str::<Vec<ObservedAction>>(&json_str) {
                        observed_actions = actions;
                    }
                }
            }
            Err(e) => {
                eprintln!("Observe stream error: {:?}", e);
            }
        }
    }

    println!("Found {} observable actions", observed_actions.len());
    for action in &observed_actions {
        println!("  - {} ({})", action.description, action.selector);
    }

    // 6. Click the "More information..." link on example.com
    println!("\n--- Acting on 'More information...' link ---");

    let mut act_stream = stagehand
        .act(
            "Click on the 'More information...' link",
            None,
            HashMap::new(),
            Some(60_000),
            None,
        )
        .await?;

    let mut act_success = false;
    while let Some(res) = act_stream.next().await {
        match res {
            Ok(act_response) => {
                if let Some(stagehand_sdk::proto::act_response::Event::Success(success)) =
                    act_response.event
                {
                    println!("[ACT RESULT] Success: {}", success);
                    act_success = success;
                }
            }
            Err(e) => {
                eprintln!("Act stream error: {:?}", e);
            }
        }
    }

    // 7. Verify the action was successful
    assert!(act_success, "The act command should have succeeded");

    // 8. Close Stagehand
    stagehand.close(true).await?;

    println!("\nTest completed successfully!");
    Ok(())
}

/// Simpler test that just verifies chromiumoxide can launch and navigate
#[tokio::test]
async fn test_chromiumoxide_basic() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Launch browser in headless mode for reliability
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .window_size(1280, 720)
            .build()
            .map_err(|e| format!("Failed to build config: {}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to launch browser: {}", e))?;

    // Spawn handler
    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    // Create page and navigate
    let page = browser
        .new_page("https://example.com")
        .await
        .map_err(|e| format!("Failed to create page: {}", e))?;
    page.wait_for_navigation()
        .await
        .map_err(|e| format!("Failed to wait for navigation: {}", e))?;

    // Get URL
    let url = page
        .url()
        .await
        .map_err(|e| format!("Failed to get URL: {}", e))?
        .unwrap_or_default();
    println!("Navigated to: {}", url);
    assert!(url.contains("example.com"));

    // Get WebSocket address
    let ws = browser.websocket_address();
    println!("WebSocket address: {}", ws);

    // Cleanup
    browser
        .close()
        .await
        .map_err(|e| format!("Failed to close browser: {}", e))?;
    handle.abort();

    Ok(())
}
