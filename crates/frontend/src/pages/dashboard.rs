//! 仪表盘：统计卡片 + 位置概览 + 提醒页。

use leptos::prelude::*;

use crate::api;

/// 仪表盘首页。
#[component]
pub fn DashboardPage() -> impl IntoView {
    let stats = LocalResource::new(move || async move { api::statistics().await.ok() });
    let tree = LocalResource::new(move || async move { api::locations_tree().await.ok() });

    view! {
        <div class="page">
            <h1>"仪表盘"</h1>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match stats.get() {
                        Some(Some(s)) => view! {
                            <div class="stat-grid">
                                <div class="stat-card">
                                    <div class="stat-num">{s.total_items}</div>
                                    <div class="stat-label">"物品总数"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-num">{format!("¥{:.0}", s.total_value)}</div>
                                    <div class="stat-label">"资产总值"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-num">{s.total_locations}</div>
                                    <div class="stat-label">"位置数"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-num">{s.total_low_stock}</div>
                                    <div class="stat-label">"库存不足"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-num">{s.total_with_warranty}</div>
                                    <div class="stat-label">"保修中"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-num">{s.total_tags}</div>
                                    <div class="stat-label">"标签数"</div>
                                </div>
                            </div>
                        }.into_any(),
                        _ => view! { <p class="muted">"暂无统计数据"</p> }.into_any(),
                    }
                })}
            </Suspense>
            <h2>"位置概览"</h2>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match tree.get() {
                        Some(Some(items)) if !items.is_empty() => view! {
                            <ul class="tree">
                                {items.into_iter().map(|t| render_tree(&t)).collect_view()}
                            </ul>
                        }.into_any(),
                        _ => view! { <p class="muted">"还没有位置，去物品页创建吧。"</p> }.into_any(),
                    }
                })}
            </Suspense>
        </div>
    }
}

/// 递归渲染位置树。
fn render_tree(item: &common::dto::TreeItem) -> AnyView {
    let count = item.item_count;
    view! {
        <li>
            <span class="tree-icon">{icon_of(&item.icon)}</span>
            <strong>{item.name.clone()}</strong>
            <span class="muted">{format!(" ({count} 件物品)")}</span>
            {(!item.children.is_empty()).then(|| {
                let kids = item.children.clone();
                view! {
                    <ul class="tree">
                        {kids.iter().map(|k| render_tree(k)).collect_view()}
                    </ul>
                }
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

/// 提醒页（保修/保养/库存不足汇总）。
#[component]
pub fn RemindersPage() -> impl IntoView {
    view! {
        <div class="page">
            <h1>"提醒"</h1>
            <p class="muted">"保修到期、自定义提醒、保养计划与库存不足的汇总。"</p>
            <p class="muted">"后台任务会按配置的间隔扫描并通过 SMTP 邮件推送。"</p>
        </div>
    }
}
