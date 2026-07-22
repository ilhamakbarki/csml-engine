use crate::data::DynamoDbClient;
use crate::EngineError;
use rusoto_s3::{DeleteObjectRequest, GetObjectRequest, PutObjectRequest, S3};
use std::io::Read;

#[tracing::instrument(name = "db.dynamo.s3.put_object", skip_all, fields(db_system = "dynamodb", db_operation = "PutObject", s3_key = crate::utils::trunc(key), s3_size = content.len() as i64))]
pub fn put_object(db: &mut DynamoDbClient, key: &str, content: String) -> Result<(), EngineError> {
    let bucket = match std::env::var("AWS_S3_BUCKET") {
        Ok(bucket) => bucket,
        Err(_) => {
            return Err(EngineError::Manager(
                "Missing AWS_S3_BUCKET env var".to_owned(),
            ))
        }
    };

    let request = PutObjectRequest {
        bucket,
        key: key.to_owned(),
        content_type: Some("application/json".to_owned()),
        body: Some(content.into_bytes().into()),
        ..Default::default()
    };

    let future = db.s3_client.put_object(request);

    let _value = db.runtime.block_on(future)?;

    Ok(())
}

#[tracing::instrument(name = "db.dynamo.s3.get_object", skip_all, fields(db_system = "dynamodb", db_operation = "GetObject", s3_key = crate::utils::trunc(key)))]
pub fn get_object(db: &mut DynamoDbClient, key: &str) -> Result<String, EngineError> {
    let bucket = match std::env::var("AWS_S3_BUCKET") {
        Ok(bucket) => bucket,
        Err(_) => {
            return Err(EngineError::Manager(
                "Missing AWS_S3_BUCKET env var".to_owned(),
            ))
        }
    };

    let request = GetObjectRequest {
        bucket,
        key: key.to_owned(),
        ..Default::default()
    };

    let future = db.s3_client.get_object(request);

    let value = db.runtime.block_on(future)?;

    match value.body {
        Some(value) => {
            let mut value = value.into_blocking_read();
            let mut buffer = String::new();

            value.read_to_string(&mut buffer)?;

            Ok(buffer)
        }
        None => return Err(EngineError::Manager("empty object".to_owned())),
    }
}

#[tracing::instrument(level = "debug", name = "db.dynamo.s3.delete_object", skip_all, fields(db_system = "dynamodb", db_operation = "DeleteObject", s3_key = crate::utils::trunc(key)))]
pub fn delete_object(db: &mut DynamoDbClient, key: &str) -> Result<(), EngineError> {
    let bucket = match std::env::var("AWS_S3_BUCKET") {
        Ok(bucket) => bucket,
        Err(_) => {
            return Err(EngineError::Manager(
                "Missing AWS_S3_BUCKET env var".to_owned(),
            ))
        }
    };

    let request = DeleteObjectRequest {
        bucket,
        key: key.to_owned(),
        ..Default::default()
    };

    let future = db.s3_client.delete_object(request);

    db.runtime.block_on(future)?;

    Ok(())
}
