//! CSV 导入导出页面（HomeBox 兼容格式）。

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::api::{self, ImportPreview, ImportResult};

/// 预览行中的字段值转文本（标签数组按逗号拼接）。
fn field_text(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// CSV 导出 / 导入。
#[component]
pub fn CsvPage() -> impl IntoView {
    let export_notice = RwSignal::new(None::<String>);
    let file_name = RwSignal::new(String::new());
    let file_bytes = RwSignal::new(Vec::<u8>::new());
    let preview = RwSignal::new(None::<ImportPreview>);
    let result = RwSignal::new(None::<ImportResult>);
    let error = RwSignal::new(None::<String>);

    let on_export = move |_| {
        export_notice.set(None);
        error.set(None);
        spawn_local(async move {
            match api::export_csv().await {
                Ok(()) => export_notice.set(Some("已开始下载".to_string())),
                Err(e) => error.set(Some(format!("导出失败：{}", e.message))),
            }
        });
    };

    let on_file_change = move |ev: ev::Event| {
        let input = match ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            Some(i) => i,
            None => return,
        };
        let file = match input.files().and_then(|list| list.get(0)) {
            Some(f) => f,
            None => return,
        };
        let name = file.name();
        preview.set(None);
        result.set(None);
        export_notice.set(None);
        error.set(None);
        spawn_local(async move {
            match JsFuture::from(file.array_buffer()).await {
                Ok(buf) => {
                    file_bytes.set(js_sys::Uint8Array::new(&buf).to_vec());
                    file_name.set(name);
                }
                Err(_) => {
                    file_bytes.set(Vec::new());
                    file_name.set(String::new());
                    error.set(Some("读取文件失败".to_string()));
                }
            }
        });
    };

    let on_preview = move |_| {
        error.set(None);
        preview.set(None);
        let name = file_name.get();
        let bytes = file_bytes.get();
        if name.is_empty() || bytes.is_empty() {
            error.set(Some("请先选择 CSV/TSV 文件".to_string()));
            return;
        }
        spawn_local(async move {
            match api::preview_import(&name, bytes).await {
                Ok(p) => preview.set(Some(p)),
                Err(e) => error.set(Some(format!("预览失败：{}", e.message))),
            }
        });
    };

    let on_import = move |_| {
        error.set(None);
        result.set(None);
        let name = file_name.get();
        let bytes = file_bytes.get();
        if name.is_empty() || bytes.is_empty() {
            error.set(Some("请先选择 CSV/TSV 文件".to_string()));
            return;
        }
        spawn_local(async move {
            match api::import_csv(&name, bytes).await {
                Ok(r) => result.set(Some(r)),
                Err(e) => error.set(Some(format!("导入失败：{}", e.message))),
            }
        });
    };

    view! {
        <div class="page">
            <h1>"CSV 导入导出"</h1>

            {move || error.get().map(|msg| view! { <p class="error">{msg}</p> })}

            <section class="section-card">
                <h2>"导出"</h2>
                <p class="muted">
                    "导出所有物品为 CSV，格式与上游 HomeBox 兼容（HB.* 列），可再导入本系统或 HomeBox。"
                </p>
                <p>
                    <button class="btn" on:click=on_export>"下载 CSV"</button>
                </p>
                {move || export_notice.get().map(|msg| view! { <p class="success">{msg}</p> })}
                <p class="muted">
                    "包含列：importRef（去重标识）、name、quantity、location（位置路径）、label（多个标签）、"
                    "purchasePrice、purchaseDate、manufacturer、modelNumber、serialNumber、notes，"
                    "以及实体自定义字段（HB.* 前缀）。"
                </p>
            </section>

            <section class="section-card">
                <h2>"导入"</h2>
                <p class="muted">
                    "支持 CSV 与 TSV 文件（自动识别分隔符）。使用 HB.import_ref 可去重更新已有物品；"
                    "HB.location 支持 `Home / Office / Desk` 嵌套路径；HB.label 逗号分隔多个标签。"
                </p>
                <div class="form-card">
                    <label>
                        "选择文件（.csv / .tsv）"
                        <input type="file" accept=".csv,.tsv,text/csv" on:change=on_file_change />
                    </label>
                    {move || {
                        let name = file_name.get();
                        (!name.is_empty()).then(|| view! { <p class="notice">{format!("已选择文件：{name}")}</p> })
                    }}
                    <div>
                        <button class="btn btn-sm" on:click=on_preview>"预览"</button>
                        <button class="btn btn-sm" on:click=on_import>"确认导入"</button>
                    </div>
                </div>

                <h3>"预览"</h3>
                {move || match preview.get() {
                    None => view! { <p class="muted">"尚未预览。"</p> }.into_any(),
                    Some(p) => {
                        let rows = p
                            .preview
                            .iter()
                            .take(50)
                            .map(|row| {
                                let name = field_text(row.get("name"));
                                let quantity = field_text(row.get("quantity"));
                                let location = field_text(row.get("location"));
                                let labels = field_text(row.get("labels"));
                                let import_ref = field_text(row.get("importRef"));
                                let price = field_text(row.get("purchasePrice"));
                                view! {
                                    <tr>
                                        <td>{name}</td>
                                        <td>{quantity}</td>
                                        <td>{location}</td>
                                        <td>{labels}</td>
                                        <td>{import_ref}</td>
                                        <td>{price}</td>
                                    </tr>
                                }
                            })
                            .collect_view();
                        let shown = p.preview.len().min(50);
                        let total = p.total;
                        let warnings = p.warnings;
                        view! {
                            <p class="notice">{format!("共 {total} 行，显示前 {shown} 行")}</p>
                            {(!warnings.is_empty()).then(|| view! {
                                <div class="notice">
                                    <strong>"警告："</strong>
                                    <ul>{warnings.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view()}</ul>
                                </div>
                            })}
                            <table class="items-table">
                                <thead>
                                    <tr>
                                        <th>"名称"</th>
                                        <th>"数量"</th>
                                        <th>"位置"</th>
                                        <th>"标签"</th>
                                        <th>"importRef"</th>
                                        <th>"购买价格"</th>
                                    </tr>
                                </thead>
                                <tbody>{rows}</tbody>
                            </table>
                        }
                        .into_any()
                    }
                }}

                <h3>"导入结果"</h3>
                {move || match result.get() {
                    None => view! { <p class="muted">"尚未导入。"</p> }.into_any(),
                    Some(r) => {
                        let created = r.created;
                        let updated = r.updated;
                        let skipped = r.skipped;
                        let errors = r.errors;
                        let warnings = r.warnings;
                        view! {
                            <div class="success">
                                <strong>"导入完成："</strong>
                                {format!("新增 {created} 件 / 更新 {updated} 件 / 跳过 {skipped} 行")}
                            </div>
                            {(!errors.is_empty()).then(|| view! {
                                <div class="error">
                                    <strong>"错误："</strong>
                                    <ul>{errors.iter().map(|e| view! { <li>{e.clone()}</li> }).collect_view()}</ul>
                                </div>
                            })}
                            {(!warnings.is_empty()).then(|| view! {
                                <div class="notice">
                                    <strong>"警告："</strong>
                                    <ul>{warnings.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view()}</ul>
                                </div>
                            })}
                        }
                        .into_any()
                    }
                }}
            </section>
        </div>
    }
}
