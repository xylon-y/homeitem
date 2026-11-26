//! 根组件：路由表 + 全局布局。

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::pages::{dashboard, items, login, settings, setup};

/// 未登录时的路由（登录/初始化）。
#[component]
fn GuestRoutes() -> impl IntoView {
    view! {
        <Routes fallback=|| "页面不存在">
            <Route path=path!("/setup") view=setup::SetupPage />
            <Route path=path!("/*any") view=login::LoginPage />
        </Routes>
    }
}

/// 登录后的主路由。
#[component]
fn MainRoutes() -> impl IntoView {
    view! {
        <Routes fallback=|| "页面不存在">
            <Route path=path!("/") view=dashboard::DashboardPage />
            <Route path=path!("/items") view=items::ItemsPage />
            <Route path=path!("/locations") view=items::LocationsPage />
            <Route path=path!("/reminders") view=dashboard::RemindersPage />
            <Route path=path!("/settings") view=settings::SettingsPage />
        </Routes>
    }
}

/// 应用根：检查登录态，路由分发。
#[component]
pub fn App() -> impl IntoView {
    let logged_in = move || crate::api::token::get().is_some();

    view! {
        <Router>
            <div class="app-shell">
                <Show when={logged_in} fallback=|| view! { <GuestRoutes /> }>
                    <nav class="topbar">
                        <a class="brand" href="/">"homeitem 物品管理"</a>
                        <div class="nav-links">
                            <a href="/">"仪表盘"</a>
                            <a href="/items">"物品"</a>
                            <a href="/locations">"位置"</a>
                            <a href="/reminders">"提醒"</a>
                            <a href="/settings">"设置"</a>
                            <button class="btn-link" on:click=move |_| {
                                crate::api::logout();
                                if let Some(w) = web_sys::window() {
                                    let _ = w.location().reload();
                                }
                            }>"退出"</button>
                        </div>
                    </nav>
                    <main class="content">
                        <MainRoutes />
                    </main>
                </Show>
            </div>
        </Router>
    }
}
