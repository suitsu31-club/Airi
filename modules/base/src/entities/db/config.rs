use sqlx::PgConnection;
use sqlx::types::Json;
use std::marker::PhantomData;
use time::PrimitiveDateTime;
use wakuwaku::entity::Query;
use wakuwaku::entity::sqlx::backend::PgSource;

#[derive(Debug, Clone)]
pub struct ModuleConfigEntity {
    pub key: String,
    pub value: Json<serde_json::Value>,
    pub last_updated_at: PrimitiveDateTime,
}

impl wakuwaku::entity::Entity for ModuleConfigEntity {}

pub trait ModuleConfig: serde::Serialize + for<'de> serde::Deserialize<'de> + Default {
    const CONFIG_PREFIX: &'static str;
    const CONFIG_KEY: &'static str;
    fn full_config_key() -> String {
        format!("{}/{}", Self::CONFIG_PREFIX, Self::CONFIG_KEY)
    }
}

pub struct FindModuleConfig<T: ModuleConfig>(PhantomData<fn() -> T>);

impl<T: ModuleConfig> Query for FindModuleConfig<T> {
    type Effect = wakuwaku::entity::Read<ModuleConfigEntity>;
}

impl<T: ModuleConfig + Send> wakuwaku::entity::Execute<PgSource> for FindModuleConfig<T> {
    type Output = T;
    type Error = sqlx::Error;

    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let key = T::full_config_key();
        todo!()
    }
}
