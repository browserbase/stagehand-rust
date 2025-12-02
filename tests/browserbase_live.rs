use stagehand_sdk::{Stagehand, V3Options, Env, Model, Transport};
use tokio_stream::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Quote {
    text: String,
    author: String,
}

#[tokio::test]
async fn test_browserbase_live_extract() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Verify required environment variables are set
    if std::env::var("BROWSERBASE_API_KEY").is_err() {
        panic!("BROWSERBASE_API_KEY must be set in .env file");
    }
    if std::env::var("BROWSERBASE_PROJECT_ID").is_err() {
        panic!("BROWSERBASE_PROJECT_ID must be set in .env file");
    }
    if std::env::var("OPENAI_API_KEY").is_err() {
        println!("Warning: OPENAI_API_KEY is not set. The test may fail if the model requires it.");
    }

    // 2. Connect to Stagehand using the REST transport
    let mut stagehand = Stagehand::connect(
        "https://api.stagehand.browserbase.com/v1".to_string(),
        Transport::Rest("https://api.stagehand.browserbase.com/v1".to_string()),
    )
    .await?;

    // 3. Initialize Stagehand for a Browserbase session
    let init_opts = V3Options {
        env: Some(Env::Browserbase),
        model: Some(Model::String("openai/gpt-4o".into())),
        ..Default::default()
    };
    
    let mut init_stream = stagehand.init(init_opts).await?;
    while let Some(res) = init_stream.next().await {
        match res {
            Ok(init_response) => {
                if let Some(stagehand_sdk::proto::init_response::Event::Result(_)) = init_response.event {
                    println!("Initialization complete.");
                } else if let Some(stagehand_sdk::proto::init_response::Event::Log(log)) = init_response.event {
                    println!("[INIT LOG] {:?}", log);
                }
            }
            Err(e) => {
                eprintln!("Initialization stream error: {:?}", e);
                return Err(e.into());
            }
        }
    }

    // 4. Perform an 'act' instruction: navigate to a page
    let mut act_stream = stagehand.act(
        "Go to https://quotes.toscrape.com/".to_string(),
        None,
        Default::default(),
        Some(60_000), // 60-second timeout
        Some("main".to_string()), // Added frame_id
    )
    .await?;

    while let Some(res) = act_stream.next().await {
        if let Err(e) = res {
            eprintln!("Act stream error: {:?}", e);
            // Log messages from the REST client are currently surfaced as errors.
            // We can print them for debugging but not fail the test.
        }
    }
    
    // 5. Perform an 'extract' instruction
    let schema = Quote {
        text: "".to_string(),
        author: "".to_string(),
    };

    let mut extract_stream = stagehand.extract(
        "Extract the first quote on the page, including the text and the author.".to_string(),
        &schema,
        None,
        Some(60_000),
        None,
        Some("main".to_string()), // Added frame_id
    )
    .await?;

    let mut extracted_quote: Option<Quote> = None;
    while let Some(res) = extract_stream.next().await {
        match res {
            Ok(extract_response) => {
                if let Some(stagehand_sdk::proto::extract_response::Event::DataJson(json_str)) = extract_response.event {
                    let quote: Quote = serde_json::from_str(&json_str)?;
                    extracted_quote = Some(quote);
                }
            }
            Err(e) => {
                eprintln!("Extract stream error: {:?}", e);
            }
        }
    }

    // 6. Clean up the session
    stagehand.close(true).await?;

    // 7. Assert the result
    assert!(extracted_quote.is_some(), "Failed to extract a quote.");
    let quote = extracted_quote.unwrap();
    assert_eq!(quote.author, "Albert Einstein");
    assert!(quote.text.starts_with("“The world as we have created it"));

    Ok(())
}