use super::{ConsumerManifest, Options};
use crate::{api::ApiSnapshot, impact, repository};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

/**
 * Validate analysis bounds and normalize a unique explicit commit for each consumer.
 */
pub(super) fn validate(
    base: &ApiSnapshot,
    head: &ApiSnapshot,
    consumers: &mut ConsumerManifest,
    options: &Options,
) -> Result<()> {
    ensure!(
        (1..=impact::MAX_LIMIT).contains(&options.limit),
        "invalid display limit"
    );
    ensure!(
        options.max_index_age_days <= 36500,
        "index age threshold exceeds 36500 days"
    );
    ensure!(
        consumers.schema_version == 1 && consumers.consumers.len() <= 256,
        "consumer manifest requires version 1 and at most 256 repositories"
    );
    let mut ids = BTreeSet::new();
    for consumer in &mut consumers.consumers {
        repository::validate_id(&consumer.repository)?;
        repository::validate_commit(&consumer.commit)?;
        consumer.commit.make_ascii_lowercase();
        ensure!(
            ids.insert(consumer.repository.clone()),
            "select exactly one commit per consumer repository"
        );
        ensure!(
            ![
                &base.producer.repository,
                &base.declarations.repository,
                &head.producer.repository,
                &head.declarations.repository
            ]
            .contains(&&consumer.repository),
            "library index repositories cannot also be selected as consumers"
        );
    }
    consumers.consumers.sort();
    Ok(())
}
