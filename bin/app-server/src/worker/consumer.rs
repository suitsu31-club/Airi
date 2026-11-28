//! The Iggy consumer worker: drives each module's event hooks.
//!
//! One consumer per topic: every handler of a topic is registered in the
//! same `iggy_consumer` chain, whose consumer is named after the topic.
//!
//! A topic's partitions are split between the consumer replicas
//! (`CONSUMER_SHARD` of `CONSUMER_SHARDS`), so each partition is consumed by
//! exactly one replica and its offsets are stored under one consumer name.

use super::Services;
use crate::dependencies::Dependencies;
use crate::settings::Settings;
use anyhow::{Context, bail};
use mailer::hooks::delivery::DeliverQueuedMail;
use wakuwaku::deps::iggy::prelude::{Identifier, IggyClient, TopicClient};
use wakuwaku::iggy::{ConsumerConfig, ConsumerRuntime, Event, PartitionKey};

/// Start one consumer per topic that has partitions for this replica.
pub async fn start(
    services: &Services,
    deps: &Dependencies,
    settings: &Settings,
) -> anyhow::Result<Vec<ConsumerRuntime>> {
    let topics = Topics::new(&deps.iggy, settings)?;
    let mut runtimes = Vec::new();

    // mailer
    if let Some(config) = topics.config::<mailer::events::outbound::EmailQueued>().await? {
        let (_, runtime) = services
            .clone()
            .iggy_consumer::<DeliverQueuedMail, _, _>()
            .start(deps.iggy.clone(), config)
            .await?;
        runtimes.push(runtime);
    }

    tracing::info!(consumers = runtimes.len(), "consumer worker started");
    Ok(runtimes)
}

/// Looks up the partitions of a topic that fall to this replica.
struct Topics<'a> {
    client: &'a IggyClient,
    stream: &'a str,
    shard: u32,
    shards: u32,
}

impl<'a> Topics<'a> {
    fn new(client: &'a IggyClient, settings: &'a Settings) -> anyhow::Result<Self> {
        let (shard, shards) = (settings.consumer_shard, settings.consumer_shards);
        if shard >= shards {
            bail!("CONSUMER_SHARD ({shard}) must be less than CONSUMER_SHARDS ({shards})");
        }
        Ok(Self {
            client,
            stream: &settings.iggy_stream,
            shard,
            shards,
        })
    }

    /// The configuration of the consumer of `E`'s topic, or `None` when no
    /// partition of it falls to this replica.
    async fn config<E: Event>(&self) -> anyhow::Result<Option<ConsumerConfig>> {
        let topic = <E::Key as PartitionKey>::SUPER_PARTITION_NAME.0;
        let details = self
            .client
            .get_topic(&Identifier::named(self.stream)?, &Identifier::named(topic)?)
            .await
            .with_context(|| format!("reading Iggy topic {}/{topic}", self.stream))?
            .with_context(|| {
                format!(
                    "Iggy topic {}/{topic} does not exist; provision it with manage-tool",
                    self.stream
                )
            })?;
        let partitions: Vec<u32> = details
            .partitions
            .iter()
            .map(|partition| partition.id)
            .filter(|id| id.checked_rem(self.shards) == Some(self.shard))
            .collect();
        if partitions.is_empty() {
            tracing::warn!(topic, "no partition of this topic falls to this consumer shard");
            return Ok(None);
        }
        tracing::info!(topic, ?partitions, "consuming");
        Ok(Some(ConsumerConfig::new(self.stream, topic, partitions)))
    }
}
