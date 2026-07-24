use diesel::{RunQueryDsl, ExpressionMethods, QueryDsl};

use crate::{
    encrypt::{decrypt_data, encrypt_data},
    EngineError, SqliteClient,
    Client
};

use super::{
    models,
    schema::csml_states,
};
use chrono::{NaiveDateTime};

#[tracing::instrument(name = "db.sqlite.state.delete_key", skip_all, fields(otel.kind = "client", db.system = "sqlite", db.operation = "delete", db.collection = "csml_states", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id), state_type = crate::utils::trunc(type_), state_key = crate::utils::trunc(key)))]
pub fn delete_state_key(
    client: &Client,
    type_: &str,
    key: &str,
    db: &SqliteClient,
) -> Result<(), EngineError> {
    diesel::delete(csml_states::table
        .filter(csml_states::bot_id.eq(&client.bot_id))
        .filter(csml_states::channel_id.eq(&client.channel_id))
        .filter(csml_states::user_id.eq(&client.user_id))
        .filter(csml_states::type_.eq(type_))
        .filter(csml_states::key.eq(key))
    ).execute(&db.client)?;

    Ok(())
}

#[tracing::instrument(name = "db.sqlite.state.get_key", skip_all, fields(otel.kind = "client", db.system = "sqlite", db.operation = "select", db.collection = "csml_states", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id), state_type = crate::utils::trunc(type_), state_key = crate::utils::trunc(key)))]
pub fn get_state_key(
    client: &Client,
    type_: &str,
    key: &str,
    db: &SqliteClient,
) -> Result<Option<serde_json::Value>, EngineError> {
    let state: Result<models::State, diesel::result::Error> = csml_states::table
    .filter(csml_states::bot_id.eq(&client.bot_id))
    .filter(csml_states::channel_id.eq(&client.channel_id))
    .filter(csml_states::user_id.eq(&client.user_id))

    .filter(csml_states::type_.eq(type_))
    .filter(csml_states::key.eq(key))

    .get_result(&db.client);

    match state {
        Ok(state) => {
            let value = decrypt_data(state.value)?;
            Ok(Some(value))
        },
        Err(_err) => {
            Ok(None)
        }
    }
}

#[tracing::instrument(name = "db.sqlite.state.get_current", skip_all, fields(otel.kind = "client", db.system = "sqlite", db.operation = "select", db.collection = "csml_states", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id)))]
pub fn get_current_state(
    client: &Client,
    db: &SqliteClient,
) -> Result<Option<serde_json::Value>, EngineError> {

    let current_state: models::State = csml_states::table
        .filter(csml_states::bot_id.eq(&client.bot_id))
        .filter(csml_states::channel_id.eq(&client.channel_id))
        .filter(csml_states::user_id.eq(&client.user_id))

        .filter(csml_states::type_.eq("hold"))
        .filter(csml_states::key.eq("position"))

        .get_result(&db.client)?;

    let current_state = serde_json::json!({
        "client": {
            "bot_id": current_state.bot_id,
            "channel_id": current_state.channel_id,
            "user_id": current_state.user_id
        },
        "type": current_state.type_,
        "value": decrypt_data(current_state.value)?,
        "created_at": current_state.created_at.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string(),
    });

    Ok(Some(current_state))
}

#[tracing::instrument(name = "db.sqlite.state.set_items", skip_all, fields(otel.kind = "client", db.system = "sqlite", db.operation = "insert", db.collection = "csml_states", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id), state_type = crate::utils::trunc(type_), db.batch_size = keys_values.len() as i64))]
pub fn set_state_items(
    client: &Client,
    type_: &str,
    keys_values: Vec<(&str, &serde_json::Value)>,
    expires_at: Option<NaiveDateTime>,
    db: &SqliteClient,
) -> Result<(), EngineError> {
    if keys_values.len() == 0 {
        return Ok(());
    }

    let mut new_states = vec!();
    for (key, value) in keys_values.iter() {

        let value = encrypt_data(value)?;

        let mem = models::NewState {
            id: models::UUID::new_v4(),

            bot_id: &client.bot_id,
            channel_id: &client.channel_id,
            user_id: &client.user_id,
            type_,
            key,
            value,
            expires_at,
        };

        new_states.push(mem);
    }

    diesel::insert_into(csml_states::table)
    .values(&new_states)
    .execute(&db.client)?;

    Ok(())
}

#[tracing::instrument(name = "db.sqlite.state.delete_user", skip_all, fields(otel.kind = "client", db.system = "sqlite", db.operation = "delete", db.collection = "csml_states", bot_id = crate::utils::trunc(&client.bot_id), channel_id = crate::utils::trunc(&client.channel_id)))]
pub fn delete_user_state(
    client: &Client,
    db: &SqliteClient
) -> Result<(), EngineError> {
    diesel::delete(csml_states::table
        .filter(csml_states::bot_id.eq(&client.bot_id))
        .filter(csml_states::channel_id.eq(&client.channel_id))
        .filter(csml_states::user_id.eq(&client.user_id))
    ).execute(&db.client).ok();

    Ok(())
}

#[tracing::instrument(name = "db.sqlite.state.delete_all_bot_data", skip_all, fields(otel.kind = "client", db.system = "sqlite", db.operation = "delete", db.collection = "csml_states", bot_id = crate::utils::trunc(bot_id)))]
pub fn delete_all_bot_data(
    bot_id: &str,
    db: &SqliteClient,
) -> Result<(), EngineError> {
    diesel::delete(
        csml_states::table
        .filter(csml_states::bot_id.eq(bot_id))
    ).execute(&db.client).ok();

    Ok(())
}