use crate::{
    encrypt::{decrypt_data, encrypt_data},
    EngineError, MongoDbClient,
};
use bson::{doc, Document};
use csml_interpreter::data::Client;

pub fn format_state_data(
    client: &Client,
    _type: &str,
    keys_values: Vec<(&str, &serde_json::Value)>,
    expires_at: Option<bson::DateTime>,
) -> Result<Vec<Document>, EngineError> {
    let client = bson::to_bson(client)?;

    keys_values.iter().fold(Ok(vec![]), |vec, (key, value)| {
        let time = bson::DateTime::from_chrono(chrono::Utc::now());

        let value = encrypt_data(value)?;
        let mut vec = vec?;

        vec.push(doc! {
            "client": client.clone(),
            "type": _type,
            "key": key,
            "value": value,
            "expires_at": expires_at,
            "created_at": time
        });
        Ok(vec)
    })
}

#[tracing::instrument(name = "db.mongo.state.delete_key", skip_all, fields(otel.kind = "client", db.system = "mongodb", db.operation = "delete_one", db.collection = "state", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id), state_type = crate::utils::trunc(_type), state_key = crate::utils::trunc(key)))]
pub fn delete_state_key(
    client: &Client,
    _type: &str,
    key: &str,
    db: &MongoDbClient,
) -> Result<(), EngineError> {
    let state = db.client.collection::<Document>("state");

    let filter = doc! {
        "client": bson::to_bson(client)?,
        "type": _type,
        "key": key,
    };
    state.delete_one(filter, None)?;

    Ok(())
}

#[tracing::instrument(name = "db.mongo.state.get_key", skip_all, fields(otel.kind = "client", db.system = "mongodb", db.operation = "find_one", db.collection = "state", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id), state_type = crate::utils::trunc(_type), state_key = crate::utils::trunc(key)))]
pub fn get_state_key(
    client: &Client,
    _type: &str,
    key: &str,
    db: &MongoDbClient,
) -> Result<Option<serde_json::Value>, EngineError> {
    let state = db.client.collection::<Document>("state");

    let filter = doc! {
        "client.bot_id": client.bot_id.to_owned(),
        "client.user_id": client.user_id.to_owned(),
        "client.channel_id": client.channel_id.to_owned(),
        "type": _type,
        "key": key,
    };

    match state.find_one(filter, None)? {
        Some(value) => {
            let state: serde_json::Value = bson::from_bson(bson::Bson::Document(value))?;
            let val = state["value"].as_str().unwrap().to_owned();
            Ok(Some(decrypt_data(val)?))
        }
        None => Ok(None),
    }
}

#[tracing::instrument(name = "db.mongo.state.get_current", skip_all, fields(otel.kind = "client", db.system = "mongodb", db.operation = "find_one", db.collection = "state", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id)))]
pub fn get_current_state(
    client: &Client,
    db: &MongoDbClient,
) -> Result<Option<serde_json::Value>, EngineError> {
    let state = db.client.collection::<Document>("state");

    let filter = doc! {
        "client.bot_id": client.bot_id.to_owned(),
        "client.user_id": client.user_id.to_owned(),
        "client.channel_id": client.channel_id.to_owned(),
        "type": "hold",
        "key": "position",
    };

    match state.find_one(filter, None)? {
        Some(doc) => {
            let state: serde_json::Value = bson::from_bson(bson::Bson::Document(doc))?;
            let value = state["value"].as_str().unwrap().to_owned();

            let current_state = serde_json::json!({
                "client": state["client"],
                "type": state["type"],
                "value": decrypt_data(value)?,
                "created_at": state["created_at"],
            });

            Ok(Some(current_state))
        }
        None => Ok(None),
    }
}

#[tracing::instrument(name = "db.mongo.state.set_items", skip_all, fields(otel.kind = "client", db.system = "mongodb", db.operation = "insert_many", db.collection = "state", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id), state_type = crate::utils::trunc(_type), db.batch_size = keys_values.len() as i64))]
pub fn set_state_items(
    client: &Client,
    _type: &str,
    keys_values: Vec<(&str, &serde_json::Value)>,
    expires_at: Option<bson::DateTime>,
    db: &MongoDbClient,
) -> Result<(), EngineError> {
    if keys_values.len() == 0 {
        return Ok(());
    }

    let state_data = format_state_data(client, _type, keys_values, expires_at)?;
    let state = db.client.collection::<Document>("state");
    state.insert_many(state_data, None)?;

    Ok(())
}

#[tracing::instrument(name = "db.mongo.state.delete_user", skip_all, fields(otel.kind = "client", db.system = "mongodb", db.operation = "delete_many", db.collection = "state", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id)))]
pub fn delete_user_state(client: &Client, db: &MongoDbClient) -> Result<(), EngineError> {
    let collection = db.client.collection::<Document>("state");

    let filter = doc! {
        "client.bot_id": client.bot_id.to_owned(),
        "client.user_id": client.user_id.to_owned(),
        "client.channel_id": client.channel_id.to_owned(),
    };

    collection.delete_many(filter, None)?;

    Ok(())
}
