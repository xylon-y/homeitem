//! 保养记录页面。

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use crate::api;

use common::dto::{MaintenanceCreate, MaintenanceUpdate};
use leptos::task::spawn_local;

type Ts = chrono::DateTime<chrono::Utc>;

/// 将 `<input type="date">` 的 "YYYY-MM-DD" 字符串解析为 UTC 时间。
fn parse_date(s: &str) -> Option<Ts> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc))
}

/// 将时间格式化为 date input 需要的 "YYYY-MM-DD"。
fn fmt_date(d: Option<Ts>) -> String {
    d.map(|dt| dt.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

#[component]
pub fn MaintenancePage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();

    // 物品名称（面包屑用）。
    let entity = LocalResource::new(move || {
        let iid = id();
        async move { api::entity(&iid).await.ok() }
    });

    // 状态筛选与记录列表。
    let status = RwSignal::new("all".to_string());
    let records = LocalResource::new(move || {
        let iid = id();
        let st = status.get();
        async move { api::maintenance(&iid, &st).await.ok() }
    });

    // 新建 / 编辑表单状态。
    let editing = RwSignal::new(None::<String>);
    let form_name = RwSignal::new(String::new());
    let form_desc = RwSignal::new(String::new());
    let form_cost = RwSignal::new(String::new());
    let form_completed = RwSignal::new(String::new());
    let form_scheduled = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let clear_form = move || {
        editing.set(None);
        form_name.set(String::new());
        form_desc.set(String::new());
        form_cost.set(String::new());
        form_completed.set(String::new());
        form_scheduled.set(String::new());
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let eid = id();
        let mid = editing.get();
        let name = form_name.get();
        let description = form_desc.get();
        let cost = form_cost.get().trim().parse::<f64>().unwrap_or(0.0);
        let completed_date = parse_date(&form_completed.get());
        let scheduled_date = parse_date(&form_scheduled.get());
        error.set(None);
        busy.set(true);
        spawn_local(async move {
            let result = match mid {
                Some(mid) => api::update_maintenance(
                    &eid,
                    &mid,
                    &MaintenanceUpdate {
                        name,
                        description,
                        cost,
                        completed_date,
                        scheduled_date,
                    },
                )
                .await
                .map(|_| ()),
                None => api::create_maintenance(
                    &eid,
                    &MaintenanceCreate {
                        name,
                        description,
                        cost,
                        completed_date,
                        scheduled_date,
                    },
                )
                .await
                .map(|_| ()),
            };
            busy.set(false);
            match result {
                Ok(_) => {
                    clear_form();
                    records.refetch();
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    view! {
        <div class="page">
            <nav class="breadcrumb">
                <a href="/items">"物品"</a>
                <span>" / "</span>
                <Suspense fallback={|| view! { <span class="muted">"…"</span> }}>
                    {move || Suspend::new(async move {
                        match entity.get() {
                            Some(Some(e)) => view! { <span>{e.summary.name}</span> }.into_any(),
                            _ => view! { <span class="muted">"（未知物品）"</span> }.into_any(),
                        }
                    })}
                </Suspense>
                <span>" / 保养记录"</span>
            </nav>

            <h1>"保养记录"</h1>

            <div class="toolbar">
                <label>
                    "状态："
                    <select on:change=move |ev| {
                        let v = event_target_value(&ev);
                        status.set(v);
                        records.refetch();
                    }>
                        <option value="all" prop:selected=move || status.get() == "all">"全部"</option>
                        <option value="scheduled" prop:selected=move || status.get() == "scheduled">"计划中"</option>
                        <option value="completed" prop:selected=move || status.get() == "completed">"已完成"</option>
                    </select>
                </label>
            </div>

            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match records.get() {
                        Some(Some(list)) if !list.is_empty() => view! {
                            <table class="items-table">
                                <thead>
                                    <tr>
                                        <th>"名称"</th>
                                        <th>"说明"</th>
                                        <th>"费用"</th>
                                        <th>"日期"</th>
                                        <th>"状态"</th>
                                        <th>"操作"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {list.into_iter().map(|m| {
                                        let mid = m.id.clone();
                                        let mid_del = mid.clone();
                                        let eid = id();
                                        let eid_del = eid.clone();
                                        let name = m.name.clone();
                                        let name_cell = name.clone();
                                        let desc = m.description.clone();
                                        let desc_cell = desc.clone();
                                        let cost = m.cost;
                                        let completed = m.completed_date;
                                        let scheduled = m.scheduled_date;
                                        let date = fmt_date(completed.or(scheduled));
                                        let (badge, badge_class) = if completed.is_some() {
                                            ("已完成", "badge done")
                                        } else {
                                            ("计划中", "badge pending")
                                        };
                                        view! {
                                            <tr>
                                                <td>{name_cell}</td>
                                                <td>{desc_cell}</td>
                                                <td>{format!("¥{cost:.2}")}</td>
                                                <td>{date}</td>
                                                <td><span class=badge_class>{badge}</span></td>
                                                <td>
                                                    <button type="button" class="link" on:click=move |_| {
                                                        editing.set(Some(mid.clone()));
                                                        form_name.set(name.clone());
                                                        form_desc.set(desc.clone());
                                                        form_cost.set(cost.to_string());
                                                        form_completed.set(fmt_date(completed));
                                                        form_scheduled.set(fmt_date(scheduled));
                                                        error.set(None);
                                                    }>"编辑"</button>
                                                    <button type="button" class="link danger" on:click=move |_| {
                                                        let mid = mid_del.clone();
                                                        let eid = eid_del.clone();
                                                        spawn_local(async move {
                                                            match api::delete_maintenance(&eid, &mid).await {
                                                                Ok(_) => records.refetch(),
                                                                Err(e) => error.set(Some(e.message)),
                                                            }
                                                        });
                                                    }>"删除"</button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any(),
                        Some(Some(_)) => view! { <p class="muted">"暂无保养记录。"</p> }.into_any(),
                        Some(None) => view! { <p class="error">"加载失败，请确认后端已启动。"</p> }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>

            <div class="card">
                <h2>{move || if editing.get().is_some() { "编辑保养记录" } else { "新建保养记录" }}</h2>
                <form on:submit=on_submit>
                    <label>
                        "名称"
                        <input type="text" bind:value=form_name required />
                    </label>
                    <label>
                        "说明"
                        <input type="text" bind:value=form_desc />
                    </label>
                    <label>
                        "费用"
                        <input type="number" step="0.01" bind:value=form_cost />
                    </label>
                    <label>
                        "完成日期"
                        <input type="date" bind:value=form_completed />
                    </label>
                    <label>
                        "计划日期"
                        <input type="date" bind:value=form_scheduled />
                    </label>
                    <Show when=move || error.get().is_some()>
                        <p class="error">{move || error.get()}</p>
                    </Show>
                    <div class="form-actions">
                        <button type="submit" disabled=move || busy.get()>
                            {move || if busy.get() { "保存中…" } else { "保存" }}
                        </button>
                        <Show when=move || editing.get().is_some()>
                            <button type="button" on:click=move |_| clear_form()>"取消编辑"</button>
                        </Show>
                    </div>
                </form>
            </div>
        </div>
    }
}
