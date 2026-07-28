use std::fmt;
use std::sync::Arc;
use teloxide::prelude::*;
use reqwest;
use serde::Deserialize;
use fantoccini::{ClientBuilder, Locator};
use tokio::time;
use serde_json::json;
use config::{Config as ConfigLoader, File};

#[derive(Deserialize, Debug)]
struct METARObject {
    rawOb: String,
}

#[derive(Debug, Default)]
struct FltObject {
    flight_number: Option<String>,
    gate_departure_time: Option<String>,
    takeoff_time: Option<String>,
    landing_time: Option<String>,
    gate_arrival_time: Option<String>,
}

#[derive(Deserialize, Clone)]
struct Config {
    driver: String,
    headless: bool,
    webdriver_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            driver: "geckodriver".to_string(),
            headless: false,
            webdriver_url: "http://localhost:4444".to_string(),
        }
    }
}

impl fmt::Display for FltObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f, 
            "Flight: {}\nGate Dept: {}\nTakeoff: {}\nLanding: {}\nGate Arrv: {}\n",
            if self.flight_number.is_some() { self.flight_number.clone().unwrap() } else { "N/A".to_string() },
            if self.gate_departure_time.is_some() { self.gate_departure_time.clone().unwrap() } else { "N/A".to_string() },
            if self.takeoff_time.is_some() { self.takeoff_time.clone().unwrap() } else { "N/A".to_string() },
            if self.landing_time.is_some() { self.landing_time.clone().unwrap() } else { "N/A".to_string() },
            if self.gate_arrival_time.is_some() { self.gate_arrival_time.clone().unwrap() } else { "N/A".to_string() }
        )
    }
}

impl FltObject {
    fn from_time_vec(times: Vec<String>) -> Self {
        let mut new_self = Self::default();
       
        if let Some(gate_departure_time) = times.get(0) {
            new_self.gate_departure_time = Some(gate_departure_time.clone());
        }

        if let Some(takeoff_time) = times.get(1) {
            new_self.takeoff_time = Some(takeoff_time.clone());
        }

        if let Some(landing_time) = times.get(2) {
            new_self.landing_time = Some(landing_time.clone())
        }

        if let Some(gate_arrival_time) = times.get(3) {
            new_self.gate_arrival_time = Some(gate_arrival_time.clone());
        }

        new_self
    }

    fn add_flight_number(mut self, flt_num: &str) -> Self {
        self.flight_number = Some(flt_num.to_string());
        self
    }

}

fn load_config() -> Result<Config, config::ConfigError> {
    let settings = ConfigLoader::builder()
        .add_source(File::with_name("config"))
        .build()?;

    settings.try_deserialize()
}

#[tokio::main]
async fn main() {
    let config = Arc::new(load_config().unwrap_or_default());

    pretty_env_logger::init();

    let bot = Bot::from_env();

    
    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let config = Arc::clone(&config);
        
        async move {
            if let Some(text) = msg.text() {
                let mut items = text.split(" ");
                let Some(command) = items.next() else {
                    panic!("Invalid command");
                };
                let value = items.next();

                match command {
                    "/metar" => {
                        if let Ok(metar_response) = fetch_metar(value).await {
                            if let Some(metar) = metar_response.first() {
                                bot.send_message(msg.chat.id, &metar.rawOb).await?;
                            }
                        } else {
                            bot.send_message(msg.chat.id, format!("Could not retrieve METAR")).await?;
                        }
                    }
                    "/search" => {
                        let output = search_airport().await;
                    }
                    "/flt" => {
                        if let Ok(flt_num_response) = search_flt_num(value, config.as_ref()).await {
                            bot.send_message(msg.chat.id, flt_num_response.to_string()).await?;
                        } else {
                            bot.send_message(msg.chat.id, format!("Could not find flight number: {:?}", &value)).await?;
                        }
                    }
                    _ => {
                        bot.send_message(msg.chat.id, format!("You said {}", text))
                            .await?;
                    }
                }
            }
            respond(())
        }
    })
    .await;
}


async fn fetch_metar(id: Option<&str>) -> Result<Vec<METARObject>, Box<dyn std::error::Error + Send + Sync>>{
    let url = {
        match id {
            Some(value) => format!("https://aviationweather.gov/api/data/metar?ids={}&format=json", value),
            None => "https://aviationweather.gov/api/data/metar?ids=KIND&format=json".into()
        }
    };

    let metar_object: Vec<METARObject> = reqwest::get(url)
        .await?
        .json()
        .await?;

    Ok(metar_object)
}

async fn search_flt_num(flt_num: Option<&str>, config: &Config) -> Result<FltObject, Box<dyn std::error::Error + Send + Sync>> {
    // Setup capabilities for headless mode in firefox

    println!("Searching flight number: {:?}", flt_num);

    let caps = match config.driver.as_str() {
        "geckodriver" => {
            json!({
                "moz:firefoxOptions": {
                    "args":["-headless"]
                }
            })
        }
        "chromium" => {
            json!({
                "goog:chromeOptions": {
                    "args": [
                        "--headless=new",
                        "--no-sandbox",
                        "--disable-dev-shm-usage"
                    ]
                }
            })
        }
        _ => {
            json!({
                "moz:firefoxOptions": {
                    "args":["-headless"]
                }
            })
        }
    };
        

    let client = match ClientBuilder::native()
        .capabilities(caps.as_object().unwrap().clone())
        .connect("http://localhost:4444")
        .await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect: {e:?}");
            return Err(e.into());
        }
    };

    println!("Client Started...");

    if flt_num.is_none() {
        println!("Cannot search for flt_num: None");
        return Err("Cannot search for flt_num: None".into())
    }

    let url = format!("https://www.flightaware.com/live/flight/{}", flt_num.unwrap());

    println!("URL: {}", url);
    client.goto(&url).await?;

    let mut times: Vec<String> = Vec::new();
    
    let headings = client
        .find_all(Locator::Css(".flightPageDataActualTimeText"))
        .await?;

    for heading in headings {
        times.push(heading.text().await?);
    }

    let flt_obj = FltObject::from_time_vec(times).add_flight_number(&flt_num.unwrap());

    Ok(flt_obj)
} 


async fn search_airport() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let caps = json!({
        "moz:firefoxOptions": {
            "args":["-headless"]
        }
    });

    let client = ClientBuilder::native()
        .capabilities(caps.as_object().unwrap().clone())
        .connect("http://localhost:4444")
        .await?;

    client.goto("https://www.flightaware.com").await?;

    let search_box = client
        .find(Locator::Css("[data-testid='search-input']"))
        .await?;

    search_box.send_keys("KIND").await?;
    search_box.send_keys("\u{E007}").await?;

    let title = client.title().await?;

    println!("{}", title);

    time::sleep(time::Duration::from_secs(5)).await;

    let table = client
        .wait()
        .for_element(Locator::Css("[data-type='arrivals']"))
        .await?;

    let rows = table
        .find_all(Locator::Css("tr"))
        .await?;

    for row in rows {
        let cells = row
            .find_all(Locator::Css("td"))
            .await?;

        for cell in cells {
            println!("{}", cell.text().await?);
        }

        println!("---");
    }

    client.close().await?;

    Ok(())
}