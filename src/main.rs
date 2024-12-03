#![allow(clippy::upper_case_acronyms)]
mod cors;
mod model;
mod schema;

use crate::model::TDS;
use crate::schema::tds_readings::dsl::tds_readings;
use diesel::query_dsl::methods::{LimitDsl, OrderDsl};
use diesel::{Connection, ExpressionMethods, PgConnection, RunQueryDsl};
use dotenv::dotenv;
use rand::random;
use rocket::http::Status;
use rocket::response::status::Custom;
use rocket::serde::json::Json;
use rocket::{get, routes};
use rocket_sync_db_pools::{database, diesel};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use schema::tds_readings::timestamp;
use serde::Deserialize;
use std::env;
use std::time::Duration;

#[database("diesel_postgres_pool")]
pub struct Db(PgConnection);

#[derive(Deserialize)]
struct IncomingMessage {
    tds_value: String,
}

async fn handle_message(
    message: String,
    db_pool: &mut PgConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Received raw message: \"{message}\"");
    let parsed_message: Result<IncomingMessage, serde_json::Error> = serde_json::from_str(&message);

    match parsed_message {
        Ok(parsed) => {
            diesel::insert_into(tds_readings)
                .values(TDS {
                    id: random(),
                    tds_ppm: parsed.tds_value.parse().unwrap(),
                    timestamp: chrono::Local::now().timestamp(),
                })
                .execute(db_pool)?;
            println!("Inserted message: \"{}\" successfully", parsed.tds_value);
            Ok(())
        }
        Err(err) => {
            eprintln!("Failed to parse message as JSON: {}", err);
            Err(Box::new(err))
        }
    }
}

async fn run_mqtt_client(db_pool: &mut PgConnection) {
    let mut mqttoptions = MqttOptions::new("rocket_mqtt", "localhost", 1883);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    mqttoptions.set_credentials("tdsusr", "tdspass");

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    if let Err(err) = client.subscribe("tds/topic", QoS::AtMostOnce).await {
        eprintln!("Failed to subscribe to topic: {}", err);
        return;
    }

    while let Ok(event) = eventloop.poll().await {
        if let Event::Incoming(Packet::Publish(publish)) = event {
            if let Ok(message) = String::from_utf8(publish.payload.to_vec()) {
                if let Err(err) = handle_message(message, db_pool).await {
                    eprintln!("Error processing message: {}", err);
                }
            }
        }
    }
}

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

#[get("/last_message")]
async fn fetch_last_message(db: Db) -> Result<Json<TDS>, Custom<String>> {
    db.run(|conn| tds_readings.load::<TDS>(conn))
        .await
        .map(|messages| {
            Ok(if let Some(last_message) = messages.last() {
                Json(TDS {
                    id: last_message.id,
                    tds_ppm: last_message.tds_ppm,
                    timestamp: last_message.timestamp,
                })
            } else {
                return Err(Custom(
                    Status::InternalServerError,
                    format!("Failed to fetch message"),
                ));
            })
        })
        .map_err(|err| {
            Custom(
                Status::InternalServerError,
                format!("Failed to fetch message: {:?}", err),
            )
        })?
}

#[get("/tds_history")]
async fn fetch_tds_history(db: Db) -> Result<Json<Vec<TDS>>, Custom<String>> {
    db.run(|conn| {
        tds_readings
            .order(timestamp.desc())
            .limit(60)
            .load::<TDS>(conn)
    })
    .await
    .map(Json)
    .map_err(|err| {
        Custom(
            Status::InternalServerError,
            format!("Failed to fetch history: {:?}", err),
        )
    })
}

#[rocket::main]
async fn main() {
    tokio::spawn(async move {
        run_mqtt_client(&mut establish_connection()).await;
    });

    rocket::build()
        .attach(Db::fairing())
        .attach(cors::CORS)
        .mount("/", routes![fetch_last_message, fetch_tds_history])
        .launch()
        .await
        .expect("Failed to launch Rocket");
}
