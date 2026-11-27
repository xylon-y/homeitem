//! 提醒与通知器页面。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use common::dto::{NotifierCreate, NotifierKind, NotifierUpdate, ReminderKind};

/// 提醒汇总页：保修到期 / 自定义提醒 / 保养计划 / 库存不足。
#[component]
pub fn RemindersPage() -> impl IntoView {
    let kind_filter = RwSignal::new(String::new());
    let results = LocalResource::new(move || {
        let kind = kind_filter.get();
        async move {
            let q = if kind.is_empty() {
                String::new()
            } else {
                format!("kinds={kind}")
            };
            api::reminders(&q).await
        }
    });

    view! {
        <div class="page">
            <h1>"提醒"</h1>
            <p class="muted">"保修到期、自定义提醒、保养计划与库存不足的汇总。"</p>

            <div class="filter-bar">
                <label for="reminder-kind">"类型："</label>
                <select
                    id="reminder-kind"
                    on:change=move |ev| {
                        kind_filter.set(event_target_value(&ev));
                        results.refetch();
                    }
                >
                    <option value="">"全部"</option>
                    <option value="warranty">"保修到期"</option>
                    <option value="reminder">"自定义提醒"</option>
                    <option value="maintenance">"保养计划"</option>
                    <option value="lowStock">"库存不足"</option>
                </select>
            </div>

            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match results.get() {
                        Some(Ok(resp)) => {
                            let counts = resp.counts;
                            let rows = resp.items;
                            view! {
                                <div class="stat-grid">
                                    <div class="stat-card">
                                        <div class="stat-num">{counts.overdue}</div>
                                        <div class="stat-label">"已逾期"</div>
                                    </div>
                                    <div class="stat-card">
                                        <div class="stat-num">{counts.due_soon}</div>
                                        <div class="stat-label">"即将到期"</div>
                                    </div>
                                    <div class="stat-card">
                                        <div class="stat-num">{counts.total}</div>
                                        <div class="stat-label">"提醒总数"</div>
                                    </div>
                                </div>

                                {if rows.is_empty() {
                                    view! { <p class="muted">"当前没有提醒。"</p> }.into_any()
                                } else {
                                    view! {
                                        <table class="items-table">
                                            <thead>
                                                <tr>
                                                    <th>"类型"</th>
                                                    <th>"物品"</th>
                                                    <th>"日期"</th>
                                                    <th>"剩余天数"</th>
                                                    <th>"状态"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {rows.into_iter().map(|item| {
                                                    let label = reminder_kind_label(item.kind);
                                                    let name = item.entity_name.clone();
                                                    let id = item.entity_id.clone();
                                                    let date = item.date.format("%Y-%m-%d").to_string();
                                                    let days = item.days_remaining;
                                                    let days_text = if days < 0 {
                                                        format!("逾期 {} 天", -days)
                                                    } else if days == 0 {
                                                        "今天".to_string()
                                                    } else {
                                                        format!("{} 天后", days)
                                                    };
                                                    let overdue = item.overdue;
                                                    view! {
                                                        <tr>
                                                            <td>{label}</td>
                                                            <td><a href={format!("/items/{id}")}>{name}</a></td>
                                                            <td>{date}</td>
                                                            <td>{days_text}</td>
                                                            <td>
                                                                {overdue.then(|| view! {
                                                                    <span class="badge badge-danger">"已逾期"</span>
                                                                })}
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any()
                                }}
                            }.into_any()
                        }
                        Some(Err(e)) => view! {
                            <p class="error">{format!("加载失败：{}", e.message)}</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>
        </div>
    }
}

/// 通知器管理页：列表 + 新建/编辑表单 + 删除与测试发送。
#[component]
pub fn NotifiersPage() -> impl IntoView {
    let list = LocalResource::new(move || async move { api::notifiers().await });

    let name = RwSignal::new(String::new());
    let kind = RwSignal::new(String::from("email"));
    let target = RwSignal::new(String::new());
    let is_active = RwSignal::new(true);
    let kw = RwSignal::new(false);
    let kr = RwSignal::new(false);
    let km = RwSignal::new(false);
    let kl = RwSignal::new(false);
    let editing_id: RwSignal<Option<String>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let notice: RwSignal<Option<String>> = RwSignal::new(None);

    let set_error = move |m: String| {
        notice.set(None);
        error.set(Some(m));
    };
    let set_notice = move |m: String| {
        error.set(None);
        notice.set(Some(m));
    };
    let reset_form = move || {
        editing_id.set(None);
        name.set(String::new());
        kind.set("email".to_string());
        target.set(String::new());
        is_active.set(true);
        kw.set(false);
        kr.set(false);
        km.set(false);
        kl.set(false);
    };

    view! {
        <div class="page">
            <h1>"通知器"</h1>

            {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
            {move || notice.get().map(|m| view! { <p class="notice">{m}</p> })}

            <h2>{move || if editing_id.get().is_some() { "编辑通知器" } else { "新建通知器" }}</h2>
            <form class="form-card" on:submit=move |ev| {
                ev.prevent_default();
                let n = name.get();
                let t = target.get();
                let k = if kind.get() == "webhook" { NotifierKind::Webhook } else { NotifierKind::Email };
                let active = is_active.get();
                let mut ks = Vec::new();
                if kw.get() { ks.push(ReminderKind::Warranty); }
                if kr.get() { ks.push(ReminderKind::Reminder); }
                if km.get() { ks.push(ReminderKind::Maintenance); }
                if kl.get() { ks.push(ReminderKind::LowStock); }
                let editing = editing_id.get();
                spawn_local(async move {
                    let result = match &editing {
                        Some(id) => api::update_notifier(id, &NotifierUpdate {
                            name: n, kind: k, target: t, is_active: active, kinds: ks,
                        }).await.map(|_| ()),
                        None => api::create_notifier(&NotifierCreate {
                            name: n, kind: k, target: t, is_active: active, kinds: ks,
                        }).await.map(|_| ()),
                    };
                    match result {
                        Ok(()) => {
                            reset_form();
                            set_notice("保存成功".to_string());
                            list.refetch();
                        }
                        Err(e) => set_error(format!("保存失败：{}", e.message)),
                    }
                });
            }>
                <label class="field">
                    <span>"名称"</span>
                    <input type="text" bind:value=name required />
                </label>
                <label class="field">
                    <span>"类型"</span>
                    <select bind:value=kind>
                        <option value="email">"邮件"</option>
                        <option value="webhook">"Webhook"</option>
                    </select>
                </label>
                <label class="field">
                    <span>"目标（邮箱地址或 URL）"</span>
                    <input type="text" bind:value=target required />
                </label>
                <label class="field checkbox">
                    <input type="checkbox" bind:checked=is_active />
                    <span>"启用"</span>
                </label>
                <fieldset class="field kinds">
                    <legend>"订阅的提醒类型"</legend>
                    <label><input type="checkbox" bind:checked=kw /> "保修到期"</label>
                    <label><input type="checkbox" bind:checked=kr /> "自定义提醒"</label>
                    <label><input type="checkbox" bind:checked=km /> "保养计划"</label>
                    <label><input type="checkbox" bind:checked=kl /> "库存不足"</label>
                </fieldset>
                <div class="form-actions">
                    <button type="submit">"保存"</button>
                    <button type="button" on:click=move |_| {
                        reset_form();
                        error.set(None);
                        notice.set(None);
                    }>"取消"</button>
                </div>
            </form>

            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match list.get() {
                        Some(Ok(items)) if !items.is_empty() => view! {
                            <table class="items-table">
                                <thead>
                                    <tr>
                                        <th>"名称"</th>
                                        <th>"类型"</th>
                                        <th>"目标"</th>
                                        <th>"状态"</th>
                                        <th>"订阅类型"</th>
                                        <th>"操作"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|n| {
                                        let edit_id = n.id.clone();
                                        let del_id = n.id.clone();
                                        let test_id = n.id.clone();
                                        let nm = n.name.clone();
                                        let tg = n.target.clone();
                                        let kd = n.kind;
                                        let ac = n.is_active;
                                        let ks = n.kinds.clone();
                                        view! {
                                            <tr>
                                                <td>{n.name.clone()}</td>
                                                <td>{notifier_kind_label(n.kind)}</td>
                                                <td>{n.target.clone()}</td>
                                                <td>{if n.is_active { "启用" } else { "停用" }}</td>
                                                <td>{n.kinds.iter().map(|k| reminder_kind_label(*k)).collect::<Vec<_>>().join("，")}</td>
                                                <td class="actions">
                                                    <button type="button" on:click=move |_| {
                                                        editing_id.set(Some(edit_id.clone()));
                                                        name.set(nm.clone());
                                                        kind.set(if kd == NotifierKind::Email { "email".to_string() } else { "webhook".to_string() });
                                                        target.set(tg.clone());
                                                        is_active.set(ac);
                                                        kw.set(ks.contains(&ReminderKind::Warranty));
                                                        kr.set(ks.contains(&ReminderKind::Reminder));
                                                        km.set(ks.contains(&ReminderKind::Maintenance));
                                                        kl.set(ks.contains(&ReminderKind::LowStock));
                                                        error.set(None);
                                                        notice.set(None);
                                                    }>"编辑"</button>
                                                    <button type="button" class="btn-danger" on:click=move |_| {
                                                        let id = del_id.clone();
                                                        spawn_local(async move {
                                                            match api::delete_notifier(&id).await {
                                                                Ok(()) => {
                                                                    set_notice("删除成功".to_string());
                                                                    list.refetch();
                                                                }
                                                                Err(e) => set_error(format!("删除失败：{}", e.message)),
                                                            }
                                                        });
                                                    }>"删除"</button>
                                                    <button type="button" on:click=move |_| {
                                                        let id = test_id.clone();
                                                        spawn_local(async move {
                                                            match api::test_notifier(&id, "").await {
                                                                Ok(()) => set_notice("测试发送成功".to_string()),
                                                                Err(e) => set_error(format!("测试发送失败：{}", e.message)),
                                                            }
                                                        });
                                                    }>"测试发送"</button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any(),
                        Some(Ok(_)) => view! { <p class="muted">"还没有通知器。"</p> }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error">{format!("加载失败：{}", e.message)}</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>
        </div>
    }
}

fn reminder_kind_label(kind: ReminderKind) -> &'static str {
    match kind {
        ReminderKind::Warranty => "保修到期",
        ReminderKind::Reminder => "自定义提醒",
        ReminderKind::Maintenance => "保养计划",
        ReminderKind::LowStock => "库存不足",
    }
}

fn notifier_kind_label(kind: NotifierKind) -> &'static str {
    match kind {
        NotifierKind::Email => "email",
        NotifierKind::Webhook => "webhook",
    }
}
