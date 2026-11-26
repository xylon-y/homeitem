//! Background reminder scanner: emails for warranty, custom reminders,
//! scheduled maintenance and low stock.

use common::query::RemindersQuery;

use crate::config::Config;
use crate::db::repo::{notifiers, users};
use crate::db::{with_read, with_write, Pool};
use crate::error::AppResult;

/// Run one scan across all groups; returns how many notifications fired.
///
/// # Errors
/// Database errors are logged and skipped per group; the function itself
/// only fails on unrecoverable pool errors.
pub async fn run_scan(pool: &Pool, config: &Config) -> AppResult<usize> {
    let group_ids = list_group_ids(pool).await?;
    let mut sent = 0usize;
    for gid in group_ids {
        match scan_group(pool, config, &gid).await {
            Ok(n) => sent += n,
            Err(e) => tracing::warn!(group = %gid, error = %e, "reminder scan failed for group"),
        }
    }
    Ok(sent)
}

async fn list_group_ids(pool: &Pool) -> AppResult<Vec<String>> {
    with_read(pool, |conn| {
        crate::db::repo::query_all(conn, "SELECT id FROM groups", rusqlite::params![], |r| {
            crate::db::repo::text(r, 0)
        })
    })
    .await
}

async fn scan_group(pool: &Pool, config: &Config, group_id: &str) -> AppResult<usize> {
    let q = RemindersQuery { include_overdue: true, ..RemindersQuery::default() };
    let reminders = with_read(pool, {
        let gid = group_id.to_string();
        move |conn| notifiers::reminders(conn, &gid, &q)
    })
    .await?;
    if reminders.items.is_empty() {
        return Ok(0);
    }

    let active = with_read(pool, {
        let gid = group_id.to_string();
        move |conn| notifiers::list_notifiers(conn, &gid)
    })
    .await?
    .into_iter()
    .filter(|n| n.is_active)
    .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(0);
    }

    let recipients = with_read(pool, {
        let gid = group_id.to_string();
        move |conn| users::list_members_out(conn, &gid)
    })
    .await?
    .into_iter()
    .filter(|m| m.email.contains('@'))
    .map(|m| m.email)
    .collect::<Vec<_>>();

    let mailer = crate::mail::Mailer::from_config(config)?;
    let mut sent = 0usize;
    for item in &reminders.items {
        let dedupe = format!(
            "{}:{}:{}",
            serde_json::to_string(&item.kind).unwrap_or_default().trim_matches('"'),
            item.entity_id,
            item.date.format("%Y-%m-%d"),
        );
        for notifier in &active {
            if !notifier.kinds.contains(&item.kind) {
                continue;
            }
            let nid = notifier.id.clone();
            let key = dedupe.clone();
            let already = with_write(pool, move |conn| notifiers::already_sent(conn, &nid, &key)).await?;
            if already {
                continue;
            }
            let delivered = match (&notifier.kind, &mailer) {
                (common::dto::NotifierKind::Email, Some(m)) => {
                    let subject = format!("【homeitem】提醒: {}", item.entity_name);
                    let body = format!(
                        "{}\n\n物品: {}\n日期: {}\n备注: {}\n\n查看: {}/items/{}",
                        kind_label(&item.kind),
                        item.entity_name,
                        item.date.format("%Y-%m-%d"),
                        item.note,
                        config.public_base_url,
                        item.entity_id,
                    );
                    let mut ok_any = false;
                    for email in &recipients {
                        match m.send(email, &subject, &body).await {
                            Ok(()) => ok_any = true,
                            Err(e) => tracing::warn!(to = %email, error = %e, "reminder mail failed"),
                        }
                    }
                    ok_any
                }
                _ => false,
            };
            if delivered {
                let nid = notifier.id.clone();
                let key = dedupe.clone();
                with_write(pool, move |conn| notifiers::record_send(conn, &nid, &key)).await?;
                sent += 1;
            }
        }
    }
    Ok(sent)
}

/// Drop notification-log entries older than `days`.
pub async fn prune_log(pool: &Pool, days: i64) -> AppResult<usize> {
    with_write(pool, move |conn| notifiers::prune_log(conn, days)).await
}

fn kind_label(kind: &common::dto::ReminderKind) -> &'static str {
    match kind {
        common::dto::ReminderKind::Warranty => "保修到期",
        common::dto::ReminderKind::Reminder => "自定义提醒",
        common::dto::ReminderKind::Maintenance => "保养计划",
        common::dto::ReminderKind::LowStock => "库存不足",
    }
}

/// Spawn the periodic scanner task. Called once at startup.
pub fn spawn(pool: Pool, config: Config) {
    let interval = std::time::Duration::from_secs((config.reminder_interval_min.max(1) as u64) * 60);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ticks: u64 = 0;
        loop {
            ticker.tick().await;
            ticks += 1;
            match run_scan(&pool, &config).await {
                Ok(n) if n > 0 => tracing::info!(sent = n, "reminder scan dispatched"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "reminder scan error"),
            }
            if ticks % 168 == 0 {
                // weekly dedupe-log cleanup
                if let Err(e) = prune_log(&pool, 30).await {
                    tracing::warn!(error = %e, "notification log prune failed");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_labels_are_chinese() {
        assert_eq!(kind_label(&common::dto::ReminderKind::Warranty), "保修到期");
        assert_eq!(kind_label(&common::dto::ReminderKind::LowStock), "库存不足");
    }
}
