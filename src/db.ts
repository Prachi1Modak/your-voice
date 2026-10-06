import Database from "@tauri-apps/plugin-sql";

let db: Database | null = null;

export async function getDb(): Promise<Database> {
if (db) return db;

db = await Database.load("sqlite:metadata.db");

await db.execute(`
    CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL,
    type TEXT NOT NULL,
    file_path TEXT NOT NULL,
    thumbnail_path TEXT,
    transcript TEXT,
    duration_secs INT,
    created_at TEXT NOT NULL
    )
`);

await db.execute(`
    CREATE TABLE IF NOT EXISTS weekly_reports (
    id TEXT PRIMARY KEY,
    week_start TEXT NOT NULL,
    week_end TEXT NOT NULL,
    report_md TEXT NOT NULL,
    provider TEXT NOT NULL,
    generated_at TEXT NOT NULL
    )
`);

await db.execute(`
    CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
    )
`);

await db.execute(`
    INSERT OR IGNORE INTO settings (key, value) VALUES ('use_claude_api', 'false')
`);

return db;
}

export async function createEntry(params: {
  type: 'video' | 'audio';
  filePath: string;
  thumbnailPath?: string;
  durationSecs?: number;
}): Promise<string> {
  const db = await getDb();
  const id = crypto.randomUUID();
  const date = new Date().toISOString().slice(0, 10);
  const createdAt = new Date().toISOString();
  await db.execute(
    `INSERT INTO entries (id, date, type, file_path, thumbnail_path, transcript, duration_secs, created_at)
     VALUES (?, ?, ?, ?, ?, NULL, ?, ?)`,
    [id, date, params.type, params.filePath, params.thumbnailPath ?? null, params.durationSecs ?? null, createdAt]
  );
  return id;
}

export async function updateTranscript(entryId: string, transcript: string): Promise<void> {
  const db = await getDb();
  await db.execute(`UPDATE entries SET transcript = ? WHERE id = ?`, [transcript, entryId]);
}

export async function getEntriesForDate(date: string): Promise<any[]> {
  const db = await getDb();
  return db.select(`SELECT * FROM entries WHERE date = ? ORDER BY created_at DESC`, [date]);
}

export async function getEntriesForMonth(yearMonth: string): Promise<any[]> {
  // yearMonth = "2026-10"
  const db = await getDb();
  return db.select(`SELECT * FROM entries WHERE date LIKE ? ORDER BY created_at DESC`, [yearMonth + '%']);
}