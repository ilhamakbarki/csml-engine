use diesel::{RunQueryDsl, ExpressionMethods, QueryDsl};

use crate::{
    EngineError, PostgresqlClient,
};

use super::{
    schema::{
        csml_conversations,
        csml_memories, csml_states
    }
};

#[tracing::instrument(name = "db.pg.setup.delete_expired", skip_all, fields(db_system = "postgresql", db_operation = "delete"))]
pub fn delete_expired_data(
    db: &PostgresqlClient,
) -> Result<(), EngineError> {
    let date_now = chrono::Utc::now().naive_utc();

    diesel::delete(
        csml_conversations::table
        .filter(csml_conversations::expires_at.lt(date_now))
    ).execute(&db.client).ok();

    diesel::delete(
        csml_memories::table
        .filter(csml_memories::expires_at.lt(date_now))
    ).execute(&db.client).ok();

    diesel::delete(
        csml_states::table
        .filter(csml_states::expires_at.lt(date_now))
    ).execute(&db.client).ok();

    Ok(())
}