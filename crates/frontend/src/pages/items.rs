//! 物品列表 / 位置页。

use leptos::prelude::*;

use crate::api;

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
                        Some(Some(page_data)) if !page_data.items.is_empty() => {
                            let total = page_data.total;
                            let cur = page_data.page;
                            let pages = (total + 24) / 25;
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
                                <div class="pager">
                                    <button disabled={cur <= 1} on:click=move |_| page.update(|p| *p = (*p - 1).max(1))>"上一页"</button>
                                    <span>{format!("第 {cur} / {pages} 页，共 {total} 件")}</span>
                                    <button disabled={cur >= pages} on:click=move |_| page.update(|p| *p += 1)>"下一页"</button>
                                </div>
                            }.into_any()
                        }
                        Some(Some(_)) => view! { <p class="muted">"没有找到物品。"</p> }.into_any(),
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
