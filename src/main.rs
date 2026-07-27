use std::fmt;
use teloxide::prelude::*;
use reqwest;
use serde::Deserialize;
use fantoccini::{ClientBuilder, Locator};
use tokio::time;
use serde_json::json;

#[derive(Deserialize, Debug)]
struct METARObject {
    rawOb: String,
}

#[derive(Debug, Default)]
struct FltObject {
    gate_departure_time: Option<String>,
    takeoff_time: Option<String>,
    landing_time: Option<String>,
    gate_arrival_time: Option<String>,
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
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let bot = Bot::from_env();

    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
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
                    if let Ok(flt_num_response) = search_flt_num(value).await {
                        bot.send_message(msg.chat.id, flt_num_response.gate_arrival_time).await?;
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

async fn search_flt_num(flt_num: Option<&str>) -> Result<FltObject, Box<dyn std::error::Error + Send + Sync>> {
    // Setup capabilities for headless mode in firefox

    let caps = json!({
        "moz:firefoxOptions": {
            "args":["-headless"]
        }
    });

    let client = ClientBuilder::native()
        .capabilities(caps.as_object().unwrap().clone())
        .connect("http://localhost:4444")
        .await?;

    if flt_num.is_none() {
        return Err("Cannot search for flt_num: None".into())
    }

    let url = format!("https://www.flightaware.com/live/flight/{}", flt_num.unwrap());

    client.goto(&url).await?;

    let mut times: Vec<String> = Vec::new();
    
    let headings = client
        .find_all(Locator::Css(".flightPageDataActualTimeText"))
        .await?;

    for heading in headings {
        times.push(heading.text().await?);
    }

    let flt_obj = FltObject::from_time_vec(times);

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