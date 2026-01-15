//! Example: use a chromiumoxide `Page` with the Stagehand Rust SDK.
//!
//! What this demonstrates:
//! - Start a Stagehand session (remote Stagehand API / Browserbase browser)
//! - Attach chromiumoxide to the same browser via CDP (`browserbase_cdp_url`)
//! - Use a helper to convert a chromiumoxide `Page` into the Stagehand `frame_id`
//!   so Stagehand uses the correct page in `observe/act/extract`.
//!
//! Environment variables required:
//! - MODEL_API_KEY (or another supported model provider API key)
//! - BROWSERBASE_API_KEY
//! - BROWSERBASE_PROJECT_ID
//!
//! Optional:
//! - STAGEHAND_BASE_URL (defaults to https://api.stagehand.browserbase.com/v1)

use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::page::{GetFrameTreeParams, NavigateParams};
use futures::StreamExt;
use stagehand_sdk::{
    ActResponseEvent, Env, ExtractResponseEvent, Model, ObserveResponseEvent, Stagehand,
    TransportChoice, V3Options,
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();

    println!("=== Stagehand Rust SDK + chromiumoxide Page Example ===\n");

    println!("1. Connecting to Stagehand...");
    let mut stagehand = Stagehand::connect(TransportChoice::default_rest()).await?;
    println!("   Connected!\n");

    println!("2. Starting browser session...");
    let opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-5-nano".into())),
        verbose: Some(1),
        ..Default::default()
    };
    stagehand.start(opts).await?;
    println!("   Session ID: {:?}\n", stagehand.session_id());

    println!("3. Fetching Browserbase CDP URL...");
    let cdp_url = stagehand.browserbase_cdp_url().await?;
    println!("   CDP URL: {cdp_url}\n");

    println!("4. Connecting chromiumoxide over CDP...");
    let (browser, mut handler) = Browser::connect(&cdp_url).await?;
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });
    println!("   Connected!\n");

    println!("5. Getting a page and navigating with chromiumoxide...");
    let pages = browser.pages().await?;
    let page = if pages.is_empty() {
        browser.new_page("about:blank").await?
    } else {
        pages.into_iter().next().unwrap()
    };

    page.execute(NavigateParams::builder().url("https://example.com").build()?)
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    println!("   Chromiumoxide navigation complete.\n");

    println!("6. Resolving Stagehand frame_id from chromiumoxide page via Page.getFrameTree...");
      let frame_tree = page.execute(GetFrameTreeParams::default()).await?.result.frame_tree;
      let frame_id = frame_tree.frame.id.inner().clone();
    println!("   frame_id: {frame_id}\n");

    println!("7. Stagehand.observe(frame_id=...) ...");
    let mut observe_stream = stagehand
        .observe(
            Some("Find the most relevant click target on this page".to_string()),
            None,
            Some(30_000),
            None,
            Some(frame_id.clone()),
        )
        .await?;

    while let Some(msg) = observe_stream.next().await {
        if let Ok(event) = msg {
            if let Some(ObserveResponseEvent::ElementsJson(json)) = event.event {
                println!("   Observed elements JSON: {json}");
            }
        }
    }

    println!("\n8. Stagehand.extract(frame_id=...) ...");
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
            Some(frame_id.clone()),
        )
        .await?;

    while let Some(msg) = extract_stream.next().await {
        if let Ok(event) = msg {
            if let Some(ExtractResponseEvent::DataJson(json)) = event.event {
                println!("   Extracted: {json}");
            }
        }
    }

    println!("\n9. Stagehand.act(frame_id=...) ...");
    let mut act_stream = stagehand
        .act(
            "Click on the 'More information...' link",
            None,
            HashMap::new(),
            Some(30_000),
            Some(frame_id.clone()),
        )
        .await?;

    while let Some(msg) = act_stream.next().await {
        if let Ok(event) = msg {
            if let Some(ActResponseEvent::Success(success)) = event.event {
                println!("   Act success: {success}");
            }
        }
    }

    println!("\n10. Cleaning up...");
    handler_task.abort();
    stagehand.end().await?;
    println!("   Done.");

    Ok(())
}
