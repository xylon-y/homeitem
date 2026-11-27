//! 物品列表 / 位置 / 表单 / 详情页。

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use wasm_bindgen_futures::JsFuture;

use common::dto::{EntityCreate, EntityOut, EntityPatch, EntityUpdate, ItemAttachment, TreeItem};

use crate::api;

type Ts = chrono::DateTime<chrono::Utc>;

/// 将 `<input type="date">` 的 "YYYY-MM-DD" 解析为 UTC 时间。
fn parse_date(s: &str) -> Option<Ts> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc))
}

/// 时间格式化为 date input 需要的 "YYYY-MM-DD"。
fn fmt_date(d: Option<Ts>) -> String {
    d.map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// 浏览器整页跳转。
fn goto(path: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href(path);
    }
}

/// 扁平化位置树，返回 (id, 带缩进的名称)。
fn flatten_locations(items: &[TreeItem], depth: usize, out: &mut Vec<(String, String)>) {
    for item in items {
        out.push((item.id.clone(), format!("{}{}", "　".repeat(depth), item.name)));
        flatten_locations(&item.children, depth + 1, out);
    }
}

/// 物品列表页（搜索 + 分页）。
#[component]
pub fn ItemsPage() -> impl IntoView {
    let query = RwSignal::new(String::new());
    let page = RwSignal::new(1i64);
    let results = LocalResource::new(move || {
        let q = query.get();
        let p = page.get();
        async move {
            let mut qs = format!("page={p}&pageSize=25");
            if !q.trim().is_empty() {
                qs.push_str(&format!("&q={}", urlencoding_lite(&q)));
            }
            api::entities(&qs).await.ok()
        }
    });

    view! {
        <div class="page">
            <h1>"物品"</h1>
            <form class="search-bar" on:submit=move |ev| {
                ev.prevent_default();
                page.set(1);
                results.refetch();
            }>
                <input type="search" placeholder="搜索名称、序列号、备注…" bind:value=query />
                <button type="submit">"搜索"</button>
            </form>

            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match results.get() {
                        Some(Some(page_data)) => {
                            let total = page_data.total;
                            let cur = page_data.page;
                            // 空结果时也保证至少有一页，分页控件始终可见，避免越界后无法返回。
                            let pages = ((total + 24) / 25).max(1);
                            let body = if page_data.items.is_empty() {
                                view! { <p class="muted">"没有找到物品。"</p> }.into_any()
                            } else {
                                view! {
                                    <table class="items-table">
                                        <thead>
                                            <tr>
                                                <th>"名称"</th>
                                                <th>"数量"</th>
                                                <th>"价格"</th>
                                                <th>"位置"</th>
                                                <th>"更新时间"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {page_data.items.into_iter().map(|it| {
                                                let name = it.name.clone();
                                                let qty = it.quantity;
                                                let price = it.purchase_price;
                                                let loc = it.location.as_ref().map(|l| l.name.clone()).unwrap_or_default();
                                                let upd = it.updated_at.format("%Y-%m-%d").to_string();
                                                view! {
                                                    <tr>
                                                        <td>{name}</td>
                                                        <td>{qty}</td>
                                                        <td>{format!("¥{price:.2}")}</td>
                                                        <td>{loc}</td>
                                                        <td>{upd}</td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            };
                            view! {
                                {body}
                                <div class="pager">
                                    <button disabled={cur <= 1} on:click=move |_| page.update(|p| *p = (*p - 1).max(1))>"上一页"</button>
                                    <span>{format!("第 {cur} / {pages} 页，共 {total} 件")}</span>
                                    <button disabled={cur >= pages} on:click=move |_| page.update(|p| *p += 1)>"下一页"</button>
                                </div>
                            }.into_any()
                        }
                        Some(None) => view! { <p class="error">"加载失败，请确认后端已启动。"</p> }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>
        </div>
    }
}

/// 位置页：展示树形视图。
#[component]
pub fn LocationsPage() -> impl IntoView {
    let tree = LocalResource::new(move || async move { api::locations_tree().await.ok() });

    view! {
        <div class="page">
            <h1>"位置"</h1>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match tree.get() {
                        Some(Some(items)) if !items.is_empty() => view! {
                            <ul class="tree">
                                {items.into_iter().map(|t| render_loc(&t)).collect_view()}
                            </ul>
                        }.into_any(),
                        _ => view! { <p class="muted">"还没有位置。"</p> }.into_any(),
                    }
                })}
            </Suspense>
        </div>
    }
}

fn render_loc(item: &common::dto::TreeItem) -> AnyView {
    let count = item.item_count;
    view! {
        <li>
            <strong>{item.name.clone()}</strong>
            <span class="muted">{format!(" ({count} 件物品)")}</span>
            {(!item.children.is_empty()).then(|| {
                let kids = item.children.clone();
                view! {
                    <ul class="tree">
                        {kids.iter().map(|k| render_loc(k)).collect_view()}
                    </ul>
                }
            })}
        </li>
    }.into_any()
}

/// 轻量 URL 编码（避免额外依赖）。
fn urlencoding_lite(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// 物品新建 / 编辑页。
#[component]
pub fn ItemFormPage() -> impl IntoView {
    let params = use_params_map();
    // 路由带 id 即编辑，否则新建。
    let id = move || params.get().get("id");

    // 编辑模式下加载原实体（新建时直接返回 None）。
    let entity_res = LocalResource::new(move || {
        let iid = id().unwrap_or_default();
        async move {
            if iid.is_empty() {
                None
            } else {
                api::entity(&iid).await.ok()
            }
        }
    });
    let types = LocalResource::new(move || api::entity_types());
    let locations = LocalResource::new(move || api::locations_tree());

    // 表单信号：数字字段用 String 信号 + parse()。
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let entity_type_id = RwSignal::new(String::new());
    let location_id = RwSignal::new(String::new());
    let quantity = RwSignal::new("1".to_string());
    let min_quantity = RwSignal::new("0".to_string());
    let manufacturer = RwSignal::new(String::new());
    let model_number = RwSignal::new(String::new());
    let serial_number = RwSignal::new(String::new());
    let purchase_price = RwSignal::new("0".to_string());
    let purchase_date = RwSignal::new(String::new());
    let purchase_from = RwSignal::new(String::new());
    let warranty_expires = RwSignal::new(String::new());
    let notes = RwSignal::new(String::new());
    // 编辑时保留原实体，提交时复制表单未覆盖的全部字段。
    let loaded = RwSignal::new(None::<EntityOut>);
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    // 实体加载完成后预填表单。
    Effect::new(move |_| {
        if let Some(Some(e)) = entity_res.get() {
            name.set(e.summary.name.clone());
            description.set(e.summary.description.clone());
            entity_type_id.set(
                e.summary
                    .entity_type
                    .as_ref()
                    .map(|t| t.id.clone())
                    .unwrap_or_default(),
            );
            location_id.set(e.summary.location_id.clone().unwrap_or_default());
            quantity.set(e.summary.quantity.to_string());
            min_quantity.set(e.summary.min_quantity.to_string());
            purchase_price.set(e.summary.purchase_price.to_string());
            purchase_date.set(fmt_date(e.purchase_date));
            warranty_expires.set(fmt_date(e.summary.warranty_expires));
            manufacturer.set(e.manufacturer.clone());
            model_number.set(e.model_number.clone());
            serial_number.set(e.serial_number.clone());
            purchase_from.set(e.purchase_from.clone());
            notes.set(e.notes.clone());
            loaded.set(Some(e));
        }
    });

    let on_submit = move |ev: ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get().trim().to_string();
        if n.is_empty() {
            error.set(Some("名称不能为空".to_string()));
            return;
        }
        let et = entity_type_id.get();
        if et.is_empty() {
            error.set(Some("请选择物品类型".to_string()));
            return;
        }
        let d = description.get();
        let loc = location_id.get();
        let location = if loc.is_empty() { None } else { Some(loc) };
        let qty = quantity.get().trim().parse::<f64>().unwrap_or(1.0);
        let min_qty = min_quantity.get().trim().parse::<f64>().unwrap_or(0.0);
        let price = purchase_price.get().trim().parse::<f64>().unwrap_or(0.0);
        let pd = parse_date(&purchase_date.get());
        let we = parse_date(&warranty_expires.get());
        let man = manufacturer.get();
        let model = model_number.get();
        let serial = serial_number.get();
        let from = purchase_from.get();
        let note = notes.get();
        let edit_id = id();
        let base = loaded.get();
        error.set(None);
        busy.set(true);

        spawn_local(async move {
            let result = match (edit_id, base) {
                (Some(iid), Some(e)) => {
                    let parent_id = e.summary.parent.as_ref().map(|p| p.id.clone());
                    let tag_ids = e.summary.tags.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
                    let req = EntityUpdate {
                        name: n,
                        description: d,
                        entity_type_id: et,
                        parent_id,
                        location_id: location,
                        quantity: qty,
                        min_quantity: min_qty,
                        tag_ids,
                        archived: e.summary.archived,
                        insured: e.summary.insured,
                        asset_id: e.summary.asset_id,
                        manufacturer: man,
                        model_number: model,
                        serial_number: serial,
                        notes: note,
                        lifetime_warranty: e.lifetime_warranty,
                        warranty_expires: we,
                        warranty_details: e.warranty_details,
                        purchase_date: pd,
                        purchase_from: from,
                        purchase_price: price,
                        sold_date: e.summary.sold_date,
                        sold_to: e.sold_to,
                        sold_price: e.summary.sold_price,
                        sold_notes: e.sold_notes,
                        reminder_date: e.summary.reminder_date,
                        reminder_note: e.reminder_note,
                        sync_child_entity_locations: e.sync_child_entity_locations,
                        fields: e.fields,
                    };
                    api::update_entity(&iid, &serde_json::to_value(req).unwrap())
                        .await
                        .map(|out| out.summary.id)
                }
                (Some(_), None) => Err(api::ApiError {
                    code: "ERROR".into(),
                    message: "物品数据尚未加载完成，请稍后重试".into(),
                }),
                (None, _) => {
                    let req = EntityCreate {
                        name: n,
                        description: d,
                        entity_type_id: et,
                        parent_id: None,
                        location_id: location,
                        quantity: qty,
                        min_quantity: min_qty,
                        tag_ids: Vec::new(),
                        manufacturer: man,
                        model_number: model,
                        serial_number: serial,
                        notes: note,
                        purchase_price: price,
                        purchase_date: pd,
                        purchase_from: from,
                        warranty_expires: we,
                    };
                    api::create_entity(&req).await.map(|out| out.summary.id)
                }
            };
            busy.set(false);
            match result {
                Ok(new_id) => goto(&format!("/items/{new_id}")),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    view! {
        <div class="page">
            <h1>{move || if id().is_some() { "编辑物品" } else { "新建物品" }}</h1>
            <section class="card">
                <form class="form" on:submit=on_submit>
                    <label>
                        "名称"
                        <input type="text" required bind:value=name />
                    </label>
                    <label>
                        "描述"
                        <input type="text" bind:value=description />
                    </label>
                    <label>
                        "类型"
                        <Suspense fallback=|| view! { <select disabled><option>"加载中…"</option></select> }>
                            {move || Suspend::new(async move {
                                let opts = match types.get() {
                                    Some(Ok(v)) => v,
                                    _ => Vec::new(),
                                };
                                view! {
                                    <select on:change=move |ev: ev::Event| entity_type_id.set(event_target_value(&ev))>
                                        <option value="" prop:selected=move || entity_type_id.get().is_empty()>"请选择类型"</option>
                                        {opts.into_iter().map(|t| {
                                            let tid = t.id.clone();
                                            let selected_id = tid.clone();
                                            view! {
                                                <option value=tid prop:selected=move || entity_type_id.get() == selected_id>{t.name}</option>
                                            }
                                        }).collect_view()}
                                    </select>
                                }
                            })}
                        </Suspense>
                    </label>
                    <label>
                        "位置"
                        <Suspense fallback=|| view! { <select disabled><option>"加载中…"</option></select> }>
                            {move || Suspend::new(async move {
                                let mut opts: Vec<(String, String)> = Vec::new();
                                if let Some(Ok(tree)) = locations.get() {
                                    flatten_locations(&tree, 0, &mut opts);
                                }
                                view! {
                                    <select on:change=move |ev: ev::Event| location_id.set(event_target_value(&ev))>
                                        <option value="" prop:selected=move || location_id.get().is_empty()>"无位置"</option>
                                        {opts.into_iter().map(|(lid, lname)| {
                                            let selected_id = lid.clone();
                                            view! {
                                                <option value=lid prop:selected=move || location_id.get() == selected_id>{lname}</option>
                                            }
                                        }).collect_view()}
                                    </select>
                                }
                            })}
                        </Suspense>
                    </label>
                    <label>
                        "数量"
                        <input type="number" step="any" bind:value=quantity />
                    </label>
                    <label>
                        "最低数量"
                        <input type="number" step="any" bind:value=min_quantity />
                    </label>
                    <label>
                        "制造商"
                        <input type="text" bind:value=manufacturer />
                    </label>
                    <label>
                        "型号"
                        <input type="text" bind:value=model_number />
                    </label>
                    <label>
                        "序列号"
                        <input type="text" bind:value=serial_number />
                    </label>
                    <label>
                        "购买价格"
                        <input type="number" step="0.01" bind:value=purchase_price />
                    </label>
                    <label>
                        "购买日期"
                        <input type="date" bind:value=purchase_date />
                    </label>
                    <label>
                        "购买渠道"
                        <input type="text" bind:value=purchase_from />
                    </label>
                    <label>
                        "保修到期"
                        <input type="date" bind:value=warranty_expires />
                    </label>
                    <label>
                        "备注"
                        <textarea bind:value=notes />
                    </label>
                    <Show when=move || error.get().is_some()>
                        <p class="error">{move || error.get()}</p>
                    </Show>
                    <div class="form-actions">
                        <button type="submit" disabled=move || busy.get()>
                            {move || if busy.get() { "保存中…" } else { "保存" }}
                        </button>
                        <a href="/items">"取消"</a>
                    </div>
                </form>
            </section>
        </div>
    }
}

/// 物品详情页。
#[component]
pub fn ItemDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();

    let entity_res = LocalResource::new(move || {
        let iid = id();
        async move { api::entity(&iid).await.ok() }
    });
    let path_res = LocalResource::new(move || {
        let iid = id();
        async move { api::entity_path(&iid).await.ok() }
    });
    let attachments_res = LocalResource::new(move || {
        let iid = id();
        async move { api::attachments(&iid).await.ok() }
    });

    let upload_kind = RwSignal::new("photo".to_string());
    let upload_title = RwSignal::new(String::new());
    let selected_file = RwSignal::new(None::<web_sys::File>);
    let error = RwSignal::new(None::<String>);

    let on_file_change = move |ev: ev::Event| {
        let input = event_target::<web_sys::HtmlInputElement>(&ev);
        selected_file.set(input.files().and_then(|list| list.get(0)));
    };

    let on_upload = move |_| {
        let file = match selected_file.get() {
            Some(f) => f,
            None => {
                error.set(Some("请先选择要上传的文件".to_string()));
                return;
            }
        };
        let kind = upload_kind.get();
        let title = upload_title.get();
        let eid = id();
        spawn_local(async move {
            let array_buffer = match JsFuture::from(file.array_buffer()).await {
                Ok(v) => v,
                Err(_) => {
                    error.set(Some("读取文件失败".to_string()));
                    return;
                }
            };
            let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
            match api::upload_attachment(&eid, bytes, &file.name(), &file.type_(), &kind, &title).await
            {
                Ok(_) => {
                    error.set(None);
                    selected_file.set(None);
                    upload_title.set(String::new());
                    attachments_res.refetch();
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    view! {
        <div class="page">
            <nav class="breadcrumb">
                <a href="/items">"物品"</a>
                <Suspense fallback=|| view! { <span class="muted">" / …"</span> }>
                    {move || Suspend::new(async move {
                        let paths = path_res.get().flatten().unwrap_or_default();
                        view! {
                            {paths.into_iter().map(|p| view! {
                                <span>" / "</span>
                                <a href=format!("/items/{}", p.id)>{p.name}</a>
                            }).collect_view()}
                        }
                    })}
                </Suspense>
            </nav>

            <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
                {move || Suspend::new(async move {
                    match entity_res.get() {
                        Some(Some(e)) => {
                            let s = &e.summary;
                            let name = s.name.clone();
                            let name_del = name.clone();
                            let desc = s.description.clone();
                            let qty = s.quantity;
                            let min_qty = s.min_quantity;
                            let price = s.purchase_price;
                            let total = s.total_price;
                            let asset = s.asset_id;
                            let serial = e.serial_number.clone();
                            let model = e.model_number.clone();
                            let man = e.manufacturer.clone();
                            let warranty = fmt_date(s.warranty_expires);
                            let reminder = fmt_date(s.reminder_date);
                            let location = s.location.as_ref().map(|l| l.name.clone())
                                .unwrap_or_else(|| "未设置".to_string());
                            let tags = s.tags.clone();
                            let archived = s.archived;
                            let thumbnail = s.thumbnail_id.clone();
                            let maintenance_count = e.maintenance_count;
                            let eid = s.id.clone();
                            let eid_thumb = eid.clone();
                            let eid_dup = eid.clone();
                            let eid_del = eid.clone();
                            let eid_patch = eid.clone();

                            view! {
                                <div class="card">
                                    <div class="detail-header">
                                        <h1>{name}</h1>
                                        {thumbnail.map(|tid| {
                                            let src = api::attachment_url(&eid_thumb, &tid);
                                            view! { <img src=src style="max-width:200px" alt="缩略图" /> }
                                        })}
                                    </div>
                                    <dl class="detail-list">
                                        <dt>"描述"</dt><dd>{desc}</dd>
                                        <dt>"数量"</dt><dd>{qty.to_string()}</dd>
                                        <dt>"最低数量"</dt><dd>{min_qty.to_string()}</dd>
                                        <dt>"购买价格"</dt><dd>{format!("¥{price:.2}")}</dd>
                                        <dt>"总价值"</dt><dd>{format!("¥{total:.2}")}</dd>
                                        <dt>"资产编号"</dt><dd>{format!("#{asset}")}</dd>
                                        <dt>"序列号"</dt><dd>{serial}</dd>
                                        <dt>"型号"</dt><dd>{model}</dd>
                                        <dt>"制造商"</dt><dd>{man}</dd>
                                        <dt>"保修到期"</dt><dd>{warranty}</dd>
                                        <dt>"提醒日期"</dt><dd>{reminder}</dd>
                                        <dt>"位置"</dt><dd>{location}</dd>
                                        <dt>"状态"</dt><dd>{if archived { "已归档" } else { "在用" }}</dd>
                                    </dl>
                                    <div class="tags">
                                        {tags.into_iter().map(|t| {
                                            let color = if t.color.is_empty() {
                                                "#3b82f6".to_string()
                                            } else {
                                                format!("background-color: {}", t.color)
                                            };
                                            view! {
                                                <span class="tag">
                                                    <span class="tag-dot" style=color></span>
                                                    {t.name}
                                                </span>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>

                                <div class="toolbar">
                                    <a class="button" href=format!("/items/{eid}/edit")>"编辑"</a>
                                    <button type="button" on:click=move |_| {
                                        let eid = eid_dup.clone();
                                        spawn_local(async move {
                                            match api::duplicate_entity(&eid).await {
                                                Ok(new_e) => goto(&format!("/items/{}", new_e.summary.id)),
                                                Err(e) => error.set(Some(e.message)),
                                            }
                                        });
                                    }>"复制"</button>
                                    <button type="button" on:click=move |_| {
                                        let msg = format!("确定删除物品「{}」？", name_del);
                                        if web_sys::window()
                                            .and_then(|w| w.confirm_with_message(&msg).ok())
                                            .unwrap_or(false)
                                        {
                                            let eid = eid_del.clone();
                                            spawn_local(async move {
                                                match api::delete_entity(&eid).await {
                                                    Ok(_) => goto("/items"),
                                                    Err(e) => error.set(Some(e.message)),
                                                }
                                            });
                                        }
                                    }>"删除"</button>
                                    <button type="button" on:click=move |_| {
                                        let eid = eid_patch.clone();
                                        spawn_local(async move {
                                            match api::patch_entity(
                                                &eid,
                                                &EntityPatch {
                                                    archived: Some(!archived),
                                                    ..Default::default()
                                                },
                                            ).await {
                                                Ok(_) => entity_res.refetch(),
                                                Err(e) => error.set(Some(e.message)),
                                            }
                                        });
                                    }>{if archived { "取消归档" } else { "归档" }}</button>
                                    <a href=format!("/maintenance/{eid}")>
                                        {format!("保养记录 ({maintenance_count})")}
                                    </a>
                                </div>
                            }.into_any()
                        }
                        Some(None) => view! {
                            <p class="error">"加载失败，请确认后端已启动。"</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>

            <section class="card">
                <h2>"附件"</h2>
                <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
                    {move || Suspend::new(async move {
                        let list = attachments_res.get().flatten().unwrap_or_default();
                        let eid = id();
                        view! {
                            <div class="attachment-grid">
                                {list.into_iter().map(|a: ItemAttachment| {
                                    let aid = a.id.clone();
                                    let aid_del = aid.clone();
                                    let eid_del = eid.clone();
                                    let preview = if a.kind == "photo" {
                                        let src = api::attachment_url(&eid, &aid);
                                        view! { <img src=src alt=a.title.clone() style="max-width:160px" /> }.into_any()
                                    } else {
                                        let href = api::attachment_url(&eid, &aid);
                                        view! { <a href=href>{a.file_name.clone()}</a> }.into_any()
                                    };
                                    view! {
                                        <div class="attachment">
                                            {preview}
                                            <div class="muted">{a.title}</div>
                                            <button type="button" on:click=move |_| {
                                                let aid = aid_del.clone();
                                                let eid = eid_del.clone();
                                                spawn_local(async move {
                                                    match api::delete_attachment(&eid, &aid).await {
                                                        Ok(_) => attachments_res.refetch(),
                                                        Err(e) => error.set(Some(e.message)),
                                                    }
                                                });
                                            }>"删除"</button>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }
                    })}
                </Suspense>
            </section>

            <section class="card">
                <h2>"上传附件"</h2>
                <form on:submit=move |ev| ev.prevent_default()>
                    <label>
                        "类型"
                        <select bind:value=upload_kind>
                            <option value="photo">"照片"</option>
                            <option value="attachment">"附件"</option>
                            <option value="document">"文档"</option>
                            <option value="manual">"说明书"</option>
                            <option value="receipt">"票据"</option>
                        </select>
                    </label>
                    <label>
                        "标题"
                        <input type="text" bind:value=upload_title />
                    </label>
                    <label>
                        "文件"
                        <input type="file" on:change=on_file_change />
                    </label>
                    <div class="form-actions">
                        <button type="button" on:click=on_upload>"上传"</button>
                    </div>
                </form>
            </section>

            {move || error.get().map(|msg| view! { <p class="error">{msg}</p> })}
        </div>
    }
}
