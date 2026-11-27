//! 仪表盘：统计卡片 + 位置/标签汇总 + 采购时间线。

use leptos::prelude::*;

use crate::api;

/// 仪表盘首页。
#[component]
pub fn DashboardPage() -> impl IntoView {
    let stats = LocalResource::new(move || async move { api::statistics().await.ok() });
    let tree = LocalResource::new(move || async move { api::locations_tree().await.ok() });
    let by_location = LocalResource::new(move || async move { api::stats_by_location().await.ok() });
    let by_tag = LocalResource::new(move || async move { api::stats_by_tag().await.ok() });
    let timeline = LocalResource::new(move || async move { api::stats_value_over_time().await.ok() });

    view! {
        <div class="page">
            <div class="page-header">
                <h1>"仪表盘"</h1>
                <a class="btn" href="/items/new">"+ 新建物品"</a>
            </div>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match stats.get() {
                        Some(Some(s)) => view! {
                            <div class="stat-grid">
                                <div class="stat-card"><div class="stat-num">{s.total_items}</div><div class="stat-label">"物品总数"</div></div>
                                <div class="stat-card"><div class="stat-num">{format!("¥{:.2}", s.total_value)}</div><div class="stat-label">"资产总值"</div></div>
                                <div class="stat-card"><div class="stat-num">{s.total_locations}</div><div class="stat-label">"位置数"</div></div>
                                <div class="stat-card"><div class="stat-num">{s.total_low_stock}</div><div class="stat-label">"库存不足"</div></div>
                                <div class="stat-card"><div class="stat-num">{s.total_with_warranty}</div><div class="stat-label">"保修中"</div></div>
                                <div class="stat-card"><div class="stat-num">{s.total_tags}</div><div class="stat-label">"标签数"</div></div>
                            </div>
                        }.into_any(),
                        _ => view! { <p class="muted">"暂无统计数据"</p> }.into_any(),
                    }
                })}
            </Suspense>

            <div class="two-col">
                <section>
                    <h2>"位置概览"</h2>
                    <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                        {move || Suspend::new(async move {
                            match tree.get() {
                                Some(Some(items)) if !items.is_empty() => view! {
                                    <ul class="tree">{items.into_iter().map(|t| render_tree(&t)).collect_view()}</ul>
                                }.into_any(),
                                _ => view! { <p class="muted">"还没有位置。"</p> }.into_any(),
                            }
                        })}
                    </Suspense>
                </section>

                <section>
                    <h2>"采购时间线"</h2>
                    <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                        {move || Suspend::new(async move {
                            match timeline.get() {
                                Some(Some(entries)) if !entries.is_empty() => {
                                    let max = entries.iter().map(|e| e.value.abs()).fold(0.0f64, f64::max).max(1.0);
                                    view! {
                                        <div class="timeline">
                                            {entries.into_iter().map(|e| {
                                                let pct = (e.value.abs() / max * 100.0).max(2.0);
                                                view! {
                                                    <div class="timeline-row">
                                                        <span class="timeline-label">{e.name}</span>
                                                        <div class="timeline-bar-wrap"><div class="timeline-bar" style={format!("width: {pct:.1}%")}></div></div>
                                                        <span class="timeline-value">{format!("¥{:.0}", e.value)}</span>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }
                                _ => view! { <p class="muted">"暂无采购记录"</p> }.into_any(),
                            }
                        })}
                    </Suspense>
                </section>
            </div>

            <div class="two-col">
                <section>
                    <h2>"按位置汇总"</h2>
                    <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                        {move || Suspend::new(async move {
                            match by_location.get() {
                                Some(Some(rows)) if !rows.is_empty() => view! {
                                    <table class="items-table">
                                        <thead><tr><th>"位置"</th><th>"数量"</th><th>"总值"</th></tr></thead>
                                        <tbody>
                                            {rows.into_iter().map(|r| view! {
                                                <tr><td>{r.name}</td><td>{r.count}</td><td>{format!("¥{:.2}", r.total)}</td></tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any(),
                                _ => view! { <p class="muted">"暂无数据"</p> }.into_any(),
                            }
                        })}
                    </Suspense>
                </section>

                <section>
                    <h2>"按标签汇总"</h2>
                    <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                        {move || Suspend::new(async move {
                            match by_tag.get() {
                                Some(Some(rows)) if !rows.is_empty() => view! {
                                    <table class="items-table">
                                        <thead><tr><th>"标签"</th><th>"数量"</th><th>"总值"</th></tr></thead>
                                        <tbody>
                                            {rows.into_iter().map(|r| view! {
                                                <tr><td>{r.name}</td><td>{r.count}</td><td>{format!("¥{:.2}", r.total)}</td></tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any(),
                                _ => view! { <p class="muted">"暂无数据"</p> }.into_any(),
                            }
                        })}
                    </Suspense>
                </section>
            </div>
        </div>
    }
}

/// 递归渲染位置树。
fn render_tree(item: &common::dto::TreeItem) -> AnyView {
    let count = item.item_count;
    view! {
        <li>
            <span class="tree-icon">{icon_of(&item.icon)}</span>
            <a href="/items">{item.name.clone()}</a>
            <span class="muted">{format!(" ({count} 件物品)")}</span>
            {(!item.children.is_empty()).then(|| {
                let kids = item.children.clone();
                view! { <ul class="tree">{kids.iter().map(|k| render_tree(k)).collect_view()}</ul> }
            })}
        </li>
    }.into_any()
}

fn icon_of(raw: &str) -> &'static str {
    match raw {
        "LocationHome" => "🏠",
        "LocationRoom" => "🛏",
        "LocationBox" => "📦",
        "LocationShelf" => "🗄",
        "LocationDrawer" => "🗃",
        "LocationFridge" => "🧊",
        _ => "📁",
    }
}
