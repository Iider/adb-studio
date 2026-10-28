use crate::{App, engine, types::*};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Asia::Shanghai;
use std::sync::Arc;

pub fn next(
    rule: &ScheduleRule,
    after: DateTime<Utc>,
    anchor: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    match rule {
        ScheduleRule::Once { at } => {
            let t = DateTime::parse_from_rfc3339(at)?.with_timezone(&Utc);
            Ok((t > after).then_some(t))
        }
        ScheduleRule::Interval { minutes } => {
            if *minutes == 0 || *minutes > 525600 {
                bail!("间隔需为 1 至 525600 分钟")
            };
            let seconds = i64::from(*minutes) * 60;
            let n = ((after - anchor).num_seconds().max(0) / seconds) + 1;
            Ok(Some(anchor + Duration::seconds(n * seconds)))
        }
        ScheduleRule::Daily { time } | ScheduleRule::Weekly { time, .. } => {
            let time = NaiveTime::parse_from_str(time, "%H:%M").context("时间格式应为 HH:MM")?;
            if let ScheduleRule::Weekly { days, .. } = rule {
                if days.is_empty() || days.iter().any(|d| !(1..=7).contains(d)) {
                    bail!("请选择有效星期")
                }
            }
            let today = after.with_timezone(&Shanghai).date_naive();
            for n in 0..8 {
                let date = today + Duration::days(n);
                if let ScheduleRule::Weekly { days, .. } = rule {
                    if !days.contains(&date.weekday().number_from_monday()) {
                        continue;
                    }
                }
                let at = Shanghai
                    .from_local_datetime(&date.and_time(time))
                    .single()
                    .context("时间无效")?
                    .with_timezone(&Utc);
                if at > after {
                    return Ok(Some(at));
                }
            }
            bail!("未找到下次执行时间")
        }
    }
}

pub async fn monitor(app: Arc<App>) {
    let mut first = true;
    let mut cleanup_ticks = 0;
    loop {
        let result: Result<()> = async {
            let _gate = app.schedule_gate.lock().await;
            for mut s in app.db.list::<Schedule>("schedules").await? {
                if !s.enabled {
                    continue;
                }
                let due = DateTime::parse_from_rfc3339(&s.next_at)?.with_timezone(&Utc);
                let now = Utc::now();
                if due > now {
                    continue;
                }
                let anchor = DateTime::parse_from_rfc3339(&s.created_at)?.with_timezone(&Utc);
                let missed = first || (now - due).num_seconds() > 10;
                if let Some(at) = next(&s.rule, now, anchor)? {
                    s.next_at = at.to_rfc3339();
                } else {
                    s.enabled = false;
                }
                s.last_result = Some(if missed {
                    "错过时间，已跳过".into()
                } else {
                    "已触发".into()
                });
                // 先保存已消费的时间点，异常重启也不会再次派发。
                app.db.put("schedules", &s.id, &s).await?;
                if !missed {
                    let result = engine::workflow(&app, &s.workflow_id).await;
                    s.last_result = Some(match result {
                        Ok(w) => match engine::start(app.clone(), &s.device_id, w).await {
                            Ok(_) => "已开始".into(),
                            Err(e) => format!("已跳过：{e}"),
                        },
                        Err(e) => format!("已跳过：{e}"),
                    });
                    app.db.put("schedules", &s.id, &s).await?;
                }
                let _ = app.events.send(serde_json::json!({"type":"changed"}));
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!("定时检查：{e}")
        };
        first = false;
        cleanup_ticks += 1;
        if cleanup_ticks % 30 == 0 {
            if let Err(error) = crate::workflow_cleanup::retry(&app).await {
                tracing::warn!("任务删除后的截图清理：{error}");
            }
        }
        if cleanup_ticks >= 3600 {
            cleanup_ticks = 0;
            if let Err(e) = cleanup(&app).await {
                tracing::warn!("记录清理：{e}")
            }
        }
        tokio::select! {_=app.stop.cancelled()=>break,_=tokio::time::sleep(std::time::Duration::from_secs(1))=>{}}
    }
}
async fn cleanup(app: &App) -> Result<()> {
    let cutoff = Utc::now() - Duration::days(30);
    for r in app.db.list::<Run>("runs").await? {
        if r.busy() {
            continue;
        }
        if r.ended_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|at| at < cutoff)
        {
            for e in app
                .db
                .list::<serde_json::Value>("events")
                .await?
                .iter()
                .filter(|v| v["run_id"] == r.id)
            {
                if let Some(id) = e["id"].as_str() {
                    app.db.delete("events", id).await?;
                }
            }
            let _ = tokio::fs::remove_dir_all(app.data.join("shots").join(&r.id)).await;
            app.db.delete("runs", &r.id).await?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shanghai_daily() {
        let t = "2026-10-13T23:00:00Z".parse().unwrap();
        assert_eq!(
            next(
                &ScheduleRule::Daily {
                    time: "08:00".into()
                },
                t,
                t
            )
            .unwrap()
            .unwrap()
            .to_rfc3339(),
            "2026-10-14T00:00:00+00:00"
        );
    }
    #[test]
    fn weekly_monday() {
        let t = "2026-10-14T00:00:00Z".parse().unwrap();
        assert_eq!(
            next(
                &ScheduleRule::Weekly {
                    time: "09:00".into(),
                    days: vec![1]
                },
                t,
                t
            )
            .unwrap()
            .unwrap()
            .to_rfc3339(),
            "2026-10-19T01:00:00+00:00"
        );
    }
    #[test]
    fn interval_skips_backlog() {
        let a = "2026-10-14T00:00:00Z".parse().unwrap();
        let t = "2026-10-14T00:29:00Z".parse().unwrap();
        assert_eq!(
            next(&ScheduleRule::Interval { minutes: 10 }, t, a)
                .unwrap()
                .unwrap()
                .to_rfc3339(),
            "2026-10-14T00:30:00+00:00"
        );
    }
}
