use crate::config::{Instance, Tunnel};
use crate::error::{NydusError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct StateDb {
    conn: Connection,
}

impl StateDb {
    /// Open or create the state database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = StateDb { conn };
        db.run_migrations()?;
        Ok(db)
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<()> {
        // Create instances table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS instances (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                profile TEXT NOT NULL,
                region TEXT NOT NULL,
                instance_id TEXT NOT NULL,
                public_ip TEXT,
                public_dns TEXT,
                private_ip TEXT,
                ssh_user TEXT NOT NULL,
                ssh_key_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                last_synced INTEGER,
                desired_state TEXT NOT NULL,
                status TEXT,
                notes TEXT
            )",
            [],
        )?;

        // Create index on name
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_instances_name ON instances(name)",
            [],
        )?;

        // Create index on instance_id
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_instances_instance_id ON instances(instance_id)",
            [],
        )?;

        // Create tunnels table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tunnels (
                id INTEGER PRIMARY KEY,
                instance_name TEXT NOT NULL,
                remote_host TEXT NOT NULL DEFAULT '127.0.0.1',
                remote_port INTEGER NOT NULL,
                local_port INTEGER NOT NULL,
                pid INTEGER,
                started_at INTEGER NOT NULL,
                mode TEXT NOT NULL,
                open_url INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (instance_name) REFERENCES instances(name) ON DELETE CASCADE
            )",
            [],
        )?;

        // Create indexes on tunnels
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tunnels_instance ON tunnels(instance_name)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tunnels_pid ON tunnels(pid)",
            [],
        )?;

        // Create context table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS context (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        Ok(())
    }

    /// Insert or update an instance
    pub fn upsert_instance(&self, instance: &Instance) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO instances (
                name, profile, region, instance_id, public_ip, public_dns, private_ip,
                ssh_user, ssh_key_path, created_at, last_seen, last_synced,
                desired_state, status, notes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(name) DO UPDATE SET
                profile = excluded.profile,
                region = excluded.region,
                instance_id = excluded.instance_id,
                public_ip = excluded.public_ip,
                public_dns = excluded.public_dns,
                private_ip = excluded.private_ip,
                ssh_user = excluded.ssh_user,
                ssh_key_path = excluded.ssh_key_path,
                last_seen = excluded.last_seen,
                last_synced = excluded.last_synced,
                desired_state = excluded.desired_state,
                status = excluded.status,
                notes = excluded.notes",
            params![
                instance.name,
                instance.profile,
                instance.region,
                instance.instance_id,
                instance.public_ip,
                instance.public_dns,
                instance.private_ip,
                instance.ssh_user,
                instance.ssh_key_path,
                instance.created_at,
                instance.last_seen,
                instance.last_synced,
                instance.desired_state,
                instance.status,
                instance.notes,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get an instance by name
    pub fn get_instance(&self, name: &str) -> Result<Instance> {
        self.conn
            .query_row(
                "SELECT id, name, profile, region, instance_id, public_ip, public_dns, private_ip,
                        ssh_user, ssh_key_path, created_at, last_seen, last_synced,
                        desired_state, status, notes
                 FROM instances WHERE name = ?1",
                params![name],
                |row| {
                    Ok(Instance {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        profile: row.get(2)?,
                        region: row.get(3)?,
                        instance_id: row.get(4)?,
                        public_ip: row.get(5)?,
                        public_dns: row.get(6)?,
                        private_ip: row.get(7)?,
                        ssh_user: row.get(8)?,
                        ssh_key_path: row.get(9)?,
                        created_at: row.get(10)?,
                        last_seen: row.get(11)?,
                        last_synced: row.get(12)?,
                        desired_state: row.get(13)?,
                        status: row.get(14)?,
                        notes: row.get(15)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| NydusError::InstanceNotFound(name.to_string()))
    }

    /// List all instances
    pub fn list_instances(&self) -> Result<Vec<Instance>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, profile, region, instance_id, public_ip, public_dns, private_ip,
                    ssh_user, ssh_key_path, created_at, last_seen, last_synced,
                    desired_state, status, notes
             FROM instances ORDER BY created_at DESC",
        )?;

        let instances = stmt
            .query_map([], |row| {
                Ok(Instance {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    profile: row.get(2)?,
                    region: row.get(3)?,
                    instance_id: row.get(4)?,
                    public_ip: row.get(5)?,
                    public_dns: row.get(6)?,
                    private_ip: row.get(7)?,
                    ssh_user: row.get(8)?,
                    ssh_key_path: row.get(9)?,
                    created_at: row.get(10)?,
                    last_seen: row.get(11)?,
                    last_synced: row.get(12)?,
                    desired_state: row.get(13)?,
                    status: row.get(14)?,
                    notes: row.get(15)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(instances)
    }

    /// Delete an instance
    pub fn delete_instance(&self, name: &str) -> Result<()> {
        let rows = self.conn.execute("DELETE FROM instances WHERE name = ?1", params![name])?;
        if rows == 0 {
            return Err(NydusError::InstanceNotFound(name.to_string()));
        }
        Ok(())
    }

    /// Get current context (active instance name)
    pub fn get_current_context(&self) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM context WHERE key = 'current_instance'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Set current context
    pub fn set_current_context(&self, name: Option<&str>) -> Result<()> {
        if let Some(name) = name {
            self.conn.execute(
                "INSERT INTO context (key, value) VALUES ('current_instance', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![name],
            )?;
        } else {
            self.conn.execute("DELETE FROM context WHERE key = 'current_instance'", [])?;
        }
        Ok(())
    }

    /// Create a tunnel
    pub fn create_tunnel(&self, tunnel: &Tunnel) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tunnels (
                instance_name, remote_host, remote_port, local_port, pid,
                started_at, mode, open_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                tunnel.instance_name,
                tunnel.remote_host,
                tunnel.remote_port,
                tunnel.local_port,
                tunnel.pid,
                tunnel.started_at,
                tunnel.mode,
                tunnel.open_url as i32,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// List tunnels for an instance
    pub fn list_tunnels(&self, instance_name: Option<&str>) -> Result<Vec<Tunnel>> {
        if let Some(name) = instance_name {
            let mut stmt = self.conn.prepare(
                "SELECT id, instance_name, remote_host, remote_port, local_port, pid,
                        started_at, mode, open_url
                 FROM tunnels WHERE instance_name = ?1 ORDER BY started_at DESC",
            )?;
            let tunnels = stmt
                .query_map(params![name], |row| {
                    Ok(Tunnel {
                        id: row.get(0)?,
                        instance_name: row.get(1)?,
                        remote_host: row.get(2)?,
                        remote_port: row.get(3)?,
                        local_port: row.get(4)?,
                        pid: row.get(5)?,
                        started_at: row.get(6)?,
                        mode: row.get(7)?,
                        open_url: row.get::<_, i32>(8)? != 0,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(tunnels)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, instance_name, remote_host, remote_port, local_port, pid,
                        started_at, mode, open_url
                 FROM tunnels ORDER BY started_at DESC",
            )?;
            let tunnels = stmt
                .query_map([], |row| {
                    Ok(Tunnel {
                        id: row.get(0)?,
                        instance_name: row.get(1)?,
                        remote_host: row.get(2)?,
                        remote_port: row.get(3)?,
                        local_port: row.get(4)?,
                        pid: row.get(5)?,
                        started_at: row.get(6)?,
                        mode: row.get(7)?,
                        open_url: row.get::<_, i32>(8)? != 0,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(tunnels)
        }
    }

    /// Update tunnel PID
    pub fn update_tunnel_pid(&self, id: i64, pid: Option<i32>) -> Result<()> {
        self.conn.execute("UPDATE tunnels SET pid = ?1 WHERE id = ?2", params![pid, id])?;
        Ok(())
    }

    /// Delete a tunnel
    pub fn delete_tunnel(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM tunnels WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Get active tunnels (with alive PIDs)
    pub fn get_active_tunnels(&self, instance_name: &str) -> Result<Vec<Tunnel>> {
        let tunnels = self.list_tunnels(Some(instance_name))?;
        Ok(tunnels
            .into_iter()
            .filter(|t| {
                if let Some(pid) = t.pid {
                    crate::util::is_process_alive(pid)
                } else {
                    false
                }
            })
            .collect())
    }
}
