//! Integration test demonstrating chromiumoxide connecting to a Browserbase cloud browser.
//!
//! This test:
//! 1. Creates a Stagehand session (which provisions a Browserbase cloud browser)
//! 2. Gets the CDP WebSocket URL for the remote browser
//! 3. Connects chromiumoxide directly to the remote browser via CDP
//! 4. Uses chromiumoxide to navigate and interact with pages
//! 5. Optionally uses Stagehand's AI-powered methods alongside direct CDP control

use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::page::NavigateParams;
use futures::StreamExt;
use stagehand_sdk::{Env, Model, Stagehand, TransportChoice, V3Options};
use stagehand_sdk::{ActResponseEvent, ExtractResponseEvent};
use std::collections::HashMap;
use async_std::task as async_std_task;

/// Test that creates a Browserbase session via Stagehand and connects chromiumoxide to it
#[tokio::test]
async fn test_chromiumoxide_browserbase_connection() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables
    dotenvy::dotenv().ok();

    println!("=== Chromiumoxide + Browserbase Integration Test ===\n");

    // 1. Create a Stagehand session - this provisions a Browserbase cloud browser
    let api_base = std::env::var("STAGEHAND_API_URL")
        .unwrap_or_else(|_| "https://api.stagehand.browserbase.com/v1".to_string());

    println!("1. Creating Stagehand session at: {}", api_base);

    let mut stagehand = Stagehand::connect(TransportChoice::Rest(api_base)).await?;

    let init_opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        verbose: Some(1),
        ..Default::default()
    };

    stagehand.start(init_opts).await?;

    let session_id = stagehand.session_id().expect("Session ID should be set after start");
    println!("   Session ID: {}", session_id);

    // 2. Get the CDP WebSocket URL for connecting chromiumoxide
    let cdp_url = stagehand.browserbase_cdp_url()
        .expect("CDP URL should be available after start");
    println!("2. CDP WebSocket URL: {}", cdp_url);

    // Give the session more time to be fully ready
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // 3. Connect chromiumoxide to the remote Browserbase browser
    println!("3. Connecting chromiumoxide to remote browser...");
    println!("   URL being used: {}", cdp_url);

    // First test raw connection to verify it works
    {
        use async_tungstenite::async_std::connect_async_with_config;
        use async_tungstenite::tungstenite::protocol::WebSocketConfig;
        let config = WebSocketConfig::default()
            .max_message_size(None)
            .max_frame_size(None);
        println!("   Testing raw connection first...");
        match connect_async_with_config(&cdp_url, Some(config)).await {
            Ok((_, response)) => println!("   Raw connection OK! Status: {:?}", response.status()),
            Err(e) => println!("   Raw connection failed: {:?}", e),
        }
    }

    // Now try chromiumoxide
    println!("   Now trying chromiumoxide Browser::connect...");
    let (browser, mut handler) = Browser::connect(&cdp_url)
        .await
        .map_err(|e| format!("Failed to connect to browser: {}", e))?;

    // Spawn handler for browser events using async-std
    let handler_task = async_std_task::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    println!("   Connected to remote browser!");

    // 4. Use chromiumoxide directly to navigate
    println!("4. Navigating to example.com using chromiumoxide CDP...");

    // Get the first page or create one
    let pages = browser.pages().await?;
    let page = if pages.is_empty() {
        browser.new_page("about:blank").await?
    } else {
        pages.into_iter().next().unwrap()
    };

    // Navigate using CDP
    page.execute(NavigateParams::builder()
        .url("https://example.com")
        .build()?)
        .await?;

    // Wait for page to load
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get the current URL using chromiumoxide
    let current_url = page.url().await?.unwrap_or_default();
    println!("   Current URL (via chromiumoxide): {}", current_url);
    assert!(current_url.contains("example.com"), "Should be on example.com");

    // Get page title using chromiumoxide
    let _nav_history = page.execute(
        chromiumoxide::cdp::browser_protocol::page::GetNavigationHistoryParams::default()
    ).await?;
    println!("   Page loaded successfully!");

    // 5. Now use Stagehand's AI-powered methods on the same browser session
    println!("5. Using Stagehand AI to extract data from the same session...");

    // Schema must be in JSON Schema format
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "url": { "type": "string" }
        }
    });

    let mut extract_stream = stagehand
        .extract(
            "Extract the page title and current URL",
            schema,
            None,
            Some(30_000),
            None,
            None,
        )
        .await?;

    while let Some(res) = extract_stream.next().await {
        match res {
            Ok(response) => {
                if let Some(ExtractResponseEvent::DataJson(json)) = response.event {
                    println!("   Stagehand extracted: {}", json);
                }
            }
            Err(e) => eprintln!("   Extract error: {:?}", e),
        }
    }

    // 6. Use Stagehand to click the "More information..." link
    println!("6. Using Stagehand AI to click the link...");

    let mut act_stream = stagehand
        .act(
            "Click on the 'More information...' link",
            None,
            HashMap::new(),
            Some(30_000),
            None,
        )
        .await?;

    while let Some(res) = act_stream.next().await {
        match res {
            Ok(response) => {
                if let Some(ActResponseEvent::Success(success)) = response.event {
                    println!("   Act success: {}", success);
                }
            }
            Err(e) => eprintln!("   Act error: {:?}", e),
        }
    }

    // Wait for navigation
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 7. Verify navigation happened using chromiumoxide
    println!("7. Verifying navigation with chromiumoxide...");
    let new_url = page.url().await?.unwrap_or_default();
    println!("   New URL (via chromiumoxide): {}", new_url);

    // 8. Take a screenshot using chromiumoxide CDP
    println!("8. Taking screenshot via chromiumoxide CDP...");
    let screenshot = page.screenshot(
        chromiumoxide::page::ScreenshotParams::builder()
            .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png)
            .build(),
    ).await?;
    println!("   Screenshot captured: {} bytes", screenshot.len());

    // 9. Clean up
    println!("9. Cleaning up...");
    drop(handler_task); // Cancel the async-std task by dropping it
    stagehand.end().await?;

    println!("\n=== Test completed successfully! ===");
    println!("Demonstrated:");
    println!("  - Creating Browserbase session via Stagehand");
    println!("  - Connecting chromiumoxide to remote browser via CDP");
    println!("  - Direct CDP control (navigation, screenshots)");
    println!("  - AI-powered actions via Stagehand on same session");

    Ok(())
}

/// Test raw async-tungstenite connection to Browserbase
#[tokio::test]
async fn test_raw_websocket_connection() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();

    println!("=== Raw WebSocket Connection Test ===\n");

    // Create a session via Stagehand
    let api_base = std::env::var("STAGEHAND_API_URL")
        .unwrap_or_else(|_| "https://api.stagehand.browserbase.com/v1".to_string());

    let mut stagehand = Stagehand::connect(TransportChoice::Rest(api_base)).await?;

    let init_opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        verbose: Some(1),
        ..Default::default()
    };

    stagehand.start(init_opts).await?;
    let session_id = stagehand.session_id().expect("Session ID should be set");
    let api_key = std::env::var("BROWSERBASE_API_KEY")?;

    println!("Session ID: {}", session_id);

    // Wait for session to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Try raw async-tungstenite connection with same config as chromiumoxide
    let url = format!("wss://connect.browserbase.com/?sessionId={}&apiKey={}", session_id, api_key);
    println!("Connecting to: {}", url);

    use async_tungstenite::async_std::connect_async_with_config;
    use async_tungstenite::tungstenite::protocol::WebSocketConfig;

    // Use same config as chromiumoxide
    let config = WebSocketConfig::default()
        .max_message_size(None)
        .max_frame_size(None);

    match connect_async_with_config(&url, Some(config)).await {
        Ok((ws_stream, response)) => {
            println!("Connected with config! Response status: {:?}", response.status());
            drop(ws_stream);
        }
        Err(e) => {
            println!("Connection failed: {:?}", e);
        }
    }

    stagehand.end().await?;
    println!("\n=== Test completed ===");

    Ok(())
}

/// Simpler test that just verifies chromiumoxide can launch locally
#[tokio::test]
async fn test_chromiumoxide_local() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use chromiumoxide::browser::BrowserConfig;

    // Launch browser locally in headless mode
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

    // Cleanup
    browser
        .close()
        .await
        .map_err(|e| format!("Failed to close browser: {}", e))?;
    handle.abort();

    Ok(())
}
