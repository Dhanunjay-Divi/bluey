// Insert inside `sqlite_snapshot_tests` in managed_cloud_provisioning_snapshot.rs.
// Test-only, closed-fixture SQLite logical-state capture; not a generic dumper.
use std::time::Instant;

const MAX_SCHEMA_OBJECTS: usize = 8_192;
const MAX_TABLES: usize = 1_024;
const MAX_COLUMNS: usize = 1_024;
const MAX_ROWS_PER_TABLE: usize = 100_000;
const MAX_TOTAL_ROWS: usize = 1_000_000;
const MAX_TOTAL_CELLS: usize = 1_000_000;
const MAX_RAW_VALUE_BYTES: usize = 1 << 20;
const MAX_ENCODED_VALUE_BYTES: usize = 64 << 20;
const MAX_METADATA_STRING_BYTES: usize = 1 << 20;
const MAX_METADATA_BYTES: usize = 16 << 20;
const MAX_QUERY_BYTES: usize = 2 << 20;

#[derive(Debug, PartialEq, Eq)]
struct DurableSqliteSnapshot {
    application_id: i64,
    schema_version: i64,
    user_version: i64,
    schema: Vec<DurableSqliteSchemaRow>,
    tables: Vec<DurableSqliteTable>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DurableSqliteSchemaRow {
    object_type: String,
    name: String,
    table_name: String,
    sql: Option<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DurableSqliteTable {
    name: String,
    identity_mode: String,
    columns: Vec<String>,
    rows: Vec<DurableSqliteRow>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DurableSqliteRow {
    rowid: i64,
    values: Vec<String>,
}

struct SnapshotBounds {
    started: Instant,
    total_rows: usize,
    total_cells: usize,
    encoded_value_bytes: usize,
    metadata_bytes: usize,
}

impl SnapshotBounds {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            total_rows: 0,
            total_cells: 0,
            encoded_value_bytes: 0,
            metadata_bytes: 0,
        }
    }

    fn checkpoint(&self) -> anyhow::Result<()> {
        if self.started.elapsed().as_secs() >= 30 {
            anyhow::bail!("durable SQLite snapshot exceeded its 30-second checkpoint budget")
        }
        Ok(())
    }

    fn add_encoded_value(&mut self, bytes: usize) -> anyhow::Result<()> {
        self.checkpoint()?;
        self.encoded_value_bytes = self
            .encoded_value_bytes
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("durable SQLite encoded-value size overflow"))?;
        if self.encoded_value_bytes > MAX_ENCODED_VALUE_BYTES {
            anyhow::bail!("durable SQLite snapshot exceeds 64 MiB encoded-value budget")
        }
        Ok(())
    }

    fn add_metadata(&mut self, bytes: usize, label: &str) -> anyhow::Result<()> {
        self.checkpoint()?;
        if bytes > MAX_METADATA_STRING_BYTES {
            anyhow::bail!("durable SQLite snapshot exceeds 1 MiB {label} metadata")
        }
        self.metadata_bytes = self
            .metadata_bytes
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("durable SQLite metadata size overflow"))?;
        if self.metadata_bytes > MAX_METADATA_BYTES {
            anyhow::bail!("durable SQLite snapshot exceeds 16 MiB metadata budget")
        }
        Ok(())
    }
}

struct QueryBudget {
    allocated_bytes: usize,
}

impl QueryBudget {
    fn reserve(&mut self, bytes: usize, label: &str) -> anyhow::Result<()> {
        self.allocated_bytes = self
            .allocated_bytes
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("durable SQLite query-size overflow"))?;
        if self.allocated_bytes > MAX_QUERY_BYTES {
            anyhow::bail!("durable SQLite snapshot exceeds 2 MiB {label} query budget")
        }
        Ok(())
    }
}

/// Captures one main-database checkout and drops it before returning. The
/// snapshot operation under test therefore cannot retain this checkout.
fn capture_durable_sqlite(pool: &DbPool) -> anyhow::Result<DurableSqliteSnapshot> {
    crate::db::run_blocking_db(|| {
        let connection = pool.get()?;
        capture_durable_sqlite_connection(&connection)
    })
}

fn capture_durable_sqlite_connection(
    connection: &rusqlite::Connection,
) -> anyhow::Result<DurableSqliteSnapshot> {
    let mut bounds = SnapshotBounds::new();
    let application_id = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let schema_version = connection.query_row("PRAGMA schema_version", [], |row| row.get(0))?;
    let user_version = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    let mut schema_statement = connection.prepare(
        "SELECT type, name, tbl_name, sql FROM main.sqlite_schema \
         WHERE type IN ('table', 'index', 'trigger', 'view')",
    )?;
    let mut schema_rows = schema_statement.query([])?;
    let mut schema = Vec::new();
    while let Some(row) = schema_rows.next()? {
        bounds.checkpoint()?;
        add_bounded(&schema, MAX_SCHEMA_OBJECTS, "schema objects")?;
        schema.push(DurableSqliteSchemaRow {
            object_type: required_metadata_text(row, 0, "schema object type", &mut bounds)?,
            name: required_metadata_text(row, 1, "schema object name", &mut bounds)?,
            table_name: required_metadata_text(row, 2, "schema table name", &mut bounds)?,
            sql: optional_metadata_text(row, 3, "schema SQL", &mut bounds)?,
        });
    }
    schema.sort();

    let mut tables = Vec::new();
    for entry in schema.iter().filter(|entry| entry.object_type == "table") {
        bounds.checkpoint()?;
        add_bounded(&tables, MAX_TABLES, "tables")?;
        tables.push(capture_table(connection, &entry.name, &mut bounds)?);
    }
    tables.sort();

    Ok(DurableSqliteSnapshot {
        application_id,
        schema_version,
        user_version,
        schema,
        tables,
    })
}

fn capture_table(
    connection: &rusqlite::Connection,
    name: &str,
    bounds: &mut SnapshotBounds,
) -> anyhow::Result<DurableSqliteTable> {
    let mut table_statement = connection
        .prepare("SELECT type, wr FROM pragma_table_list WHERE schema='main' AND name=?1")?;
    let mut table_rows = table_statement.query([name])?;
    let table_row = table_rows
        .next()?
        .ok_or_else(|| anyhow::anyhow!("durable SQLite snapshot found no table-list metadata"))?;
    let table_type = required_metadata_text(table_row, 0, "table-list type", bounds)?;
    let without_rowid: i64 = table_row.get(1)?;
    if table_type == "virtual" || table_type == "shadow" {
        anyhow::bail!("durable SQLite snapshot refuses {table_type} table {name:?}")
    }
    if table_type != "table" {
        anyhow::bail!("durable SQLite snapshot refuses unsupported table type {table_type:?}")
    }
    if without_rowid != 0 {
        anyhow::bail!("durable SQLite snapshot refuses WITHOUT ROWID table {name:?}")
    }

    let mut column_statement =
        connection.prepare("SELECT cid, name, hidden FROM pragma_table_xinfo(?1) ORDER BY cid")?;
    let mut column_rows = column_statement.query([name])?;
    let mut columns = Vec::new();
    while let Some(row) = column_rows.next()? {
        bounds.checkpoint()?;
        let hidden: i64 = row.get(2)?;
        if hidden != 0 {
            anyhow::bail!("durable SQLite snapshot refuses hidden or generated column in {name:?}")
        }
        add_bounded(&columns, MAX_COLUMNS, "columns")?;
        columns.push(required_metadata_text(
            row,
            1,
            "table-xinfo column name",
            bounds,
        )?);
    }
    if columns.is_empty() {
        anyhow::bail!("durable SQLite snapshot found no visible columns for {name:?}")
    }

    let rowid_alias = ["rowid", "_rowid_", "oid"]
        .into_iter()
        .find(|alias| {
            !columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case(alias))
        })
        .ok_or_else(|| {
            anyhow::anyhow!("durable SQLite snapshot found fully shadowed rowid for {name:?}")
        })?;
    let mut query_budget = QueryBudget { allocated_bytes: 0 };
    let quoted_rowid = quote_sqlite_metadata_identifier(rowid_alias, &mut query_budget, bounds)?;
    let quoted_table = quote_sqlite_metadata_identifier(name, &mut query_budget, bounds)?;
    let quoted_columns = columns
        .iter()
        .map(|column| quote_sqlite_metadata_identifier(column, &mut query_budget, bounds))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let select_columns = join_quoted_columns(&quoted_columns, &mut query_budget, bounds)?;
    let sql = select_table_query(
        &quoted_rowid,
        &select_columns,
        &quoted_table,
        &mut query_budget,
        bounds,
    )?;
    let mut statement = connection.prepare(&sql)?;
    let mut query = statement.query([])?;
    let mut rows = Vec::new();
    while let Some(row) = query.next()? {
        bounds.checkpoint()?;
        add_bounded(&rows, MAX_ROWS_PER_TABLE, "rows in one table")?;
        add_counter(&mut bounds.total_rows, MAX_TOTAL_ROWS, "total rows")?;
        let mut values = Vec::new();
        for column in 0..columns.len() {
            bounds.checkpoint()?;
            add_counter(&mut bounds.total_cells, MAX_TOTAL_CELLS, "total cells")?;
            values.push(serialize_sqlite_value(row.get_ref(column + 1)?, bounds)?);
        }
        rows.push(DurableSqliteRow {
            rowid: row.get(0)?,
            values,
        });
    }
    rows.sort();

    bounds.add_metadata(name.len(), "table name copy")?;
    let identity_mode_len = "rowid:"
        .len()
        .checked_add(rowid_alias.len())
        .ok_or_else(|| anyhow::anyhow!("durable SQLite identity-mode length overflow"))?;
    bounds.add_metadata(identity_mode_len, "identity mode")?;
    let mut identity_mode = String::with_capacity(identity_mode_len);
    identity_mode.push_str("rowid:");
    identity_mode.push_str(rowid_alias);
    Ok(DurableSqliteTable {
        name: name.to_owned(),
        identity_mode,
        columns,
        rows,
    })
}

fn required_metadata_text(
    row: &rusqlite::Row<'_>,
    index: usize,
    label: &str,
    bounds: &mut SnapshotBounds,
) -> anyhow::Result<String> {
    match row.get_ref(index)? {
        rusqlite::types::ValueRef::Text(bytes) => metadata_string(bytes, label, bounds),
        _ => anyhow::bail!("durable SQLite snapshot found non-text {label} metadata"),
    }
}

fn optional_metadata_text(
    row: &rusqlite::Row<'_>,
    index: usize,
    label: &str,
    bounds: &mut SnapshotBounds,
) -> anyhow::Result<Option<String>> {
    match row.get_ref(index)? {
        rusqlite::types::ValueRef::Null => Ok(None),
        rusqlite::types::ValueRef::Text(bytes) => metadata_string(bytes, label, bounds).map(Some),
        _ => anyhow::bail!("durable SQLite snapshot found non-text {label} metadata"),
    }
}

fn metadata_string(
    bytes: &[u8],
    label: &str,
    bounds: &mut SnapshotBounds,
) -> anyhow::Result<String> {
    bounds.add_metadata(bytes.len(), label)?;
    Ok(std::str::from_utf8(bytes)?.to_owned())
}

fn quote_sqlite_metadata_identifier(
    identifier: &str,
    query_budget: &mut QueryBudget,
    bounds: &SnapshotBounds,
) -> anyhow::Result<String> {
    bounds.checkpoint()?;
    let escaped_quotes = identifier.bytes().filter(|byte| *byte == b'"').count();
    let capacity = identifier
        .len()
        .checked_add(escaped_quotes)
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| anyhow::anyhow!("durable SQLite quoted-identifier size overflow"))?;
    query_budget.reserve(capacity, "quoted identifier")?;
    let mut quoted = String::with_capacity(capacity);
    quoted.push('"');
    for character in identifier.chars() {
        if character == '"' {
            quoted.push('"');
        }
        quoted.push(character);
    }
    quoted.push('"');
    Ok(quoted)
}

fn join_quoted_columns(
    columns: &[String],
    query_budget: &mut QueryBudget,
    bounds: &SnapshotBounds,
) -> anyhow::Result<String> {
    bounds.checkpoint()?;
    let column_bytes = columns.iter().try_fold(0_usize, |total, column| {
        total
            .checked_add(column.len())
            .ok_or_else(|| anyhow::anyhow!("durable SQLite joined-column size overflow"))
    })?;
    let delimiters = columns
        .len()
        .checked_sub(1)
        .and_then(|count| count.checked_mul(2))
        .ok_or_else(|| anyhow::anyhow!("durable SQLite joined-column size overflow"))?;
    let capacity = column_bytes
        .checked_add(delimiters)
        .ok_or_else(|| anyhow::anyhow!("durable SQLite joined-column size overflow"))?;
    query_budget.reserve(capacity, "joined column list")?;
    let mut joined = String::with_capacity(capacity);
    for (index, column) in columns.iter().enumerate() {
        if index != 0 {
            joined.push_str(", ");
        }
        joined.push_str(column);
    }
    Ok(joined)
}

fn select_table_query(
    quoted_rowid: &str,
    select_columns: &str,
    quoted_table: &str,
    query_budget: &mut QueryBudget,
    bounds: &SnapshotBounds,
) -> anyhow::Result<String> {
    bounds.checkpoint()?;
    const PREFIX: &str = "SELECT ";
    const ROWID_ALIAS: &str = " AS \"durable_snapshot_rowid\", ";
    const FROM: &str = " FROM ";
    let capacity = PREFIX
        .len()
        .checked_add(quoted_rowid.len())
        .and_then(|value| value.checked_add(ROWID_ALIAS.len()))
        .and_then(|value| value.checked_add(select_columns.len()))
        .and_then(|value| value.checked_add(FROM.len()))
        .and_then(|value| value.checked_add(quoted_table.len()))
        .ok_or_else(|| anyhow::anyhow!("durable SQLite generated-query size overflow"))?;
    query_budget.reserve(capacity, "generated SELECT")?;
    let mut sql = String::with_capacity(capacity);
    sql.push_str(PREFIX);
    sql.push_str(quoted_rowid);
    sql.push_str(ROWID_ALIAS);
    sql.push_str(select_columns);
    sql.push_str(FROM);
    sql.push_str(quoted_table);
    Ok(sql)
}

fn add_bounded<T>(items: &[T], maximum: usize, what: &str) -> anyhow::Result<()> {
    if items.len() >= maximum {
        anyhow::bail!("durable SQLite snapshot exceeds {maximum} {what}")
    }
    Ok(())
}

fn add_counter(current: &mut usize, maximum: usize, what: &str) -> anyhow::Result<()> {
    *current = current
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("durable SQLite snapshot {what} counter overflow"))?;
    if *current > maximum {
        anyhow::bail!("durable SQLite snapshot exceeds {maximum} {what}")
    }
    Ok(())
}

fn serialize_sqlite_value(
    value: rusqlite::types::ValueRef<'_>,
    bounds: &mut SnapshotBounds,
) -> anyhow::Result<String> {
    match value {
        rusqlite::types::ValueRef::Null => {
            bounds.add_encoded_value(4)?;
            Ok("null".to_owned())
        }
        rusqlite::types::ValueRef::Integer(value) => {
            bounds.add_encoded_value("integer:".len() + decimal_i64_len(value))?;
            Ok(format!("integer:{value}"))
        }
        rusqlite::types::ValueRef::Real(value) => {
            bounds.add_encoded_value("real:".len() + 16)?;
            Ok(format!("real:{:016x}", value.to_bits()))
        }
        rusqlite::types::ValueRef::Text(value) => encode_sqlite_bytes("text:", value, bounds),
        rusqlite::types::ValueRef::Blob(value) => encode_sqlite_bytes("blob:", value, bounds),
    }
}

fn decimal_i64_len(value: i64) -> usize {
    let mut digits = 1;
    let mut magnitude = value.unsigned_abs();
    while magnitude >= 10 {
        magnitude /= 10;
        digits += 1;
    }
    digits + usize::from(value < 0)
}

fn encode_sqlite_bytes(
    tag: &str,
    value: &[u8],
    bounds: &mut SnapshotBounds,
) -> anyhow::Result<String> {
    if value.len() > MAX_RAW_VALUE_BYTES {
        anyhow::bail!("durable SQLite snapshot exceeds 1 MiB raw value limit")
    }
    let hex_len = value
        .len()
        .checked_mul(2)
        .ok_or_else(|| anyhow::anyhow!("durable SQLite hex length overflow"))?;
    let capacity = tag
        .len()
        .checked_add(hex_len)
        .ok_or_else(|| anyhow::anyhow!("durable SQLite encoded value length overflow"))?;
    bounds.add_encoded_value(capacity)?;
    let mut encoded = String::with_capacity(capacity);
    encoded.push_str(tag);
    encoded.push_str(&hex::encode(value));
    Ok(encoded)
}
