//! CSV / TSV import & export routes (HomeBox-compatible `HB.*` columns).

use salvo::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use salvo::writing::Json;
use salvo::Response;

use crate::auth::guard::require_user;
use crate::db::repo::csvio::{self, ExportRow, ImportRow};
use crate::db::{with_read, with_write};
use crate::error::{AppError, AppResult};
use crate::router;

/// Standard export columns (order matters for the file layout).
const STD_COLS: &[&str] = &[
    "HB.import_ref", "HB.name", "HB.description", "HB.quantity", "HB.min_quantity",
    "HB.asset_id", "HB.insured", "HB.serial_number", "HB.model_number", "HB.manufacturer",
    "HB.notes", "HB.purchase_from", "HB.purchase_price", "HB.purchase_time",
    "HB.lifetime_warranty", "HB.warranty_expires", "HB.warranty_details",
    "HB.sold_to", "HB.sold_price", "HB.sold_time", "HB.sold_notes", "HB.location", "HB.label",
];

fn date_only(ts: Option<&str>) -> String {
    ts.map(|s| s.chars().take(10).collect::<String>()).unwrap_or_default()
}

fn build_csv(rows: &[ExportRow]) -> AppResult<Vec<u8>> {
    // Collect all custom field names so every row has the same columns.
    let mut field_names: Vec<String> = Vec::new();
    for row in rows {
        for (name, _) in &row.fields {
            if !field_names.contains(name) {
                field_names.push(name.clone());
            }
        }
    }
    field_names.sort();

    let mut wtr = csv::WriterBuilder::new().from_writer(Vec::new());
    let mut header: Vec<String> = STD_COLS.iter().map(|s| s.to_string()).collect();
    header.extend(field_names.iter().map(|n| format!("HB.field.{n}")));
    wtr.write_record(&header)
        .map_err(|e| AppError::Internal(format!("csv header: {e}")))?;

    for row in rows {
        let fields: std::collections::HashMap<&str, &str> = row
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut rec: Vec<String> = vec![
            if row.import_ref.is_empty() { row.id.clone() } else { row.import_ref.clone() },
            row.name.clone(),
            row.description.clone(),
            row.quantity.to_string(),
            row.min_quantity.to_string(),
            row.asset_id.to_string(),
            row.insured.to_string(),
            row.serial_number.clone(),
            row.model_number.clone(),
            row.manufacturer.clone(),
            row.notes.clone(),
            row.purchase_from.clone(),
            row.purchase_price.to_string(),
            date_only(row.purchase_date.as_deref()),
            row.lifetime_warranty.to_string(),
            date_only(row.warranty_expires.as_deref()),
            row.warranty_details.clone(),
            row.sold_to.clone(),
            row.sold_price.to_string(),
            date_only(row.sold_date.as_deref()),
            row.sold_notes.clone(),
            row.location_name.clone(),
            row.tags.join(","),
        ];
        for name in &field_names {
            rec.push(fields.get(name.as_str()).copied().unwrap_or("").to_string());
        }
        wtr.write_record(&rec)
            .map_err(|e| AppError::Internal(format!("csv row: {e}")))?;
    }
    wtr.into_inner().map_err(|e| AppError::Internal(format!("csv flush: {e}")))
}

/// GET /api/v1/entities/export — download all items as HomeBox-compatible CSV.
#[salvo::handler]
pub async fn export(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
    res: &mut Response,
) -> AppResult<()> {
    let pool = router::pool(depot);
    let config = router::config(depot);
    let auth = require_user(req, config)?;
    let gid = auth.group_id.clone();
    let rows = with_read(pool, move |conn| csvio::export_rows(conn, &gid)).await?;
    let bytes = build_csv(&rows)?;
    res.headers_mut().insert(
        CONTENT_TYPE,
        "text/csv; charset=utf-8".parse().map_err(|_| AppError::Internal("bad mime".into()))?,
    );
    res.headers_mut().insert(
        CONTENT_DISPOSITION,
        "attachment; filename=\"homeitem-export.csv\""
            .parse()
            .map_err(|_| AppError::Internal("bad disposition".into()))?,
    );
    res.body(salvo::http::body::ResBody::Once(salvo::hyper::body::Bytes::from(bytes)));
    Ok(())
}

/// Detect delimiter by counting commas vs tabs in the first line.
fn detect_delimiter(bytes: &[u8]) -> u8 {
    let first_line = bytes
        .iter()
        .take_while(|&&b| b != b'\n')
        .copied()
        .collect::<Vec<_>>();
    let commas = first_line.iter().filter(|&&b| b == b',').count();
    let tabs = first_line.iter().filter(|&&b| b == b'\t').count();
    if tabs > commas { b'\t' } else { b',' }
}

fn parse_bool(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "1" | "y" | "t"
    )
}

fn parse_date(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Accept YYYY-MM-DD and ISO timestamps; normalize to ISO for the DB.
    if s.len() == 10 {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc).to_rfc3339())
    } else {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339())
    }
}

fn parse_f64(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

fn parse_asset_id(s: &str) -> i64 {
    let cleaned = s.trim().replace('-', "");
    cleaned.parse().unwrap_or(0)
}

/// Parse CSV/TSV bytes into import rows. Returns (rows, warnings).
fn parse_rows(bytes: &[u8]) -> AppResult<(Vec<ImportRow>, Vec<String>)> {
    let delim = detect_delimiter(bytes);
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .from_reader(bytes);
    let headers = rdr
        .headers()
        .map_err(|e| AppError::BadRequest(format!("CSV 表头解析失败: {e}")))?
        .iter()
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for (idx, record) in rdr.records().enumerate() {
        let record = record.map_err(|e| AppError::BadRequest(format!("第 {} 行解析失败: {e}", idx + 2)))?;
        let mut row = ImportRow::default();
        for (col, value) in headers.iter().zip(record.iter()) {
            let v = value.trim();
            match col.as_str() {
                "HB.import_ref" => row.import_ref = v.chars().take(100).collect(),
                "HB.name" => row.name = v.to_string(),
                "HB.description" => row.description = v.to_string(),
                "HB.quantity" => row.quantity = parse_f64(v),
                "HB.min_quantity" => row.min_quantity = parse_f64(v),
                "HB.asset_id" => row.asset_id = parse_asset_id(v),
                "HB.insured" => row.insured = parse_bool(v),
                "HB.serial_number" => row.serial_number = v.to_string(),
                "HB.model_number" => row.model_number = v.to_string(),
                "HB.manufacturer" => row.manufacturer = v.to_string(),
                "HB.notes" => row.notes = v.chars().take(1000).collect(),
                "HB.purchase_from" => row.purchase_from = v.to_string(),
                "HB.purchase_price" => row.purchase_price = parse_f64(v),
                "HB.purchase_time" => row.purchase_date = parse_date(v),
                "HB.lifetime_warranty" => row.lifetime_warranty = parse_bool(v),
                "HB.warranty_expires" => row.warranty_expires = parse_date(v),
                "HB.warranty_details" => row.warranty_details = v.to_string(),
                "HB.sold_to" => row.sold_to = v.to_string(),
                "HB.sold_price" => row.sold_price = parse_f64(v),
                "HB.sold_time" => row.sold_date = parse_date(v),
                "HB.sold_notes" => row.sold_notes = v.chars().take(1000).collect(),
                "HB.location" => row.location_path = v.to_string(),
                "HB.label" => {
                    row.tags = v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
                }
                _ if col.starts_with("HB.field.") => {
                    let name = col.trim_start_matches("HB.field.").trim();
                    if !name.is_empty() && !v.is_empty() {
                        row.fields.push((name.to_string(), v.to_string()));
                    }
                }
                _ => {}
            }
        }
        if row.name.trim().is_empty() {
            warnings.push(format!("第 {} 行缺少 HB.name，已跳过", idx + 2));
            continue;
        }
        if row.quantity <= 0.0 {
            row.quantity = 1.0;
        }
        rows.push(row);
    }
    Ok((rows, warnings))
}

/// Read the uploaded file bytes from a multipart request.
async fn read_upload(req: &mut salvo::Request, max_bytes: usize) -> AppResult<Vec<u8>> {
    use std::io::Read;
    let form = req
        .form_data()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart 解析失败: {e}")))?;
    let file = form
        .files
        .get("file")
        .cloned()
        .ok_or_else(|| AppError::BadRequest("缺少 file 字段".into()))?;
    if file.size() > max_bytes as u64 {
        return Err(AppError::PayloadTooLarge("文件超过上限".into()));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(file.path())
        .and_then(|mut f| f.read_to_end(&mut bytes))
        .map_err(|e| AppError::Internal(format!("读取上传文件: {e}")))?;
    Ok(bytes)
}

/// POST /api/v1/entities/import/preview — parse and preview without writing.
#[salvo::handler]
pub async fn preview(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let config = router::config(depot).clone();
    require_user(req, &config)?;
    let bytes = read_upload(req, (config.max_upload_mb as usize) * 1024 * 1024).await?;
    let (rows, warnings) = parse_rows(&bytes)?;
    let preview: Vec<serde_json::Value> = rows
        .iter()
        .take(50)
        .map(|r| serde_json::json!({
            "importRef": r.import_ref,
            "name": r.name,
            "quantity": r.quantity,
            "location": r.location_path,
            "labels": r.tags,
            "purchasePrice": r.purchase_price,
        }))
        .collect();
    Ok(Json(serde_json::json!({
        "total": rows.len(),
        "warnings": warnings,
        "preview": preview,
    })))
}

/// POST /api/v1/entities/import — import CSV/TSV with upsert by HB.import_ref.
#[salvo::handler]
pub async fn import(
    depot: &mut salvo::Depot,
    req: &mut salvo::Request,
) -> AppResult<Json<serde_json::Value>> {
    let pool = router::pool(depot);
    let config = router::config(depot).clone();
    let auth = require_user(req, &config)?;
    let bytes = read_upload(req, (config.max_upload_mb as usize) * 1024 * 1024).await?;
    let (rows, warnings) = parse_rows(&bytes)?;
    let gid = auth.group_id.clone();

    let result = with_write(pool, move |conn| {
        let mut created = 0usize;
        let mut updated = 0usize;
        let mut errors = Vec::new();
        for (idx, row) in rows.iter().enumerate() {
            match csvio::upsert_row(conn, &gid, row) {
                Ok((_, was_created)) => {
                    if was_created { created += 1 } else { updated += 1 }
                }
                Err(e) => errors.push(format!("第 {} 行 ({}): {}", idx + 2, row.name, e)),
            }
        }
        Ok::<_, crate::error::AppError>((created, updated, errors))
    })
    .await?;

    let (created, updated, errors) = result;
    Ok(Json(serde_json::json!({
        "created": created,
        "updated": updated,
        "skipped": errors.len(),
        "warnings": warnings,
        "errors": errors,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_csv_and_tsv() {
        assert_eq!(detect_delimiter(b"HB.name,HB.quantity\nItem A,2\n"), b',');
        assert_eq!(detect_delimiter(b"HB.name\tHB.quantity\nItem A\t2\n"), b'\t');
    }

    #[test]
    fn parse_bool_variants() {
        assert!(parse_bool("true"));
        assert!(parse_bool("YES"));
        assert!(parse_bool("1"));
        assert!(!parse_bool("false"));
        assert!(!parse_bool("no"));
        assert!(!parse_bool("0"));
    }

    #[test]
    fn parse_csv_with_all_columns() {
        let csv = b"HB.import_ref,HB.name,HB.quantity,HB.location,HB.label,HB.purchase_time,HB.field.Serial Number\nref-1,MacBook,2,Home / Office,laptop,2025-06-15,SN123\n";
        let (rows, warnings) = parse_rows(csv).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].import_ref, "ref-1");
        assert_eq!(rows[0].name, "MacBook");
        assert_eq!(rows[0].quantity, 2.0);
        assert_eq!(rows[0].location_path, "Home / Office");
        assert_eq!(rows[0].tags, vec!["laptop"]);
        assert_eq!(rows[0].purchase_date.as_deref(), Some("2025-06-15T00:00:00+00:00"));
        assert_eq!(rows[0].fields[0].0, "Serial Number");
        assert_eq!(rows[0].fields[0].1, "SN123");
    }

    #[test]
    fn parse_tsv_flexible() {
        let tsv = b"HB.name\tHB.price\nWidget\t9.99\n"; // unknown column ignored
        let (rows, _) = parse_rows(tsv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Widget");
    }

    #[test]
    fn skips_rows_without_name() {
        let csv = b"HB.name,HB.quantity\n,2\nNamed,1\n";
        let (rows, warnings) = parse_rows(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Named");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn export_header_matches_upstream() {
        let bytes = build_csv(&[]).unwrap();
        let header = String::from_utf8(bytes).unwrap();
        assert!(header.contains("HB.import_ref"));
        assert!(header.contains("HB.name"));
        assert!(header.contains("HB.location"));
        assert!(header.contains("HB.label"));
        assert!(header.contains("HB.purchase_time"));
    }
}
