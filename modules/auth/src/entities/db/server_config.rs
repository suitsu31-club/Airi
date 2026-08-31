use time::PrimitiveDateTime;

pub struct ServerConfigEntity {
    pub id: i32,
    pub key: String,
    pub value: serde_json::Value,
    pub last_updated_at: PrimitiveDateTime,
}