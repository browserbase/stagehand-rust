//! Integration test using chromiumoxide to spawn a local browser and interact with Stagehand.
//!
//! This test:
//! 1. Spawns a local headful browser using chromiumoxide
//! 2. Navigates to https://example.com
//! 3. Gets the frame_id from chromiumoxide
//! 4. Calls stagehand extract, observe, and act with frame_id
//! 5. Verifies the URL changed after clicking a link

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use stagehand_sdk::{Env, LocalBrowserLaunchOptions, Model, Stagehand, Transport, V3Options};
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
#[ignore] // This test requires a running Stagehand gRPC server and Chrome installed
async fn test_chromiumoxide_stagehand_integration(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // 1. Launch browser with chromiumoxide (headful mode)
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .with_head() // headful mode
            .window_size(1280, 720)
            .build()
            .map_err(|e| format!("Failed to build browser config: {}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to launch browser: {}", e))?;

    // Spawn the browser handler
    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    // 2. Create a new page and navigate to example.com
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| format!("Failed to create page: {}", e))?;

    // Get the CDP WebSocket URL for Stagehand to connect to the same browser
    let ws_url = browser.websocket_address();
    println!("Browser WebSocket URL: {}", ws_url);

    // Navigate to example.com
    page.goto("https://example.com")
        .await
        .map_err(|e| format!("Failed to navigate: {}", e))?;
    page.wait_for_navigation()
        .await
        .map_err(|e| format!("Failed to wait for navigation: {}", e))?;

    // Get the initial URL
    let initial_url = page
        .url()
        .await
        .map_err(|e| format!("Failed to get URL: {}", e))?
        .unwrap_or_default();
    println!("Initial URL: {}", initial_url);
    assert!(
        initial_url.contains("example.com"),
        "Should be on example.com"
    );

    // 3. Get the frame_id (main frame) - use target_id as frame identifier
    let frame_id = format!("{:?}", page.target_id());
    println!("Main frame ID: {}", frame_id);

    // 4. Connect Stagehand to the browser using gRPC transport with CDP URL
    // Note: This requires a running Stagehand gRPC server
    let mut stagehand = Stagehand::connect(
        "http://127.0.0.1:50051".to_string(),
        Transport::Grpc("http://127.0.0.1:50051".to_string()),
    )
    .await?;

    // Initialize Stagehand with the CDP URL pointing to our browser
    let init_opts = V3Options {
        env: Some(Env::Local),
        local_browser_launch_options: Some(LocalBrowserLaunchOptions {
            cdp_url: Some(ws_url.to_string()),
            headless: Some(false),
            ..Default::default()
        }),
        model: Some(Model::String("openai/gpt-4o".into())),
        verbose: Some(2),
        ..Default::default()
    };

    let mut init_stream = stagehand.init(init_opts).await?;
    while let Some(res) = init_stream.next().await {
        match res {
            Ok(init_response) => {
                if let Some(stagehand_sdk::proto::init_response::Event::Result(_)) =
                    init_response.event
                {
                    println!("Stagehand initialization complete.");
                } else if let Some(stagehand_sdk::proto::init_response::Event::Log(log)) =
                    init_response.event
                {
                    println!("[INIT LOG] {:?}", log);
                }
            }
            Err(e) => {
                eprintln!("Initialization stream error: {:?}", e);
            }
        }
    }

    // 5. Call extract to get links on the page
    println!("\n--- Extracting links from the page ---");
    let schema = LinksSchema { links: vec![] };

    let mut extract_stream = stagehand
        .extract(
            "Extract all the links on this page, including their text and href attributes",
            &schema,
            None,
            Some(60_000),
            None,
            Some(frame_id.clone()),
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

    // 6. Call observe to find clickable links
    println!("\n--- Observing clickable elements ---");
    let mut observe_stream = stagehand
        .observe(
            Some("Find the links that can be clicked".to_string()),
            None,
            Some(60_000),
            None,
            vec![],
            Some(frame_id.clone()),
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

    // 7. Call act on the first observed action (if any)
    if !observed_actions.is_empty() {
        let first_action = &observed_actions[0];
        println!(
            "\n--- Acting on first action: {} ---",
            first_action.description
        );

        let mut act_stream = stagehand
            .act(
                format!("Click on: {}", first_action.description),
                None,
                HashMap::new(),
                Some(60_000),
                Some(frame_id.clone()),
            )
            .await?;

        while let Some(res) = act_stream.next().await {
            match res {
                Ok(act_response) => {
                    if let Some(stagehand_sdk::proto::act_response::Event::Success(success)) =
                        act_response.event
                    {
                        println!("[ACT RESULT] Success: {}", success);
                    }
                }
                Err(e) => {
                    eprintln!("Act stream error: {:?}", e);
                }
            }
        }

        // Wait for navigation to complete
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    } else {
        // If no observed actions, try clicking the "More information..." link on example.com
        println!("\n--- Acting on 'More information...' link ---");

        let mut act_stream = stagehand
            .act(
                "Click on the 'More information...' link",
                None,
                HashMap::new(),
                Some(60_000),
                Some(frame_id.clone()),
            )
            .await?;

        while let Some(res) = act_stream.next().await {
            match res {
                Ok(act_response) => {
                    if let Some(stagehand_sdk::proto::act_response::Event::Success(success)) =
                        act_response.event
                    {
                        println!("[ACT RESULT] Success: {}", success);
                    }
                }
                Err(e) => {
                    eprintln!("Act stream error: {:?}", e);
                }
            }
        }

        // Wait for navigation
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // 8. Verify the URL has changed
    let final_url = page
        .url()
        .await
        .map_err(|e| format!("Failed to get final URL: {}", e))?
        .unwrap_or_default();
    println!("\n--- URL Verification ---");
    println!("Initial URL: {}", initial_url);
    println!("Final URL: {}", final_url);

    // The URL should have changed after clicking a link
    // On example.com, clicking "More information..." should navigate to iana.org
    assert_ne!(
        initial_url, final_url,
        "URL should have changed after clicking a link"
    );

    // 9. Close Stagehand and browser
    stagehand.close(true).await?;
    browser
        .close()
        .await
        .map_err(|e| format!("Failed to close browser: {}", e))?;
    handle.abort();

    println!("\nTest completed successfully!");
    Ok(())
}

/// Simpler test that just verifies chromiumoxide can launch and navigate
#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn test_chromiumoxide_basic() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Launch browser
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .with_head()
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
